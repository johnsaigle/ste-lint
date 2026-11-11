use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use crate::rules::Confidence;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Procedure,
    Description,
}

#[derive(Debug, Parser)]
#[command(
    name = "ste-lint",
    version,
    about = "Lint prose with practical Simplified Technical English guidance",
    long_about = "Lint Markdown and plain-text documents with a practical subset of Simplified Technical English guidance. This tool is an authoring aid, not an ASD-STE100 compliance checker."
)]
pub struct Cli {
    /// Files or directories to lint. Use '-' to read standard input.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Treat text as procedural or descriptive writing [default: description].
    #[arg(long, value_enum)]
    pub mode: Option<Mode>,

    /// Override the mode-specific maximum sentence length.
    #[arg(long)]
    pub max_sentence_words: Option<usize>,

    /// Maximum sentences in one paragraph [default: 6].
    #[arg(long)]
    pub max_paragraph_sentences: Option<usize>,

    /// Comma-separated rule codes to run. Replaces `select` from the config file.
    #[arg(long, value_delimiter = ',')]
    pub select: Option<Vec<String>>,

    /// Comma-separated rule codes to skip. Adds to `ignore` from the config file.
    #[arg(long, value_delimiter = ',')]
    pub ignore: Vec<String>,

    /// Report only findings at or above this confidence [default: low].
    #[arg(long, value_enum)]
    pub min_confidence: Option<Confidence>,

    /// Read settings from this file instead of searching for `ste-lint.toml`.
    #[arg(long, conflicts_with = "no_config")]
    pub config: Option<PathBuf>,

    /// Do not read a config file.
    #[arg(long)]
    pub no_config: bool,

    /// Maximum number of files to process.
    #[arg(long, default_value_t = 10_000)]
    pub max_files: usize,

    /// Maximum bytes read from one file.
    #[arg(long, default_value_t = 2_097_152)]
    pub max_file_bytes: u64,

    /// Maximum directory traversal depth.
    #[arg(long, default_value_t = 32)]
    pub max_depth: usize,

    /// Maximum diagnostics emitted across all files.
    #[arg(long, default_value_t = 1_000)]
    pub max_diagnostics: usize,

    /// Worker threads. Zero selects an automatically bounded value.
    #[arg(short = 'j', long, default_value_t = 0)]
    pub jobs: usize,

    /// Comma-separated extensions to include.
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "md,markdown,mdown,mkdn,txt,rst,adoc,asciidoc"
    )]
    pub extensions: Vec<String>,

    /// Include hidden and ignored files.
    #[arg(long)]
    pub no_ignore: bool,

    /// Diagnostic output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,
}
