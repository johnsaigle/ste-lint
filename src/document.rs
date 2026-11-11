use std::{ops::Range, sync::LazyLock};

use regex::Regex;

static WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[[:alpha:]]+(?:['.-][[:alpha:]]+)*").expect("built-in word pattern is valid")
});
static HTML_COMMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<!--.*?(?:-->|\z)").expect("built-in HTML comment pattern is valid")
});
static DIRECTIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<!--\s*ste-lint-(disable-next-line|disable|enable)\b([^>]*?)-->")
        .expect("built-in directive pattern is valid")
});
static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"</?[A-Za-z][A-Za-z0-9-]*(?:\s[^<>]*)?/?>")
        .expect("built-in HTML tag pattern is valid")
});
static AUTOLINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(?:[A-Za-z][A-Za-z0-9+.-]{1,31}:[^<>\s]+|[^<>\s@]+@[^<>\s]+)>")
        .expect("built-in autolink pattern is valid")
});
static ENTITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(?:[A-Za-z][A-Za-z0-9]{1,31}|#[0-9]{1,7}|#[xX][0-9A-Fa-f]{1,6});")
        .expect("built-in entity pattern is valid")
});
static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[[^\]\n]*\](?:\[[^\]\n]*\])?").expect("built-in image pattern is valid")
});
static REFERENCE_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\]\[[^\]\n]*\]").expect("built-in reference label pattern is valid")
});
static LINK_DEFINITION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ {0,3}\[([^\]\n]+)\]:").expect("built-in link definition pattern is valid")
});
static TABLE_DELIMITER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\|?\s*:?-+:?\s*(?:\|\s*:?-+:?\s*)+\|?\s*$")
        .expect("built-in table delimiter pattern is valid")
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
    suppressions: Vec<Suppression>,
}

/// Lines where inline directives turn off some or all rules.
#[derive(Debug)]
struct Suppression {
    /// One-based line numbers, end exclusive.
    lines: Range<usize>,
    /// `None` means every rule.
    codes: Option<Vec<String>>,
}

impl Document {
    pub fn new(source: String, markdown: bool) -> Self {
        let line_starts = line_starts(&source);
        // Read directives after code is masked, so an example directive in a
        // code fence or a code span has no effect.
        let (visible, suppressions) = if markdown {
            let code_masked = mask_markdown_code(&source);
            let suppressions = suppressions(&code_masked, &line_starts);
            (mask_markdown_markup(code_masked), suppressions)
        } else {
            let suppressions = suppressions(&source, &line_starts);
            (mask_directives(&source), suppressions)
        };
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
            suppressions,
        }
    }

    /// True when an inline directive turns off `code` at `offset`.
    pub fn is_suppressed(&self, offset: usize, code: &str) -> bool {
        if self.suppressions.is_empty() {
            return false;
        }
        let (line, _) = self.location(offset);
        self.suppressions.iter().any(|suppression| {
            suppression.lines.contains(&line)
                && suppression.codes.as_ref().is_none_or(|codes| {
                    codes
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(code))
                })
        })
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
        } else if markdown && trimmed.starts_with('|') {
            if let Some(paragraph_start) = start.take() {
                ranges.push(paragraph_start..offset);
            }
            push_table_cells(line, offset, &mut ranges);
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

/// Push each cell of a Markdown table row as its own paragraph.
///
/// A row is a set of separate values, not one sentence, so the cells must not
/// be joined for the length checks.
fn push_table_cells(line: &str, base: usize, ranges: &mut Vec<Range<usize>>) {
    let mut cell_start = 0;
    let mut previous = '\0';
    for (offset, character) in line.char_indices() {
        if character == '|' && previous != '\\' {
            push_cell(line, base, cell_start..offset, ranges);
            cell_start = offset + 1;
        }
        previous = character;
    }
    push_cell(line, base, cell_start..line.len(), ranges);
}

fn push_cell(line: &str, base: usize, cell: Range<usize>, ranges: &mut Vec<Range<usize>>) {
    if !line[cell.clone()].trim().is_empty() {
        ranges.push(base + cell.start..base + cell.end);
    }
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

/// Mask front matter, code fences, and code spans.
fn mask_markdown_code(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    let front_matter = front_matter_end(source);
    blank(&mut bytes, 0..front_matter);

    let mut fenced = false;
    let mut offset = front_matter;
    for line in source[front_matter..].split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            blank(&mut bytes, offset..offset + line.len());
            fenced = !fenced;
        } else if fenced {
            blank(&mut bytes, offset..offset + line.len());
        }
        offset += line.len();
    }
    mask_inline_code(&mut bytes);
    String::from_utf8(bytes).expect("masking preserves UTF-8")
}

/// Mask Markdown and HTML markup in text that has no code left in it.
fn mask_markdown_markup(text: String) -> String {
    let mut bytes = text.into_bytes();
    // Comments can span lines, so mask them before the line-based passes.
    // Code is blank at this point, so a `<!--` inside code does not start a
    // comment.
    mask_pattern(&mut bytes, &HTML_COMMENT);

    let text = String::from_utf8(bytes.clone()).expect("masking preserves UTF-8");
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if TABLE_DELIMITER.is_match(line) {
            blank(&mut bytes, offset..offset + line.len());
        } else if let Some(definition) = LINK_DEFINITION.captures(line) {
            // A footnote definition keeps its prose. A link reference
            // definition holds only a label, a destination, and a title.
            let label = definition.get(1).expect("label group exists");
            let end = if label.as_str().starts_with('^') {
                definition.get(0).expect("match exists").end()
            } else {
                line.len()
            };
            blank(&mut bytes, offset..offset + end);
        } else {
            mask_link_targets(&mut bytes, line, offset);
        }
        offset += line.len();
    }
    for pattern in [&IMAGE, &REFERENCE_LABEL, &AUTOLINK, &HTML_TAG, &ENTITY] {
        mask_pattern(&mut bytes, pattern);
    }
    String::from_utf8(bytes).expect("masking preserves UTF-8")
}

/// Mask inline directive comments in formats other than Markdown.
fn mask_directives(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    mask_pattern(&mut bytes, &DIRECTIVE);
    String::from_utf8(bytes).expect("masking preserves UTF-8")
}

fn mask_pattern(bytes: &mut [u8], pattern: &Regex) {
    let text = std::str::from_utf8(bytes).expect("masking preserves UTF-8");
    let ranges: Vec<_> = pattern.find_iter(text).map(|found| found.range()).collect();
    for range in ranges {
        blank(bytes, range);
    }
}

/// Return the end offset of YAML (`---`) or TOML (`+++`) front matter.
fn front_matter_end(source: &str) -> usize {
    let mut lines = source.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return 0;
    };
    let fence = first.trim_end();
    if fence != "---" && fence != "+++" {
        return 0;
    }
    let mut offset = first.len();
    for line in lines {
        offset += line.len();
        let trimmed = line.trim_end();
        if trimmed == fence || (fence == "---" && trimmed == "...") {
            return offset;
        }
    }
    0
}

/// Parse `ste-lint-disable`, `ste-lint-enable`, and
/// `ste-lint-disable-next-line` comments.
///
/// A directive without codes applies to every rule. `ste-lint-enable` without
/// codes ends every open region. With codes, it ends only the regions that
/// were opened with the same codes.
fn suppressions(source: &str, line_starts: &[usize]) -> Vec<Suppression> {
    let mut suppressions = Vec::new();
    let mut open: Vec<(usize, Option<Vec<String>>)> = Vec::new();
    for directive in DIRECTIVE.captures_iter(source) {
        let whole = directive.get(0).expect("match exists");
        let line = line_starts.partition_point(|start| *start <= whole.start());
        let end_line = line_starts.partition_point(|start| *start < whole.end());
        let codes = parse_codes(&directive[2]);
        match &directive[1] {
            "disable-next-line" => suppressions.push(Suppression {
                lines: end_line + 1..end_line + 2,
                codes,
            }),
            "disable" => open.push((line, codes)),
            _ => {
                let (closed, still_open): (Vec<_>, Vec<_>) =
                    open.into_iter().partition(|(_, open_codes)| {
                        codes.is_none() || open_codes.as_ref() == codes.as_ref()
                    });
                open = still_open;
                suppressions.extend(closed.into_iter().map(|(start, codes)| Suppression {
                    lines: start..line + 1,
                    codes,
                }));
            }
        }
    }
    suppressions.extend(open.into_iter().map(|(start, codes)| Suppression {
        lines: start..usize::MAX,
        codes,
    }));
    suppressions
}

fn parse_codes(text: &str) -> Option<Vec<String>> {
    let codes: Vec<String> = text
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|code| !code.is_empty())
        .map(str::to_ascii_uppercase)
        .collect();
    (!codes.is_empty() && !codes.iter().any(|code| code == "*" || code == "ALL")).then_some(codes)
}

/// Mask code spans. A span can wrap onto the next line, as in CommonMark,
/// but it cannot cross a blank line.
fn mask_inline_code(bytes: &mut [u8]) {
    let text = std::str::from_utf8(bytes).expect("masking preserves UTF-8");
    let mut spans = Vec::new();
    let mut opening: Option<usize> = None;
    for (offset, character) in text.char_indices() {
        if character != '`' {
            continue;
        }
        match opening.take() {
            Some(start) if !crosses_blank_line(&text[start..offset]) => {
                spans.push(start..offset + 1);
            }
            _ => opening = Some(offset),
        }
    }
    for span in spans {
        blank(bytes, span);
    }
}

fn crosses_blank_line(text: &str) -> bool {
    text.split('\n').skip(1).any(|line| line.trim().is_empty())
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

    #[test]
    fn masks_markdown_structures_that_are_not_prose() {
        let source = "---\ntitle: The value is ignored by tools.\n---\n\
            <!-- The comment is ignored by tools. -->\n\
            [ci]: https://example.com/ci\n\
            [![Build][badge]][ci] <img src=\"x.png\"> <https://example.com>\n\
            Use it &mdash; now.\n";
        let document = Document::new(source.to_owned(), true);
        let visible = document.visible();
        assert_eq!(visible.len(), source.len());
        for hidden in [
            "title", "comment", "example", "Build", "badge", "img", "mdash",
        ] {
            assert!(
                !visible.contains(hidden),
                "{hidden:?} is visible: {visible:?}"
            );
        }
        assert!(visible.contains("Use it"));
    }

    #[test]
    fn masks_code_spans_that_wrap_lines() {
        let document = Document::new("Use `is then\nreturned` now.".to_owned(), true);
        assert!(!document.visible().contains("returned"));

        let document = Document::new("A ` tick.\n\nThe value is `x`.".to_owned(), true);
        assert!(document.visible().contains("tick"));
        assert!(!document.visible().contains('x'));
    }

    #[test]
    fn keeps_footnote_definition_prose() {
        let document = Document::new("[^1]: The note text.\n".to_owned(), true);
        assert!(!document.visible().contains("[^1]"));
        assert!(document.visible().contains("The note text."));
    }

    #[test]
    fn treats_table_cells_as_separate_paragraphs() {
        let source = "| Name | Description |\n| --- | --- |\n| `a` | One two three |\n";
        let document = Document::new(source.to_owned(), true);
        let cells: Vec<_> = document
            .paragraphs()
            .iter()
            .map(|paragraph| document.visible()[paragraph.clone()].trim())
            .collect();
        assert_eq!(cells, ["Name", "Description", "One two three"]);
    }

    #[test]
    fn applies_inline_directives() {
        let source = "<!-- ste-lint-disable-next-line STE003 -->\nOne.\nTwo.\n\
            <!-- ste-lint-disable -->\nThree.\n<!-- ste-lint-enable -->\nFour.\n\
            <!-- ste-lint-disable STE006, STE007 -->\nFive.\n";
        let document = Document::new(source.to_owned(), false);
        let offset = |text: &str| source.find(text).unwrap();

        assert!(document.is_suppressed(offset("One"), "STE003"));
        assert!(!document.is_suppressed(offset("One"), "STE001"));
        assert!(!document.is_suppressed(offset("Two"), "STE003"));
        assert!(document.is_suppressed(offset("Three"), "STE001"));
        assert!(!document.is_suppressed(offset("Four"), "STE001"));
        assert!(document.is_suppressed(offset("Five"), "STE007"));
        assert!(!document.is_suppressed(offset("Five"), "STE003"));
        assert!(!document.visible().contains("ste-lint"));
    }

    #[test]
    fn ignores_directives_inside_code() {
        let source = "```markdown\n<!-- ste-lint-disable -->\n```\n\
            `<!-- ste-lint-disable-next-line -->`\nOne.\n";
        let document = Document::new(source.to_owned(), true);
        assert!(!document.is_suppressed(source.find("One").unwrap(), "STE001"));
    }

    #[test]
    fn ends_only_matching_regions() {
        let source = "<!-- ste-lint-disable STE003 -->\n<!-- ste-lint-enable STE006 -->\nOne.\n";
        let document = Document::new(source.to_owned(), false);
        assert!(document.is_suppressed(source.find("One").unwrap(), "STE003"));
    }
}
