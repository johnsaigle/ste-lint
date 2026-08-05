mod length;
mod style;
mod verbs;

use std::ops::Range;

use crate::{config::Config, diagnostic::Diagnostic, document::Document};

const MAX_SOURCE_LINE_CHARS: usize = 240;

#[derive(Debug)]
pub struct Finding {
    pub range: Range<usize>,
    pub code: &'static str,
    pub message: String,
    pub help: Option<String>,
}

pub fn check(document: &Document, config: &Config, remaining: usize) -> Vec<Finding> {
    let mut findings = Vec::new();
    length::check(document, config, &mut findings, remaining);
    style::check(document, &mut findings, remaining);
    verbs::check(document, &mut findings, remaining);
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
        code: finding.code,
        message: finding.message,
        help: finding.help,
    }
}

fn truncate_source_line(line: &str) -> String {
    let mut characters = line.chars();
    let mut rendered: String = characters.by_ref().take(MAX_SOURCE_LINE_CHARS).collect();
    if characters.next().is_some() {
        rendered.push_str("...");
    }
    rendered
}

fn push_bounded(findings: &mut Vec<Finding>, finding: Finding, limit: usize) -> bool {
    if findings.len() >= limit {
        return false;
    }
    findings.push(finding);
    true
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, path::PathBuf};

    use crate::{cli::Format, config::Config, document::Document};

    use super::check;

    fn config() -> Config {
        Config {
            paths: vec![PathBuf::from("test.txt")],
            max_sentence_words: 5,
            max_paragraph_sentences: 2,
            max_files: 10,
            max_file_bytes: 1_024,
            max_depth: 4,
            max_diagnostics: 100,
            jobs: 1,
            extensions: HashSet::from(["txt".to_owned()]),
            no_ignore: false,
            format: Format::Text,
        }
    }

    #[test]
    fn reports_representative_ste_guidance() {
        let text = "The robust valve has been opened in order to start heating; clearly, it works. One. Two. Three.";
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
}
