//! Measure precision and recall on the labeled corpus in `tests/corpus`.
//!
//! Each `NAME.md` file has a `NAME.expected` file that lists the true
//! positives as `LINE CODE`. A `# mode: procedure` comment sets the mode.
//! Every reported finding that is not listed is a false positive, and every
//! listed entry that is not reported is a false negative.
//!
//! Run `cargo test corpus -- --nocapture` to print the table.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

use crate::{cli::Mode, config::Config, document::Document};

use super::{CODES, Filter, check};

/// The lowest precision and recall that any rule can have on the corpus.
const MIN_RULE_SCORE: f64 = 0.85;

#[derive(Default)]
struct Counts {
    true_positives: usize,
    false_positives: usize,
    false_negatives: usize,
}

impl Counts {
    fn precision(&self) -> f64 {
        ratio(
            self.true_positives,
            self.true_positives + self.false_positives,
        )
    }

    fn recall(&self) -> f64 {
        ratio(
            self.true_positives,
            self.true_positives + self.false_negatives,
        )
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        1.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn config(mode: Mode) -> Config {
    Config {
        max_sentence_words: match mode {
            Mode::Procedure => 20,
            Mode::Description => 25,
        },
        max_diagnostics: 10_000,
        mode,
        ..Config::default()
    }
}

/// Read `LINE CODE` entries and the mode from an expectations file.
fn expectations(path: &Path) -> (Mode, HashMap<(usize, String), usize>) {
    let text = fs::read_to_string(path).expect("expectations file is readable");
    let mut mode = Mode::Description;
    let mut expected = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(comment) = line.strip_prefix('#') {
            match comment.trim() {
                "mode: procedure" => mode = Mode::Procedure,
                "mode: description" => mode = Mode::Description,
                _ => {}
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let (number, code) = line
            .split_once(' ')
            .unwrap_or_else(|| panic!("bad entry {line:?} in {}", path.display()));
        let number = number.parse().expect("line number is an integer");
        *expected.entry((number, code.to_owned())).or_insert(0) += 1;
    }
    (mode, expected)
}

fn corpus_files() -> Vec<PathBuf> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut files: Vec<_> = fs::read_dir(&directory)
        .expect("corpus directory exists")
        .map(|entry| entry.expect("corpus entry is readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();
    files
}

#[test]
fn corpus_precision_and_recall() {
    let mut counts: BTreeMap<&str, Counts> = CODES
        .iter()
        .map(|code| (*code, Counts::default()))
        .collect();
    let mut mismatches = Vec::new();

    for path in corpus_files() {
        let (mode, mut expected) = expectations(&path.with_extension("expected"));
        let source = fs::read_to_string(&path).expect("corpus file is readable");
        let document = Document::new(source, true);
        let findings = check(&document, &config(mode), &Filter::all(), usize::MAX);
        let name = path.file_name().expect("file has a name").to_string_lossy();

        for finding in findings {
            let (line, _) = document.location(finding.range.start);
            let entry = counts.get_mut(finding.code).expect("code is known");
            match expected.get_mut(&(line, finding.code.to_owned())) {
                Some(remaining) if *remaining > 0 => {
                    *remaining -= 1;
                    entry.true_positives += 1;
                }
                _ => {
                    entry.false_positives += 1;
                    mismatches.push(format!("FP {name}:{line} {}", finding.code));
                }
            }
        }
        for ((line, code), remaining) in expected {
            for _ in 0..remaining {
                let entry = counts.get_mut(code.as_str()).expect("code is known");
                entry.false_negatives += 1;
                mismatches.push(format!("FN {name}:{line} {code}"));
            }
        }
    }

    mismatches.sort();
    println!("rule    TP  FP  FN  precision  recall");
    for (code, entry) in &counts {
        println!(
            "{code}  {:>3} {:>3} {:>3}  {:>9.2}  {:>6.2}",
            entry.true_positives,
            entry.false_positives,
            entry.false_negatives,
            entry.precision(),
            entry.recall()
        );
    }
    for mismatch in &mismatches {
        println!("{mismatch}");
    }

    for (code, entry) in &counts {
        assert!(
            entry.precision() >= MIN_RULE_SCORE && entry.recall() >= MIN_RULE_SCORE,
            "{code} is below {MIN_RULE_SCORE} precision or recall; mismatches: {mismatches:#?}"
        );
    }
}
