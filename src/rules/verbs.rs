use std::{ops::Range, sync::LazyLock};

use regex::Regex;

use crate::document::Document;

use super::{Finding, push_bounded};

static PASSIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:am|are|is|was|were|be|been|being)\s+(?:[[:alpha:]-]+\s+){0,2}[[:alpha:]-]+(?:ed|en)\b")
        .expect("built-in passive voice pattern is valid")
});
static COMPLEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:(?:has|have|had)\s+(?:been\s+)?[[:alpha:]-]+(?:ed|en)|(?:will|would|could|should|may|might|must)\s+have\s+[[:alpha:]-]+(?:ed|en))\b")
        .expect("built-in complex verb pattern is valid")
});
/// The candidate `-ing` word, as capture group 1.
const ING_WORD: &str = r"([[:alpha:]][[:alpha:]-]*ing)";
const AUXILIARIES: &str = "am|are|is|was|were|be|been|being|has been|have been|had been";
const PREPOSITIONS: &str =
    "by|for|of|in|on|without|after|before|while|when|from|about|during|instead of|prior to";
const DETERMINERS: &str =
    "a|an|the|this|that|these|those|its|our|their|your|my|his|her|any|each|every|no|some";

static ING_PROGRESSIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:{AUXILIARIES})\s+(?:[[:alpha:]]+ly\s+)?{ING_WORD}\b"
    ))
    .expect("built-in progressive pattern is valid")
});
static ING_PREPOSITIONAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?i)\b(?:{PREPOSITIONS})\s+{ING_WORD}\b"))
        .expect("built-in prepositional gerund pattern is valid")
});
static ING_SUBJECT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:^|[.;:]\s+){ING_WORD}\s+(?:{DETERMINERS})\b"
    ))
    .expect("built-in gerund subject pattern is valid")
});
static ING_OPENER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?:^|[.;:]\s+){ING_WORD}\s+(?:{DETERMINERS}|it|them|him|us|me|you)\b[^.;:]*?,"
    ))
    .expect("built-in participial opener pattern is valid")
});
static PRONOUN_SUBJECT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:we|they|you|i|he|she)\s+(?:am|are|is|was|were)\s+(?:[[:alpha:]]+ly\s+)?$")
        .expect("built-in pronoun subject pattern is valid")
});

/// Objects that show the `-ing` word is a verb rather than a noun modifier.
const OBJECT_WORDS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "it", "them", "him", "us", "me", "you",
    "to",
];

/// Words that end a complement, as in `the permitted values are string and int`.
const CONNECTIVES: &[&str] = &["and", "or", "but", "nor", "than", "then"];

/// Indefinite pronouns end in `-ing` but are never verb forms.
const INDEFINITE_PRONOUNS: &[&str] = &["nothing", "something", "anything", "everything"];

pub fn check(document: &Document, findings: &mut Vec<Finding>, limit: usize) {
    for matched in PASSIVE.find_iter(document.visible()) {
        if document.overlaps_technical(&matched.range()) {
            continue;
        }
        if !push_bounded(
            findings,
            Finding {
                range: matched.range(),
                code: "STE003",
                message: "possible passive voice".to_owned(),
                help: Some(
                    "Name the actor and use active voice when the actor is known.".to_owned(),
                ),
            },
            limit,
        ) {
            return;
        }
    }

    for matched in COMPLEX.find_iter(document.visible()) {
        if document.overlaps_technical(&matched.range()) {
            continue;
        }
        if !push_bounded(
            findings,
            Finding {
                range: matched.range(),
                code: "STE004",
                message: "possible complex verb construction".to_owned(),
                help: Some("Use a simple present, past, or future verb form.".to_owned()),
            },
            limit,
        ) {
            return;
        }
    }

    for range in ing_candidates(document.visible()) {
        if document.in_technical_token(&range) {
            continue;
        }
        if !push_bounded(
            findings,
            Finding {
                range: range.clone(),
                code: "STE005",
                message: format!("review the -ing form {:?}", &document.visible()[range]),
                help: Some(
                    "Use a simple verb form when the actor or the action is the point.".to_owned(),
                ),
            },
            limit,
        ) {
            return;
        }
    }
}

/// Collect spans where a closed-class word puts an `-ing` word in a verb slot.
///
/// The frames replace a plain suffix match, so technical nouns such as
/// `string`, `mapping`, and `padding` are not reported just for ending in
/// `-ing`. A word in a noun position is only reported when an auxiliary or a
/// preposition makes the verb reading the likely one.
fn ing_candidates(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();

    for captures in ING_PROGRESSIVE.captures_iter(text) {
        let Some(word) = captures.get(1) else {
            continue;
        };
        let rest = &text[word.end()..];
        let clause_final = ends_clause(rest);
        let connective = starts_connective(rest);
        // A following word means the `-ing` form takes an object or is followed
        // by a bare noun, so the verb reading wins. Compare `is logging events`
        // with `is string` or `are string and int`.
        let verb_slot = takes_object(rest) || (!clause_final && !connective);
        let pronoun = pronoun_subject(&text[..word.start()]);
        if verb_slot || ((clause_final || connective) && pronoun) {
            push_candidate(text, word.range(), &mut ranges);
        }
    }

    for captures in ING_PREPOSITIONAL.captures_iter(text) {
        let Some(word) = captures.get(1) else {
            continue;
        };
        let rest = &text[word.end()..];
        // A preposition followed by an `-ing` word alone is ambiguous:
        // `by logging` can be a gerund, while `in string, numeric, ...` is a
        // list of nouns. Require a determiner or pronoun before the object.
        if takes_object(rest) {
            push_candidate(text, word.range(), &mut ranges);
        }
    }

    for pattern in [&ING_SUBJECT, &ING_OPENER] {
        for captures in pattern.captures_iter(text) {
            if let Some(word) = captures.get(1) {
                push_candidate(text, word.range(), &mut ranges);
            }
        }
    }

    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    ranges.dedup();
    ranges
}

fn push_candidate(text: &str, range: Range<usize>, ranges: &mut Vec<Range<usize>>) {
    let word = &text[range.clone()];
    if INDEFINITE_PRONOUNS
        .iter()
        .any(|pronoun| word.eq_ignore_ascii_case(pronoun))
    {
        return;
    }
    ranges.push(range);
}

fn takes_object(rest: &str) -> bool {
    following_word(rest).is_some_and(|word| {
        OBJECT_WORDS
            .iter()
            .any(|object| word.eq_ignore_ascii_case(object))
    })
}

fn following_word(rest: &str) -> Option<&str> {
    let trimmed = rest.trim_start_matches(char::is_whitespace);
    let end = trimmed
        .char_indices()
        .find(|(_, character)| !(character.is_alphabetic() || *character == '-'))
        .map_or(trimmed.len(), |(offset, _)| offset);
    (end > 0).then(|| &trimmed[..end])
}

fn ends_clause(rest: &str) -> bool {
    let trimmed = rest.trim_start_matches(char::is_whitespace);
    trimmed.is_empty()
        || trimmed.starts_with(['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\''])
}

fn starts_connective(rest: &str) -> bool {
    following_word(rest).is_some_and(|word| {
        CONNECTIVES
            .iter()
            .any(|connective| word.eq_ignore_ascii_case(connective))
    })
}

fn pronoun_subject(prefix: &str) -> bool {
    PRONOUN_SUBJECT.is_match(prefix)
}
