use std::sync::LazyLock;

use regex::Regex;

use crate::document::Document;

use super::{Finding, push_bounded};

struct Phrase {
    regex: Regex,
    replacement: &'static str,
}

static PHRASES: LazyLock<Vec<Phrase>> = LazyLock::new(|| {
    [
        (r"(?i)\bin order to\b", "to"),
        (r"(?i)\bdue to the fact that\b", "because"),
        (r"(?i)\ba number of\b", "several or an exact number"),
        (r"(?i)\butili[sz]e(?:d|s|ing)?\b", "use"),
        (r"(?i)\bprior to\b", "before"),
        (r"(?i)\bsubsequent to\b", "after"),
        (
            r"(?i)\b(?:obviously|clearly|simply|basically)\b",
            "a specific explanation",
        ),
        (
            r"(?i)\b(?:seamless|robust|crucial)\b",
            "a precise, measurable term",
        ),
        (r"(?i)\b(?:delve|delves|delved|delving)\b", "examine"),
        (
            r"(?i)\b(?:moreover|additionally)\b",
            "a direct transition or no transition",
        ),
    ]
    .into_iter()
    .map(|(pattern, replacement)| Phrase {
        regex: Regex::new(pattern).expect("built-in phrase pattern is valid"),
        replacement,
    })
    .collect()
});

pub fn check(document: &Document, findings: &mut Vec<Finding>, limit: usize) {
    for phrase in PHRASES.iter() {
        for matched in phrase.regex.find_iter(document.visible()) {
            if !push_bounded(
                findings,
                Finding {
                    range: matched.range(),
                    code: "STE006",
                    message: format!("imprecise phrase {:?}", matched.as_str()),
                    help: Some(format!("Try {:?} instead.", phrase.replacement)),
                },
                limit,
            ) {
                return;
            }
        }
    }

    for (offset, _) in document.visible().match_indices(';') {
        if !push_bounded(
            findings,
            Finding {
                range: offset..offset + 1,
                code: "STE007",
                message: "semicolon can join multiple instructions or topics".to_owned(),
                help: Some("Use separate sentences or a vertical list.".to_owned()),
            },
            limit,
        ) {
            return;
        }
    }
}
