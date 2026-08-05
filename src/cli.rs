use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
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

    /// Treat text as procedural or descriptive writing.
    #[arg(long, value_enum, default_value_t = Mode::Description)]
    pub mode: Mode,

    /// Override the mode-specific maximum sentence length.
    #[arg(long)]
    pub max_sentence_words: Option<usize>,

    /// Maximum sentences in one paragraph.
    #[arg(long, default_value_t = 6)]
    pub max_paragraph_sentences: usize,

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
