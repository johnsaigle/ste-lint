use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub end_column: usize,
    pub source_line: String,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub help: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
    pub files_checked: usize,
    pub truncated: bool,
}
