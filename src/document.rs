use std::{ops::Range, sync::LazyLock};

use regex::Regex;

static WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[[:alpha:]]+(?:['.-][[:alpha:]]+)*").expect("built-in word pattern is valid")
});

const ABBREVIATIONS: &[&str] = &[
    "approx.", "cf.", "dept.", "dr.", "e.g.", "etc.", "fig.", "figs.", "i.e.", "inc.", "jr.",
    "mr.", "mrs.", "ms.", "no.", "prof.", "sr.", "vs.",
];

#[derive(Debug)]
pub struct Document {
    source: String,
    visible: String,
    line_starts: Vec<usize>,
    sentences: Vec<Range<usize>>,
    paragraphs: Vec<Range<usize>>,
    technical: Vec<Range<usize>>,
}

impl Document {
    pub fn new(source: String, markdown: bool) -> Self {
        let visible = if markdown {
            mask_markdown(&source)
        } else {
            source.clone()
        };
        let line_starts = line_starts(&source);
        let paragraphs = paragraph_ranges(&visible, markdown);
        let sentences = sentence_ranges(&visible, &paragraphs);
        let technical = technical_spans(&visible);
        Self {
            source,
            visible,
            line_starts,
            sentences,
            paragraphs,
            technical,
        }
    }

    pub fn visible(&self) -> &str {
        &self.visible
    }

    pub fn sentences(&self) -> &[Range<usize>] {
        &self.sentences
    }

    pub fn paragraphs(&self) -> &[Range<usize>] {
        &self.paragraphs
    }

    pub fn location(&self, offset: usize) -> (usize, usize) {
        let line_index = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts[line_index];
        let column = self.source[line_start..offset.min(self.source.len())]
            .chars()
            .count()
            + 1;
        (line_index + 1, column)
    }

    pub fn source_excerpt(&self, range: &Range<usize>) -> &str {
        let start_line = self
            .line_starts
            .partition_point(|start| *start <= range.start)
            - 1;
        let end_offset = range.end.saturating_sub(1).min(self.source.len());
        let end_line = self
            .line_starts
            .partition_point(|start| *start <= end_offset)
            - 1;
        let start = self.line_starts[start_line];
        let end = self
            .line_starts
            .get(end_line + 1)
            .copied()
            .unwrap_or(self.source.len());
        self.source[start..end].trim()
    }

    pub fn word_count(&self, range: &Range<usize>) -> usize {
        words(&self.visible[range.clone()]).count()
    }

    pub fn sentence_count(&self, paragraph: &Range<usize>) -> usize {
        self.sentences
            .iter()
            .filter(|sentence| sentence.start >= paragraph.start && sentence.end <= paragraph.end)
            .count()
    }

    /// True when `range` belongs to a filename, path, URL, or identifier.
    ///
    /// Grammar checks skip these spans because the spelling inside them is
    /// decided by a tool or a convention, not by prose.
    pub fn in_technical_token(&self, range: &Range<usize>) -> bool {
        self.overlaps_technical(range)
    }

    /// True when any technical token overlaps `range`.
    ///
    /// Rules that match phrases rather than single words use this form, so a
    /// match such as `is cached.data` is skipped as a whole.
    pub fn overlaps_technical(&self, range: &Range<usize>) -> bool {
        if range.start >= range.end {
            return false;
        }
        let index = self
            .technical
            .partition_point(|span| span.end <= range.start);
        self.technical
            .get(index)
            .is_some_and(|span| span.start < range.end)
    }
}

pub fn words(text: &str) -> impl Iterator<Item = &str> {
    WORD.find_iter(text).map(|matched| matched.as_str())
}

fn is_token_char(character: char) -> bool {
    character.is_alphanumeric()
        || matches!(
            character,
            '_' | '-' | '.' | '/' | '\\' | ':' | '@' | '#' | '~'
        )
}

fn has_extension(token: &str) -> bool {
    token.rfind('.').is_some_and(|dot| {
        let extension = &token[dot + 1..];
        !token[..dot].is_empty()
            && (1..=5).contains(&extension.len())
            && extension
                .chars()
                .all(|character| character.is_ascii_alphabetic())
    })
}

fn is_technical_token(token: &str) -> bool {
    token.contains("://")
        || token.contains("::")
        || token.contains(['/', '\\', '@', '#', '~', '_'])
        || has_extension(token)
}

fn technical_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut start = None;
    for (offset, character) in text.char_indices() {
        if is_token_char(character) {
            start.get_or_insert(offset);
        } else if let Some(token_start) = start.take() {
            push_technical_span(text, token_start, offset, &mut spans);
        }
    }
    if let Some(token_start) = start {
        push_technical_span(text, token_start, text.len(), &mut spans);
    }
    spans
}

fn push_technical_span(text: &str, start: usize, end: usize, spans: &mut Vec<Range<usize>>) {
    let run = &text[start..end];
    let trimmed = run
        .trim_start_matches(['(', '[', '{', '<', '"', '\'', '*'])
        .trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '"', '\'', '*']);

    if trimmed.is_empty() || !is_technical_token(trimmed) {
        return;
    }

    let leading = run.len()
        - run
            .trim_start_matches(['(', '[', '{', '<', '"', '\'', '*'])
            .len();
    spans.push(start + leading..start + leading + trimmed.len());
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
    starts
}

fn paragraph_ranges(text: &str, markdown: bool) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(paragraph_start) = start.take() {
                ranges.push(paragraph_start..offset);
            }
        } else if markdown && is_markdown_block_start(trimmed) {
            if let Some(paragraph_start) = start.take() {
                ranges.push(paragraph_start..offset);
            }
            start = Some(offset);
            if is_markdown_heading(trimmed) {
                ranges.push(offset..offset + line.len());
                start = None;
            }
        } else if start.is_none() {
            start = Some(offset);
        }
        offset += line.len();
    }
    if let Some(paragraph_start) = start {
        ranges.push(paragraph_start..text.len());
    }
    ranges
}

fn sentence_ranges(text: &str, paragraphs: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for paragraph in paragraphs {
        let mut start = paragraph.start;
        for (relative, character) in text[paragraph.clone()].char_indices() {
            let end = paragraph.start + relative + character.len_utf8();
            if matches!(character, '.' | '!' | '?')
                && is_sentence_end(
                    text,
                    paragraph.start,
                    paragraph.end,
                    relative + paragraph.start,
                )
            {
                push_trimmed(text, start..end, &mut ranges);
                start = end;
            }
        }
        push_trimmed(text, start..paragraph.end, &mut ranges);
    }
    ranges
}

fn is_sentence_end(
    text: &str,
    paragraph_start: usize,
    paragraph_end: usize,
    offset: usize,
) -> bool {
    let character = text[offset..]
        .chars()
        .next()
        .expect("offset is on a character");
    if character == '.' && is_abbreviation(text, paragraph_start, offset) {
        return false;
    }

    let after = offset + character.len_utf8();
    text[after..paragraph_end]
        .chars()
        .find(|next| !is_closing_delimiter(*next))
        .is_none_or(char::is_whitespace)
}

fn is_closing_delimiter(character: char) -> bool {
    matches!(
        character,
        '\'' | '"' | ')' | ']' | '}' | '*' | '_' | '\u{2019}' | '\u{201d}'
    )
}

fn is_abbreviation(text: &str, paragraph_start: usize, period: usize) -> bool {
    let prefix = &text[paragraph_start..period + 1];
    let token_start = prefix
        .char_indices()
        .rev()
        .find(|(_, character)| !(character.is_alphabetic() || *character == '.'))
        .map_or(0, |(offset, character)| offset + character.len_utf8());
    let token = prefix[token_start..].to_lowercase();

    if ABBREVIATIONS.contains(&token.as_str()) {
        return true;
    }

    let without_period = token.trim_end_matches('.');
    let segments: Vec<_> = without_period.split('.').collect();
    (segments.len() >= 2 && segments.iter().all(|segment| segment.chars().count() == 1))
        || (segments.len() == 1 && segments[0].chars().count() == 1)
}

fn is_markdown_heading(line: &str) -> bool {
    let hashes = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    (1..=6).contains(&hashes) && line[hashes..].starts_with(char::is_whitespace)
}

fn is_markdown_block_start(line: &str) -> bool {
    if is_markdown_heading(line)
        || line.starts_with("> ")
        || line.starts_with("- ")
        || line.starts_with("+ ")
        || line.starts_with("* ")
        || line.starts_with('|')
    {
        return true;
    }

    let marker_end = line
        .char_indices()
        .take_while(|(_, character)| character.is_ascii_digit())
        .map(|(offset, character)| offset + character.len_utf8())
        .last()
        .unwrap_or(0);
    marker_end > 0
        && line[marker_end..]
            .strip_prefix(['.', ')'])
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn push_trimmed(text: &str, range: Range<usize>, ranges: &mut Vec<Range<usize>>) {
    let Some(content) = text.get(range.clone()) else {
        return;
    };
    let leading = content.len() - content.trim_start().len();
    let trailing = content.trim_end().len();
    if leading < trailing {
        ranges.push(range.start + leading..range.start + trailing);
    }
}

fn mask_markdown(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    let mut fenced = false;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            blank(&mut bytes, offset..offset + line.len());
            fenced = !fenced;
        } else if fenced {
            blank(&mut bytes, offset..offset + line.len());
        } else {
            mask_inline_code(&mut bytes, line, offset);
            mask_link_targets(&mut bytes, line, offset);
        }
        offset += line.len();
    }
    String::from_utf8(bytes).expect("masking preserves UTF-8")
}

fn mask_inline_code(bytes: &mut [u8], line: &str, base: usize) {
    let mut opening = None;
    for (offset, character) in line.char_indices() {
        if character == '`' {
            if let Some(start) = opening.take() {
                blank(bytes, base + start..base + offset + 1);
            } else {
                opening = Some(offset);
            }
        }
    }
}

fn mask_link_targets(bytes: &mut [u8], line: &str, base: usize) {
    let mut search_from = 0;
    while let Some(relative) = line[search_from..].find("](") {
        let start = search_from + relative + 1;
        let Some(close) = line[start..].find(')') else {
            break;
        };
        let end = start + close + 1;
        blank(bytes, base + start..base + end);
        search_from = end;
    }
}

fn blank(bytes: &mut [u8], range: Range<usize>) {
    for byte in &mut bytes[range] {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Document, words};

    #[test]
    fn masks_markdown_code_without_changing_offsets() {
        let source = "Use the tool.\n\n```rust\nlet ignored = true;\n```\n\nRead `some value`.";
        let document = Document::new(source.to_owned(), true);
        assert_eq!(document.source.len(), document.visible().len());
        assert_eq!(document.sentences().len(), 2);
        assert!(!document.visible().contains("ignored"));
        assert!(!document.visible().contains("some value"));
    }

    #[test]
    fn computes_unicode_columns() {
        let source = "Café is open.";
        let document = Document::new(source.to_owned(), false);
        assert_eq!(document.location(source.find("is").unwrap()), (1, 6));
    }

    #[test]
    fn returns_multiline_source_excerpts() {
        let document = Document::new("  First line.  \nSecond line.".to_owned(), false);
        assert_eq!(
            document.source_excerpt(&(2..27)),
            "First line.  \nSecond line."
        );
    }

    #[test]
    fn counts_dotted_technical_tokens_as_one_word() {
        let tokens: Vec<_> = words("Edit CONTRIBUTING.md with docs.rs guidance.").collect();
        assert_eq!(
            tokens,
            ["Edit", "CONTRIBUTING.md", "with", "docs.rs", "guidance"]
        );
    }

    #[test]
    fn ends_a_sentence_after_a_dotted_filename() {
        let document = Document::new("Read CONTRIBUTING.md. Then stop.".to_owned(), false);
        assert_eq!(document.sentences().len(), 2);
        assert_eq!(document.word_count(&document.sentences()[0]), 2);
    }

    #[test]
    fn keeps_common_abbreviations_inside_sentences() {
        let document = Document::new("Use e.g. this option.".to_owned(), false);
        assert_eq!(document.sentences().len(), 1);
        assert_eq!(document.word_count(&document.sentences()[0]), 4);
    }

    #[test]
    fn accepts_closing_delimiters_after_sentence_punctuation() {
        let document = Document::new("Use this option.) Then stop.".to_owned(), false);
        assert_eq!(document.sentences().len(), 2);
    }

    #[test]
    fn recognizes_technical_tokens() {
        for (source, token) in [
            ("Edit CONTRIBUTING.md.", "CONTRIBUTING.md"),
            ("Open src/logging/mod.rs.", "logging"),
            ("Run https://example.com/logging now.", "logging"),
            ("Set logging_enabled to true.", "logging"),
            ("Call logging::init now.", "logging"),
            ("Install @scope/logging now.", "logging"),
            ("Read logging.rs and stop.", "logging"),
            ("Run build-logging.sh now.", "logging"),
        ] {
            let document = Document::new(source.to_owned(), false);
            let start = source.find(token).expect("token is in the source");
            let range = start..start + token.len();
            assert!(
                document.in_technical_token(&range),
                "{token:?} in {source:?} should be technical"
            );
        }
    }

    #[test]
    fn does_not_treat_prose_words_as_technical_tokens() {
        for (source, token) in [
            ("We are testing.", "testing"),
            ("The value is a string.", "string"),
            ("Read the file (logging).", "logging"),
        ] {
            let document = Document::new(source.to_owned(), false);
            let start = source.find(token).expect("token is in the source");
            let range = start..start + token.len();
            assert!(
                !document.in_technical_token(&range),
                "{token:?} in {source:?} should be prose"
            );
        }
    }

    #[test]
    fn finds_technical_tokens_that_overlap_a_phrase() {
        let source = "The value is cached.data here.";
        let document = Document::new(source.to_owned(), false);
        let start = source.find("is cached").expect("phrase is in the source");
        let end = source
            .find(" here")
            .expect("trailing word is in the source");

        assert!(document.overlaps_technical(&(start..end)));
        assert!(!document.overlaps_technical(&(0..3)));
    }

    #[test]
    fn separates_markdown_headings_and_list_items() {
        let source = "# Setup\nUse this configuration now.\n\n- First item\n- Second item";
        let document = Document::new(source.to_owned(), true);
        let counts: Vec<_> = document
            .sentences()
            .iter()
            .map(|sentence| document.word_count(sentence))
            .collect();
        assert_eq!(counts, [1, 4, 2, 2]);
    }
}
