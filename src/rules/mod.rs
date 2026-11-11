#[cfg(test)]
mod corpus;
mod length;
mod style;
mod verbs;

use std::{collections::HashSet, ops::Range};

use serde::{Deserialize, Serialize};

use crate::{config::Config, diagnostic::Diagnostic, document::Document};

const MAX_SOURCE_LINE_CHARS: usize = 240;

/// Every rule code, in output order.
pub const CODES: [&str; 7] = [
    "STE001", "STE002", "STE003", "STE004", "STE005", "STE006", "STE007",
];

/// How likely a finding is to be a real problem.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug)]
pub struct Finding {
    pub range: Range<usize>,
    pub code: &'static str,
    pub confidence: Confidence,
    pub message: String,
    pub help: Option<String>,
}

/// Decide which findings a file reports.
#[derive(Debug, Clone)]
pub struct Filter {
    pub enabled: HashSet<&'static str>,
    pub min_confidence: Confidence,
}

impl Filter {
    #[cfg(test)]
    pub fn all() -> Self {
        Self {
            enabled: CODES.into_iter().collect(),
            min_confidence: Confidence::Low,
        }
    }

    pub fn is_enabled(&self, code: &str) -> bool {
        self.enabled.contains(code)
    }
}

/// Collect findings up to a limit, after filters and inline directives.
///
/// Filtering happens here, before the limit applies, so suppressed findings
/// never take the place of reported ones.
pub struct Sink<'a> {
    document: &'a Document,
    filter: &'a Filter,
    limit: usize,
    findings: Vec<Finding>,
}

impl Sink<'_> {
    /// Add a finding. Return false when the sink is full.
    pub fn push(&mut self, finding: Finding) -> bool {
        if !self.filter.is_enabled(finding.code)
            || finding.confidence < self.filter.min_confidence
            || self
                .document
                .is_suppressed(finding.range.start, finding.code)
        {
            return true;
        }
        if self.findings.len() >= self.limit {
            return false;
        }
        self.findings.push(finding);
        true
    }
}

pub fn check(
    document: &Document,
    config: &Config,
    filter: &Filter,
    remaining: usize,
) -> Vec<Finding> {
    let mut sink = Sink {
        document,
        filter,
        limit: remaining,
        findings: Vec::new(),
    };
    length::check(document, config, &mut sink);
    style::check(document, &mut sink);
    verbs::check(document, config, &mut sink);
    let mut findings = sink.findings;
    findings.sort_by_key(|finding| (finding.range.start, finding.code));
    findings.truncate(remaining);
    findings
}

pub fn into_diagnostic(path: &str, document: &Document, finding: Finding) -> Diagnostic {
    let (line, column) = document.location(finding.range.start);
    let (end_line, end_column) = document.location(finding.range.end);
    let source_line = truncate_source_line(document.source_excerpt(&finding.range));
    Diagnostic {
        path: path.to_owned(),
        line,
        column,
        end_column: if end_line == line {
            end_column.max(column + 1)
        } else {
            column + 1
        },
        source_line,
        severity: crate::diagnostic::Severity::Warning,
        confidence: finding.confidence,
        code: finding.code,
        message: finding.message,
        help: finding.help,
    }
}

/// Return the word at the start of `rest`, after any whitespace.
fn next_word(rest: &str) -> Option<&str> {
    let trimmed = rest.trim_start();
    let end = trimmed
        .find(|character: char| !(character.is_alphabetic() || character == '-'))
        .unwrap_or(trimmed.len());
    (end > 0).then(|| &trimmed[..end])
}

fn truncate_source_line(line: &str) -> String {
    let mut characters = line.chars();
    let mut rendered: String = characters.by_ref().take(MAX_SOURCE_LINE_CHARS).collect();
    if characters.next().is_some() {
        rendered.push_str("...");
    }
    rendered
}

#[cfg(test)]
mod tests {
    use crate::{cli::Mode, config::Config, document::Document};

    use super::{Confidence, Filter, check as check_with};

    fn check(document: &Document, config: &Config, remaining: usize) -> Vec<super::Finding> {
        check_with(document, config, &Filter::all(), remaining)
    }

    fn config() -> Config {
        Config {
            max_sentence_words: 5,
            max_paragraph_sentences: 2,
            ..Config::default()
        }
    }

    #[test]
    fn reports_representative_ste_guidance() {
        let text = "The robust valve has been opened by using the procedure; clearly, it works. One. Two. Three.";
        let document = Document::new(text.to_owned(), false);
        let findings = check(&document, &config(), 100);
        let codes: Vec<_> = findings.iter().map(|finding| finding.code).collect();

        for expected in [
            "STE001", "STE002", "STE003", "STE004", "STE005", "STE006", "STE007",
        ] {
            assert!(codes.contains(&expected), "missing {expected}: {codes:?}");
        }
    }

    #[test]
    fn bounds_findings() {
        let document = Document::new("Clearly. Clearly. Clearly.".to_owned(), false);
        assert_eq!(check(&document, &config(), 2).len(), 2);
    }

    #[test]
    fn bounds_rendered_source_lines() {
        let text = format!("Clearly {}.", "word ".repeat(100));
        let document = Document::new(text, false);
        let finding = check(&document, &config(), 1).remove(0);
        let diagnostic = super::into_diagnostic("test.txt", &document, finding);

        assert_eq!(diagnostic.source_line.chars().count(), 243);
        assert!(diagnostic.source_line.ends_with("..."));
    }

    #[test]
    fn includes_all_lines_of_a_multiline_sentence() {
        let document = Document::new("One two three\nfour five six.".to_owned(), false);
        let finding = check(&document, &config(), 1).remove(0);
        let diagnostic = super::into_diagnostic("test.txt", &document, finding);

        assert_eq!(diagnostic.source_line, "One two three\nfour five six.");
    }

    #[test]
    fn does_not_treat_a_dotted_filename_as_an_ing_form() {
        let document = Document::new("Edit CONTRIBUTING.md.".to_owned(), false);
        let findings = check(&document, &config(), 100);

        assert!(!findings.iter().any(|finding| finding.code == "STE005"));
    }

    #[test]
    fn skips_ing_forms_inside_technical_tokens() {
        let text = "Read CONTRIBUTING.md. Open src/logging/mod.rs. Call logging::init. \
                    Set logging_enabled. Fetch https://example.com/logging. Install @scope/logging. \
                    Read logging.rs and build-logging.sh.";
        let document = Document::new(text.to_owned(), false);
        let findings = check(&document, &config(), 100);
        let ing: Vec<_> = findings
            .iter()
            .filter(|finding| finding.code == "STE005")
            .map(|finding| &text[finding.range.clone()])
            .collect();

        assert!(ing.is_empty(), "unexpected -ing findings: {ing:?}");
    }

    #[test]
    fn flags_a_sentence_final_ing_form() {
        let document = Document::new("We are testing.".to_owned(), false);
        let findings = check(&document, &config(), 100);

        assert!(findings.iter().any(|finding| finding.code == "STE005"));
    }

    #[test]
    fn flags_verb_slot_ing_forms() {
        for text in [
            "The system is logging events.",
            "We are testing.",
            "Open the file by using the menu.",
            "Improve the result by logging the events.",
            "Stop the engine before opening the valve.",
            "Testing the valve takes time.",
            "Using the API, call the endpoint.",
            "The valve is opening slowly.",
            "The tool has been running for hours.",
        ] {
            let document = Document::new(text.to_owned(), false);
            let findings = check(&document, &config(), 100);

            assert!(
                findings.iter().any(|finding| finding.code == "STE005"),
                "missing STE005 in {text:?}"
            );
        }
    }

    #[test]
    fn skips_ing_words_in_noun_slots() {
        for text in [
            "The type is string.",
            "A string is required.",
            "The value is a string.",
            "Update the mapping table.",
            "The logging mode is off.",
            "Check the padding value.",
            "Set the encoding to utf-8.",
            "The meeting starts at noon.",
            "The building is closed.",
            "Read the warning label.",
            "The spring is loose.",
            "Nothing changed.",
            "Store the binding in the cache.",
            "The ceiling panel is open.",
            "Processing occurs in batches.",
            "The casing is steel.",
            "This is a smoking area.",
            "The permitted values are string and int.",
            "The default type is string or int.",
            "The value is string.",
            "Nothing, really, works here.",
            "Warning labels, then stop.",
            "Morning, noon, and night are shift names.",
            "Mapping entries, then save.",
            "There is nothing to configure.",
            "It is nothing to worry about.",
            "In string, the backslash is removed.",
            "For string, numeric, and file comparisons.",
            "Please contact us in writing.",
        ] {
            let document = Document::new(text.to_owned(), false);
            let findings = check(&document, &config(), 100);

            assert!(
                !findings.iter().any(|finding| finding.code == "STE005"),
                "unexpected STE005 in {text:?}"
            );
        }
    }

    #[test]
    fn does_not_treat_a_wrapped_line_start_as_a_clause_start() {
        let text = "The grammar checks are conservative heuristics. Treat each\nwarning as a prompt for review, not proof of an error.";
        let document = Document::new(text.to_owned(), false);
        let findings = check(&document, &config(), 100);

        assert!(!findings.iter().any(|finding| finding.code == "STE005"));
    }

    #[test]
    fn reports_an_ambiguous_copula_ing_form_for_review() {
        // `reads are blocking operations` and `the system is logging events`
        // have the same shape. English cannot separate the adjective reading
        // from the progressive reading without a lexicon, so the frame reports
        // the word for review instead of guessing.
        let document = Document::new("Local reads are blocking operations.".to_owned(), false);
        let findings = check(&document, &config(), 100);

        assert!(findings.iter().any(|finding| finding.code == "STE005"));
    }

    #[test]
    fn skips_a_clause_final_progressive_without_a_pronoun_subject() {
        let document = Document::new("The valve is opening.".to_owned(), false);
        let findings = check(&document, &config(), 100);

        assert!(!findings.iter().any(|finding| finding.code == "STE005"));
    }

    #[test]
    fn skips_verb_findings_inside_technical_tokens() {
        for text in [
            "The value is cached.data here.",
            "The job is queued.json here.",
            "The build has been cached.now",
        ] {
            let document = Document::new(text.to_owned(), false);
            let findings = check(&document, &config(), 100);

            assert!(
                !findings
                    .iter()
                    .any(|finding| finding.code == "STE003" || finding.code == "STE004"),
                "unexpected verb finding in {text:?}"
            );
        }
    }

    #[test]
    fn keeps_verb_findings_in_prose() {
        let document = Document::new("The valve is opened by the system.".to_owned(), false);
        let findings = check(&document, &config(), 100);

        assert!(findings.iter().any(|finding| finding.code == "STE003"));
    }

    fn findings_for(text: &str, code: &str) -> Vec<String> {
        let document = Document::new(text.to_owned(), false);
        check(&document, &config(), 100)
            .into_iter()
            .filter(|finding| finding.code == code)
            .map(|finding| text[finding.range].to_owned())
            .collect()
    }

    #[test]
    fn skips_non_passive_be_phrases() {
        for text in [
            "It is a hidden field.",
            "It is the token.",
            "This is open source.",
            "It is even faster now.",
            "The value is between one and ten.",
            "The goal is speed when it counts.",
            "The API is deprecated.",
            "The value is unchanged.",
            "Rust's UTF-8 is guaranteed-valid.",
        ] {
            assert!(
                findings_for(text, "STE003").is_empty(),
                "unexpected STE003 in {text:?}"
            );
        }
    }

    #[test]
    fn flags_passives_with_gaps_and_irregular_participles() {
        for (text, span) in [
            ("The flag is set by the caller.", "is set"),
            ("The binary is built nightly.", "is built"),
            ("The test is run nightly.", "is run"),
            ("A copy is made first.", "is made"),
            ("The result is then returned.", "is then returned"),
            ("It is being targeted.", "is being targeted"),
            (
                "They are all manually controlled.",
                "are all manually controlled",
            ),
            ("It should be dual licensed.", "be dual licensed"),
            ("The value isn't used.", "isn't used"),
            ("The file was rewritten.", "was rewritten"),
        ] {
            assert_eq!(findings_for(text, "STE003"), [span], "in {text:?}");
        }
    }

    #[test]
    fn rates_passive_confidence_by_agent_and_mode() {
        let rate = |text: &str, mode: Mode| {
            let document = Document::new(text.to_owned(), false);
            let mut settings = config();
            settings.mode = mode;
            check(&document, &settings, 100)
                .into_iter()
                .find(|finding| finding.code == "STE003")
                .map(|finding| finding.confidence)
        };
        assert_eq!(
            rate("The cache is cleared by the scheduler.", Mode::Description),
            Some(Confidence::High)
        );
        assert_eq!(
            rate("It is used by default.", Mode::Description),
            Some(Confidence::Low)
        );
        assert_eq!(
            rate("It is used by default.", Mode::Procedure),
            Some(Confidence::Medium)
        );
    }

    #[test]
    fn separates_perfect_verbs_from_noun_phrases() {
        for text in [
            "It has read access.",
            "We have mixed results.",
            "It has a broken link.",
        ] {
            assert!(
                findings_for(text, "STE004").is_empty(),
                "unexpected STE004 in {text:?}"
            );
        }
        for (text, span) in [
            ("It has read the file.", "has read"),
            ("There have been bugs.", "have been"),
            ("It has not been tuned.", "has not been tuned"),
            ("We haven't verified it.", "haven't verified"),
        ] {
            assert_eq!(findings_for(text, "STE004"), [span], "in {text:?}");
        }
    }

    #[test]
    fn skips_a_number_of_with_a_unit() {
        assert!(findings_for("Wait a number of seconds.", "STE006").is_empty());
        assert_eq!(
            findings_for("A number of users asked.", "STE006"),
            ["A number of"]
        );
    }

    #[test]
    fn reports_only_semicolons_that_join_clauses() {
        for text in [
            "TL;DR: it works.",
            "Use it &mdash; now.",
            "- Set the flag;\n- Run the tool.",
            "Run x = 1;",
        ] {
            assert!(
                findings_for(text, "STE007").is_empty(),
                "unexpected STE007 in {text:?}"
            );
        }
        assert_eq!(findings_for("Stop; then start.", "STE007"), [";"]);
    }

    #[test]
    fn applies_rule_selection_and_confidence() {
        let text = "Clearly, it is used; stop.";
        let document = Document::new(text.to_owned(), false);
        let codes = |filter: &Filter| {
            check_with(&document, &config(), filter, 100)
                .into_iter()
                .map(|finding| finding.code)
                .collect::<Vec<_>>()
        };

        assert_eq!(codes(&Filter::all()), ["STE006", "STE003", "STE007"]);
        let mut filter = Filter::all();
        filter.enabled.remove("STE006");
        assert_eq!(codes(&filter), ["STE003", "STE007"]);
        filter.min_confidence = Confidence::Medium;
        assert_eq!(codes(&filter), ["STE007"]);
    }

    #[test]
    fn suppressed_findings_do_not_use_the_limit() {
        let text = "<!-- ste-lint-disable STE006 -->\nClearly. Clearly.\n<!-- ste-lint-enable -->\nStop; then start.";
        let document = Document::new(text.to_owned(), false);
        let findings = check(&document, &config(), 1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "STE007");
    }
}
