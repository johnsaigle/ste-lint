use crate::{config::Config, document::Document};

use super::{Confidence, Finding, Sink};

pub fn check(document: &Document, config: &Config, sink: &mut Sink) {
    for sentence in document.sentences() {
        let count = document.word_count(sentence);
        if count > config.max_sentence_words
            && !sink.push(Finding {
                range: sentence.clone(),
                code: "STE001",
                confidence: Confidence::High,
                message: format!(
                    "sentence has {count} words; the configured maximum is {}",
                    config.max_sentence_words
                ),
                help: Some("Split the sentence into shorter statements.".to_owned()),
            })
        {
            return;
        }
    }

    for paragraph in document.paragraphs() {
        let count = document.sentence_count(paragraph);
        if count > config.max_paragraph_sentences
            && !sink.push(Finding {
                range: paragraph.start..paragraph.start.saturating_add(1),
                code: "STE002",
                confidence: Confidence::High,
                message: format!(
                    "paragraph has {count} sentences; the configured maximum is {}",
                    config.max_paragraph_sentences
                ),
                help: Some("Split the paragraph so that each paragraph has one topic.".to_owned()),
            })
        {
            return;
        }
    }
}
