use std::sync::LazyLock;

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
static ING_FORM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[[:alpha:]][[:alpha:]-]{2,}ing\b")
        .expect("built-in ing-form pattern is valid")
});

pub fn check(document: &Document, findings: &mut Vec<Finding>, limit: usize) {
    for matched in PASSIVE.find_iter(document.visible()) {
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

    for matched in ING_FORM.find_iter(document.visible()) {
        let range = matched.range();
        let touches_period = range
            .start
            .checked_sub(1)
            .is_some_and(|before| document.visible().as_bytes()[before] == b'.')
            || document
                .visible()
                .as_bytes()
                .get(range.end)
                .is_some_and(|after| *after == b'.');
        if touches_period {
            continue;
        }
        if !push_bounded(
            findings,
            Finding {
                range,
                code: "STE005",
                message: format!("review the -ing form {:?}", matched.as_str()),
                help: Some(
                    "Use an -ing form only as a technical noun or a modifier in a technical noun."
                        .to_owned(),
                ),
            },
            limit,
        ) {
            return;
        }
    }
}
