use std::sync::LazyLock;

use regex::Regex;

use crate::document::Document;

use super::{Confidence, Finding, Sink, next_word};

struct Phrase {
    regex: Regex,
    replacement: &'static str,
    /// Next words that make the phrase acceptable.
    allowed_before: &'static [&'static str],
}

/// Units that make `a number of` a literal quantity, as in `a number of
/// seconds since the epoch`.
const UNITS: &[&str] = &[
    "bits",
    "bytes",
    "kilobytes",
    "megabytes",
    "gigabytes",
    "characters",
    "nanoseconds",
    "microseconds",
    "milliseconds",
    "seconds",
    "minutes",
    "hours",
    "days",
    "weeks",
    "months",
    "years",
    "ticks",
    "cycles",
];

type PhraseSpec = (&'static str, &'static str, &'static [&'static str]);

static PHRASES: LazyLock<Vec<Phrase>> = LazyLock::new(|| {
    let specs: [PhraseSpec; 10] = [
        (r"(?i)\bin order to\b", "to", &[]),
        (r"(?i)\bdue to the fact that\b", "because", &[]),
        (r"(?i)\ba number of\b", "several or an exact number", UNITS),
        (r"(?i)\butili[sz]e(?:d|s|ing)?\b", "use", &[]),
        (r"(?i)\bprior to\b", "before", &[]),
        (r"(?i)\bsubsequent to\b", "after", &[]),
        (
            r"(?i)\b(?:obviously|clearly|simply|basically)\b",
            "a specific explanation",
            &[],
        ),
        (
            r"(?i)\b(?:seamless|robust|crucial)\b",
            "a precise, measurable term",
            &[],
        ),
        (r"(?i)\b(?:delve|delves|delved|delving)\b", "examine", &[]),
        (
            r"(?i)\b(?:moreover|additionally)\b",
            "a direct transition or no transition",
            &[],
        ),
    ];
    specs
        .into_iter()
        .map(|(pattern, replacement, allowed_before)| Phrase {
            regex: Regex::new(pattern).expect("built-in phrase pattern is valid"),
            replacement,
            allowed_before,
        })
        .collect()
});

pub fn check(document: &Document, sink: &mut Sink) {
    let text = document.visible();
    for phrase in PHRASES.iter() {
        for matched in phrase.regex.find_iter(text) {
            if next_word(&text[matched.end()..]).is_some_and(|word| {
                phrase
                    .allowed_before
                    .iter()
                    .any(|allowed| word.eq_ignore_ascii_case(allowed))
            }) {
                continue;
            }
            if !sink.push(Finding {
                range: matched.range(),
                code: "STE006",
                confidence: Confidence::High,
                message: format!("imprecise phrase {:?}", matched.as_str()),
                help: Some(format!("Try {:?} instead.", phrase.replacement)),
            }) {
                return;
            }
        }
    }

    for (offset, _) in text.match_indices(';') {
        if !joins_clauses(text, offset) {
            continue;
        }
        if !sink.push(Finding {
            range: offset..offset + 1,
            code: "STE007",
            confidence: Confidence::Medium,
            message: "semicolon can join multiple instructions or topics".to_owned(),
            help: Some("Use separate sentences or a vertical list.".to_owned()),
        }) {
            return;
        }
    }
}

/// True when the semicolon at `offset` sits between two clauses of prose.
///
/// A word must end right before the semicolon, and whitespace and a word must
/// follow it. This skips `TL;DR`, HTML entities such as `&mdash;`, and list
/// items or code that end in a semicolon.
fn joins_clauses(text: &str, offset: usize) -> bool {
    let before = &text[..offset];
    let after = &text[offset + 1..];
    let word_before = before.chars().next_back().is_some_and(|character| {
        character.is_alphanumeric()
            || matches!(character, ')' | '"' | '\'' | '\u{2019}' | '\u{201d}')
    });
    let entity = before
        .rfind(|character: char| !(character.is_ascii_alphanumeric() || character == '#'))
        .is_some_and(|start| before[start..].starts_with('&') && before.len() - start > 1);
    let spaced = after.starts_with(char::is_whitespace);
    let word_after = after
        .trim_start()
        .chars()
        .next()
        .is_some_and(char::is_alphabetic);
    word_before && !entity && spaced && word_after
}
