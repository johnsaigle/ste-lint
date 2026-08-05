use std::{collections::HashSet, path::PathBuf, thread};

use anyhow::{bail, ensure};

use crate::cli::{Cli, Format, Mode};

const HARD_MAX_INPUTS: usize = 256;
const HARD_MAX_FILES: usize = 100_000;
const HARD_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const HARD_MAX_DEPTH: usize = 128;
const HARD_MAX_DIAGNOSTICS: usize = 100_000;
const HARD_MAX_JOBS: usize = 64;
const HARD_MAX_IN_FLIGHT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug)]
pub struct Config {
    pub paths: Vec<PathBuf>,
    pub max_sentence_words: usize,
    pub max_paragraph_sentences: usize,
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_depth: usize,
    pub max_diagnostics: usize,
    pub jobs: usize,
    pub extensions: HashSet<String>,
    pub no_ignore: bool,
    pub format: Format,
}

impl TryFrom<Cli> for Config {
    type Error = anyhow::Error;

    fn try_from(cli: Cli) -> Result<Self, Self::Error> {
        ensure!(!cli.paths.is_empty(), "at least one input is required");
        ensure!(
            cli.paths.len() <= HARD_MAX_INPUTS,
            "too many inputs (maximum {HARD_MAX_INPUTS})"
        );
        ensure_range("max-files", cli.max_files, HARD_MAX_FILES)?;
        ensure_range(
            "max-paragraph-sentences",
            cli.max_paragraph_sentences,
            1_000,
        )?;
        ensure_range("max-diagnostics", cli.max_diagnostics, HARD_MAX_DIAGNOSTICS)?;
        ensure_range("max-depth", cli.max_depth, HARD_MAX_DEPTH)?;
        ensure!(
            (1..=HARD_MAX_FILE_BYTES).contains(&cli.max_file_bytes),
            "--max-file-bytes must be between 1 and {HARD_MAX_FILE_BYTES}"
        );

        let max_sentence_words = cli.max_sentence_words.unwrap_or(match cli.mode {
            Mode::Procedure => 20,
            Mode::Description => 25,
        });
        ensure_range("max-sentence-words", max_sentence_words, 1_000)?;

        let auto_jobs = thread::available_parallelism()
            .map_or(1, usize::from)
            .min(16);
        let jobs = if cli.jobs == 0 { auto_jobs } else { cli.jobs };
        ensure_range("jobs", jobs, HARD_MAX_JOBS)?;
        ensure!(
            cli.max_file_bytes.saturating_mul(jobs as u64) <= HARD_MAX_IN_FLIGHT_BYTES,
            "--jobs multiplied by --max-file-bytes must not exceed {HARD_MAX_IN_FLIGHT_BYTES} bytes"
        );

        let mut extensions = HashSet::with_capacity(cli.extensions.len());
        for extension in cli.extensions {
            let extension = extension
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if extension.is_empty()
                || extension.len() > 16
                || !extension.chars().all(|c| c.is_ascii_alphanumeric())
            {
                bail!("invalid extension: {extension:?}");
            }
            extensions.insert(extension);
        }
        ensure!(!extensions.is_empty(), "at least one extension is required");

        Ok(Self {
            paths: cli.paths,
            max_sentence_words,
            max_paragraph_sentences: cli.max_paragraph_sentences,
            max_files: cli.max_files,
            max_file_bytes: cli.max_file_bytes,
            max_depth: cli.max_depth,
            max_diagnostics: cli.max_diagnostics,
            jobs,
            extensions,
            no_ignore: cli.no_ignore,
            format: cli.format,
        })
    }
}

fn ensure_range(name: &str, value: usize, maximum: usize) -> anyhow::Result<()> {
    ensure!(
        (1..=maximum).contains(&value),
        "--{name} must be between 1 and {maximum}"
    );
    Ok(())
}
