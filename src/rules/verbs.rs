use std::{ops::Range, sync::LazyLock};

use regex::Regex;

use crate::{cli::Mode, config::Config, document::Document};

use super::{Confidence, Finding, Sink, next_word};

static AUXILIARY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:am|are|is|was|were|be|been|being)(?:n['’]t)?\b")
        .expect("built-in auxiliary pattern is valid")
});
static PERFECT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:(?:has|have|had)(?:n['’]t)?|(?:will|would|could|should|may|might|must)(?:n['’]t)?(?:\s+not)?\s+have)\b",
    )
    .expect("built-in perfect pattern is valid")
});

/// Words that can stand between an auxiliary and a participle, as in
/// `is not used`, `is then returned`, or `are all manually controlled`.
const GAP_WORDS: &[&str] = &[
    "not",
    "also",
    "often",
    "then",
    "now",
    "already",
    "still",
    "always",
    "never",
    "only",
    "just",
    "even",
    "further",
    "thus",
    "therefore",
    "first",
    "later",
    "again",
    "all",
    "both",
    "each",
    "either",
    "dual",
    "being",
    "no",
    "longer",
    "sometimes",
    "very",
    "well",
    "much",
    "best",
    "almost",
    "yet",
    "ever",
    "once",
];

/// Words that end in `-ly` but are not adverbs.
const NOT_ADVERBS: &[&str] = &[
    "apply", "reply", "supply", "family", "assembly", "ally", "rally", "jelly", "july", "italy",
    "anomaly", "monopoly",
];

/// Words that end in `-ed` but are not past participles.
const NOT_PARTICIPLES: &[&str] = &[
    "need", "seed", "speed", "feed", "shed", "embed", "hundred", "indeed", "proceed", "exceed",
    "succeed", "breed", "bleed", "deed", "weed", "sled", "sacred", "naked", "wicked", "rugged",
    "ragged", "kindred", "shred", "hatred", "infrared", "steed",
];

/// Participle forms that usually describe a state, not an action, after `be`.
const STATIVE_ADJECTIVES: &[&str] = &[
    "advanced",
    "aged",
    "balanced",
    "based",
    "biased",
    "complicated",
    "concerned",
    "crowded",
    "dedicated",
    "deprecated",
    "detailed",
    "excited",
    "experienced",
    "interested",
    "involved",
    "limited",
    "mixed",
    "outdated",
    "outspoken",
    "pleased",
    "related",
    "skilled",
    "sophisticated",
    "supposed",
    "talented",
    "tired",
    "varied",
    "unaffected",
    "unbroken",
    "unchanged",
    "undefined",
    "unexpected",
    "uninitialized",
    "unknown",
    "unlimited",
    "unmaintained",
    "unmodified",
    "unneeded",
    "unpublished",
    "unrelated",
    "unseen",
    "unsigned",
    "unspecified",
    "unsupported",
    "unused",
    "unwanted",
];

/// Irregular past participles that do not end in `-ed` and are too short to
/// match as suffixes.
const IRREGULAR_PARTICIPLES: &[&str] = &[
    "set",
    "reset",
    "run",
    "rerun",
    "put",
    "cut",
    "hit",
    "read",
    "led",
    "fed",
    "won",
    "made",
    "remade",
    "sent",
    "kept",
    "held",
    "withheld",
    "upheld",
    "found",
    "bound",
    "built",
    "rebuilt",
    "lost",
    "left",
    "paid",
    "laid",
    "told",
    "sold",
    "meant",
    "spent",
    "lent",
    "bent",
    "dealt",
    "felt",
    "heard",
    "hung",
    "shot",
    "shut",
    "split",
    "spread",
    "struck",
    "stuck",
    "spun",
    "sought",
    "thought",
    "brought",
    "bought",
    "caught",
    "taught",
    "understood",
    "born",
    "borne",
    "begun",
    "sung",
    "rung",
    "swung",
    "cast",
    "broadcast",
    "fled",
    "slid",
    "lit",
];

/// Irregular participles that also match as the end of a longer form, as in
/// `rewritten`, `undertaken`, or `withdrawn`.
const IRREGULAR_SUFFIXES: &[&str] = &[
    "written", "taken", "given", "seen", "chosen", "driven", "hidden", "broken", "gotten",
    "frozen", "stolen", "spoken", "woken", "eaten", "fallen", "beaten", "bitten", "ridden",
    "risen", "shaken", "proven", "bidden", "sworn", "worn", "torn", "drawn", "grown", "thrown",
    "shown", "known", "blown", "flown", "done",
];

/// Participles that are also common nouns, as in `has read access`.
const NOUN_HOMOGRAPHS: &[&str] = &[
    "read",
    "set",
    "run",
    "cut",
    "put",
    "hit",
    "spread",
    "split",
    "shut",
    "cast",
    "bound",
    "left",
    "lost",
    "found",
    "broadcast",
];

/// Words after `by` that do not name an actor.
const NOT_AGENTS: &[&str] = &[
    "default",
    "hand",
    "design",
    "name",
    "value",
    "reference",
    "itself",
    "themselves",
    "convention",
    "contrast",
    "comparison",
    "far",
    "now",
    "then",
    "accident",
    "chance",
    "using",
    "means",
    "way",
];

/// Words that show a perfect form with a noun homograph is a verb.
const VERB_FOLLOWERS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "it", "them", "him", "us", "me", "you",
    "to", "in", "on", "by", "for", "from", "with", "into", "at", "up", "out", "off", "as",
];

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

/// Words that end in `-ing` but are never a verb form to review. Indefinite
/// pronouns are not verbs, and `being` is an auxiliary that STE003 reports.
const NOT_ING_FORMS: &[&str] = &["nothing", "something", "anything", "everything", "being"];

pub fn check(document: &Document, config: &Config, sink: &mut Sink) {
    let text = document.visible();

    // An auxiliary inside a reported span, such as `being` in
    // `is being used`, must not start a second report.
    let mut covered = 0;
    for auxiliary in AUXILIARY.find_iter(text) {
        if auxiliary.start() < covered {
            continue;
        }
        let Some(participle) = participle_after(text, auxiliary.end()) else {
            continue;
        };
        let range = auxiliary.start()..participle.end;
        covered = range.end;
        if document.overlaps_technical(&range) {
            continue;
        }
        let confidence = if names_agent(&text[range.end..]) {
            Confidence::High
        } else if matches!(config.mode, Mode::Procedure) {
            Confidence::Medium
        } else {
            // ASD-STE100 permits passive voice in descriptive text when the
            // actor is not known or not relevant.
            Confidence::Low
        };
        if !sink.push(Finding {
            range,
            code: "STE003",
            confidence,
            message: "possible passive voice".to_owned(),
            help: Some("Name the actor and use active voice when the actor is known.".to_owned()),
        }) {
            return;
        }
    }

    for head in PERFECT.find_iter(text) {
        let Some(participle) = perfect_participle(text, head.end()) else {
            continue;
        };
        let range = head.start()..participle.end;
        if document.overlaps_technical(&range) {
            continue;
        }
        if !sink.push(Finding {
            range,
            code: "STE004",
            confidence: Confidence::Medium,
            message: "possible complex verb construction".to_owned(),
            help: Some("Use a simple present, past, or future verb form.".to_owned()),
        }) {
            return;
        }
    }

    for (range, confidence) in ing_candidates(text) {
        if document.in_technical_token(&range) {
            continue;
        }
        if !sink.push(Finding {
            range: range.clone(),
            code: "STE005",
            confidence,
            message: format!("review the -ing form {:?}", &text[range]),
            help: Some(
                "Use a simple verb form when the actor or the action is the point.".to_owned(),
            ),
        }) {
            return;
        }
    }
}

/// Find a past participle up to two gap words after an auxiliary.
///
/// Only adverbs and a few quantifiers can stand in the gap, so `is a hidden
/// field` and `is the token` are not passive.
fn participle_after(text: &str, from: usize) -> Option<Range<usize>> {
    let mut offset = from;
    for _ in 0..3 {
        let word = spaced_word(text, offset)?;
        let candidate = &text[word.clone()];
        if is_participle(candidate) {
            return Some(word);
        }
        if !is_gap_word(candidate) {
            return None;
        }
        offset = word.end;
    }
    None
}

/// Find `been` or a past participle after `has`, `have`, `had`, or a modal
/// with `have`. One adverb can stand between them.
fn perfect_participle(text: &str, from: usize) -> Option<Range<usize>> {
    let mut offset = from;
    for _ in 0..2 {
        let word = spaced_word(text, offset)?;
        let candidate = &text[word.clone()];
        if candidate.eq_ignore_ascii_case("been") {
            // Report `has been opened` as one span, so a technical token
            // such as `cached.now` after `been` still skips the finding.
            return Some(
                participle_after(text, word.end).map_or(word.clone(), |next| word.start..next.end),
            );
        }
        if is_participle(candidate) {
            let homograph = NOUN_HOMOGRAPHS
                .iter()
                .any(|noun| candidate.eq_ignore_ascii_case(noun));
            // `has set the flag` is a verb. `has read access` is a noun.
            let verb = !homograph
                || ends_clause(&text[word.end..])
                || spaced_word(text, word.end).is_some_and(|next| {
                    VERB_FOLLOWERS
                        .iter()
                        .any(|follower| text[next.clone()].eq_ignore_ascii_case(follower))
                });
            return verb.then_some(word);
        }
        if !is_gap_word(candidate) {
            return None;
        }
        offset = word.end;
    }
    None
}

/// Return the word that follows `from` after at least one whitespace
/// character. Punctuation between the words ends the search.
fn spaced_word(text: &str, from: usize) -> Option<Range<usize>> {
    let rest = &text[from..];
    let start = from + rest.len() - rest.trim_start().len();
    if start == from {
        return None;
    }
    let length = text[start..]
        .find(|character: char| !(character.is_alphabetic() || character == '-'))
        .unwrap_or(text.len() - start);
    (length > 0).then(|| start..start + length)
}

fn is_gap_word(word: &str) -> bool {
    let lower = word.to_lowercase();
    GAP_WORDS.contains(&lower.as_str())
        || (lower.len() > 3 && lower.ends_with("ly") && !NOT_ADVERBS.contains(&lower.as_str()))
}

fn is_participle(word: &str) -> bool {
    let lower = word.to_lowercase();
    let lower = lower.as_str();
    if STATIVE_ADJECTIVES.contains(&lower) || lower.starts_with("well-") {
        return false;
    }
    if lower.len() >= 4 && lower.ends_with("ed") && !NOT_PARTICIPLES.contains(&lower) {
        return true;
    }
    IRREGULAR_PARTICIPLES.contains(&lower)
        || IRREGULAR_SUFFIXES
            .iter()
            .any(|suffix| lower.ends_with(suffix))
}

/// True when a `by` phrase that names an actor follows within the clause.
fn names_agent(rest: &str) -> bool {
    let clause_end = rest
        .find(['.', ',', ';', ':', '!', '?', '(', ')'])
        .unwrap_or(rest.len());
    let mut words = rest[..clause_end].split_whitespace().take(6);
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("by") {
            return words.next().is_some_and(|actor| {
                let actor = actor.trim_matches(|character: char| !character.is_alphabetic());
                !actor.is_empty()
                    && !NOT_AGENTS
                        .iter()
                        .any(|word| actor.eq_ignore_ascii_case(word))
            });
        }
    }
    false
}

/// Collect spans where a closed-class word puts an `-ing` word in a verb slot.
///
/// The frames replace a plain suffix match, so technical nouns such as
/// `string`, `mapping`, and `padding` are not reported just for ending in
/// `-ing`. A word in a noun position is only reported when an auxiliary or a
/// preposition makes the verb reading the likely one.
fn ing_candidates(text: &str) -> Vec<(Range<usize>, Confidence)> {
    let mut ranges = Vec::new();

    for captures in ING_PROGRESSIVE.captures_iter(text) {
        let Some(word) = captures.get(1) else {
            continue;
        };
        let rest = &text[word.end()..];
        let clause_final = ends_clause(rest);
        let connective = starts_connective(rest);
        let object = takes_object(rest);
        let pronoun = pronoun_subject(&text[..word.start()]);
        if object || ((clause_final || connective) && pronoun) {
            push_candidate(text, word.range(), Confidence::Medium, &mut ranges);
        } else if !clause_final && !connective {
            // A bare noun follows. `is logging events` is a verb, but
            // `are blocking operations` is an adjective, and the two have the
            // same shape. Compare `is string` or `are string and int`, which
            // are not reported at all.
            push_candidate(text, word.range(), Confidence::Low, &mut ranges);
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
            push_candidate(text, word.range(), Confidence::Medium, &mut ranges);
        }
    }

    for pattern in [&ING_SUBJECT, &ING_OPENER] {
        for captures in pattern.captures_iter(text) {
            if let Some(word) = captures.get(1) {
                push_candidate(text, word.range(), Confidence::Medium, &mut ranges);
            }
        }
    }

    // Keep the highest confidence when two frames report the same word.
    ranges.sort_unstable_by_key(|(range, confidence)| {
        (range.start, range.end, std::cmp::Reverse(*confidence))
    });
    ranges.dedup_by(|later, earlier| later.0 == earlier.0);
    ranges
}

fn push_candidate(
    text: &str,
    range: Range<usize>,
    confidence: Confidence,
    ranges: &mut Vec<(Range<usize>, Confidence)>,
) {
    let word = &text[range.clone()];
    if NOT_ING_FORMS
        .iter()
        .any(|skipped| word.eq_ignore_ascii_case(skipped))
    {
        return;
    }
    ranges.push((range, confidence));
}

fn takes_object(rest: &str) -> bool {
    next_word(rest).is_some_and(|word| {
        OBJECT_WORDS
            .iter()
            .any(|object| word.eq_ignore_ascii_case(object))
    })
}

fn ends_clause(rest: &str) -> bool {
    let trimmed = rest.trim_start_matches(char::is_whitespace);
    trimmed.is_empty()
        || trimmed.starts_with(['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\''])
}

fn starts_connective(rest: &str) -> bool {
    next_word(rest).is_some_and(|word| {
        CONNECTIVES
            .iter()
            .any(|connective| word.eq_ignore_ascii_case(connective))
    })
}

fn pronoun_subject(prefix: &str) -> bool {
    PRONOUN_SUBJECT.is_match(prefix)
}
