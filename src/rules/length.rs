use crate::{config::Config, document::Document};

use super::{Finding, push_bounded};

pub fn check(document: &Document, config: &Config, findings: &mut Vec<Finding>, limit: usize) {
    for sentence in document.sentences() {
        let count = document.word_count(sentence);
        if count > config.max_sentence_words
            && !push_bounded(
                findings,
                Finding {
                    range: sentence.clone(),
                    code: "STE001",
                    message: format!(
                        "sentence has {count} words; the configured maximum is {}",
                        config.max_sentence_words
                    ),
                    help: Some("Split the sentence into shorter statements.".to_owned()),
                },
                limit,
            )
        {
            return;
        }
    }

    for paragraph in document.paragraphs() {
        let count = document.sentence_count(paragraph);
        if count > config.max_paragraph_sentences
            && !push_bounded(
                findings,
                Finding {
                    range: paragraph.start..paragraph.start.saturating_add(1),
                    code: "STE002",
                    message: format!(
                        "paragraph has {count} sentences; the configured maximum is {}",
                        config.max_paragraph_sentences
                    ),
                    help: Some(
                        "Split the paragraph so that each paragraph has one topic.".to_owned(),
                    ),
                },
                limit,
            )
        {
            return;
        }
    }
}
