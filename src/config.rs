use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    thread,
};

use anyhow::{Context, bail, ensure};
use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;

use crate::{
    cli::{Cli, Format, Mode},
    rules::{CODES, Confidence, Filter},
};

/// The file name searched for in the current directory and its ancestors.
pub const CONFIG_FILE_NAME: &str = "ste-lint.toml";
const HARD_MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const HARD_MAX_PATH_RULES: usize = 1_000;

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
    pub mode: Mode,
    /// Rules and confidence for files without a per-path rule.
    pub filter: Filter,
    pub config_file: Option<ConfigFile>,
}

/// Settings read from `ste-lint.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct FileSettings {
    mode: Option<Mode>,
    max_sentence_words: Option<usize>,
    max_paragraph_sentences: Option<usize>,
    select: Option<Vec<String>>,
    #[serde(default)]
    ignore: Vec<String>,
    min_confidence: Option<Confidence>,
    #[serde(default)]
    per_path_ignores: BTreeMap<String, Vec<String>>,
}

/// Per-path rules from a config file.
#[derive(Debug)]
pub struct ConfigFile {
    /// The directory that holds the config file. Globs are relative to it.
    root: PathBuf,
    per_path_ignores: Vec<(GlobMatcher, HashSet<&'static str>)>,
}

impl Config {
    /// Return the filter for one input. `None` is standard input.
    pub fn filter_for(&self, path: Option<&Path>) -> Cow<'_, Filter> {
        let (Some(path), Some(file)) = (path, &self.config_file) else {
            return Cow::Borrowed(&self.filter);
        };
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        let relative = absolute.strip_prefix(&file.root).unwrap_or(path);
        let mut filter = Cow::Borrowed(&self.filter);
        for (glob, codes) in &file.per_path_ignores {
            if glob.is_match(relative) {
                filter.to_mut().enabled.retain(|code| !codes.contains(code));
            }
        }
        filter
    }
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

        let (settings, config_path) = load_settings(&cli)?;
        let context = |name: &str| match &config_path {
            Some(path) => format!("{name} in {}", path.display()),
            None => name.to_owned(),
        };

        let max_paragraph_sentences = cli
            .max_paragraph_sentences
            .or(settings.max_paragraph_sentences)
            .unwrap_or(6);
        ensure_range("max-paragraph-sentences", max_paragraph_sentences, 1_000)?;
        ensure_range("max-diagnostics", cli.max_diagnostics, HARD_MAX_DIAGNOSTICS)?;
        ensure_range("max-depth", cli.max_depth, HARD_MAX_DEPTH)?;
        ensure!(
            (1..=HARD_MAX_FILE_BYTES).contains(&cli.max_file_bytes),
            "--max-file-bytes must be between 1 and {HARD_MAX_FILE_BYTES}"
        );

        let mode = cli.mode.or(settings.mode).unwrap_or(Mode::Description);
        let max_sentence_words = cli
            .max_sentence_words
            .or(settings.max_sentence_words)
            .unwrap_or(match mode {
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

        let select = match cli.select.as_ref() {
            Some(codes) => parse_codes(codes, "--select")?,
            None => match &settings.select {
                Some(codes) => parse_codes(codes, &context("select"))?,
                None => CODES.into_iter().collect(),
            },
        };
        let mut ignore = parse_codes(&cli.ignore, "--ignore")?;
        ignore.extend(parse_codes(&settings.ignore, &context("ignore"))?);
        let filter = Filter {
            enabled: select.difference(&ignore).copied().collect(),
            min_confidence: cli
                .min_confidence
                .or(settings.min_confidence)
                .unwrap_or(Confidence::Low),
        };

        let config_file = match config_path {
            Some(path) => Some(compile_config_file(&path, settings.per_path_ignores)?),
            None => None,
        };

        Ok(Self {
            paths: cli.paths,
            max_sentence_words,
            max_paragraph_sentences,
            max_files: cli.max_files,
            max_file_bytes: cli.max_file_bytes,
            max_depth: cli.max_depth,
            max_diagnostics: cli.max_diagnostics,
            jobs,
            extensions,
            no_ignore: cli.no_ignore,
            format: cli.format,
            mode,
            filter,
            config_file,
        })
    }
}

/// Read the config file named on the command line, or the nearest
/// `ste-lint.toml` in the current directory or one of its ancestors.
fn load_settings(cli: &Cli) -> anyhow::Result<(FileSettings, Option<PathBuf>)> {
    if cli.no_config {
        return Ok((FileSettings::default(), None));
    }
    let path = match &cli.config {
        Some(path) => {
            ensure!(
                path.is_file(),
                "config file does not exist: {}",
                path.display()
            );
            path.clone()
        }
        None => {
            let current =
                std::env::current_dir().context("failed to read the current directory")?;
            let Some(path) = current
                .ancestors()
                .map(|directory| directory.join(CONFIG_FILE_NAME))
                .find(|candidate| candidate.is_file())
            else {
                return Ok((FileSettings::default(), None));
            };
            path
        }
    };
    let path = std::path::absolute(&path).unwrap_or(path);
    let file =
        fs::File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut text = String::new();
    file.take(HARD_MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut text)
        .with_context(|| format!("failed to read {}", path.display()))?;
    ensure!(
        text.len() as u64 <= HARD_MAX_CONFIG_BYTES,
        "{} exceeds the {HARD_MAX_CONFIG_BYTES}-byte limit",
        path.display()
    );
    let settings =
        toml::from_str(&text).with_context(|| format!("invalid config file {}", path.display()))?;
    Ok((settings, Some(path)))
}

fn compile_config_file(
    path: &Path,
    per_path_ignores: BTreeMap<String, Vec<String>>,
) -> anyhow::Result<ConfigFile> {
    ensure!(
        per_path_ignores.len() <= HARD_MAX_PATH_RULES,
        "too many per-path-ignores entries in {} (maximum {HARD_MAX_PATH_RULES})",
        path.display()
    );
    let root = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let mut compiled = Vec::with_capacity(per_path_ignores.len());
    for (pattern, codes) in per_path_ignores {
        // A pattern without a separator matches at any depth, as in
        // `.gitignore`.
        let anchored = if pattern.contains('/') {
            pattern.trim_start_matches('/').to_owned()
        } else {
            format!("**/{pattern}")
        };
        let glob = GlobBuilder::new(&anchored)
            .literal_separator(true)
            .build()
            .with_context(|| format!("invalid glob {pattern:?} in {}", path.display()))?
            .compile_matcher();
        let codes = parse_codes(&codes, &format!("per-path-ignores in {}", path.display()))?;
        compiled.push((glob, codes));
    }
    Ok(ConfigFile {
        root,
        per_path_ignores: compiled,
    })
}

/// Map user-supplied rule codes to the built-in codes. `ALL` and `*` mean
/// every rule.
fn parse_codes(codes: &[String], source: &str) -> anyhow::Result<HashSet<&'static str>> {
    let mut parsed = HashSet::new();
    for code in codes {
        let code = code.trim().to_ascii_uppercase();
        if code.is_empty() {
            continue;
        }
        if code == "ALL" || code == "*" {
            parsed.extend(CODES);
            continue;
        }
        let Some(known) = CODES.iter().find(|known| **known == code) else {
            bail!(
                "unknown rule code {code:?} in {source}; expected one of {}",
                CODES.join(", ")
            );
        };
        parsed.insert(*known);
    }
    Ok(parsed)
}

fn ensure_range(name: &str, value: usize, maximum: usize) -> anyhow::Result<()> {
    ensure!(
        (1..=maximum).contains(&value),
        "--{name} must be between 1 and {maximum}"
    );
    Ok(())
}

/// Defaults for tests: description mode, all rules, and small limits.
#[cfg(test)]
impl Default for Config {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            max_sentence_words: 25,
            max_paragraph_sentences: 6,
            max_files: 10,
            max_file_bytes: 1_048_576,
            max_depth: 4,
            max_diagnostics: 100,
            jobs: 1,
            extensions: HashSet::from(["md".to_owned(), "txt".to_owned()]),
            no_ignore: false,
            format: Format::Text,
            mode: Mode::Description,
            filter: Filter::all(),
            config_file: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use clap::Parser;
    use tempfile::tempdir;

    use crate::{
        cli::{Cli, Mode},
        rules::Confidence,
    };

    use super::Config;

    fn load(arguments: &[&str]) -> anyhow::Result<Config> {
        let mut all = vec!["ste-lint"];
        all.extend_from_slice(arguments);
        Config::try_from(Cli::parse_from(all))
    }

    #[test]
    fn reads_settings_and_per_path_ignores() {
        let directory = tempdir().unwrap();
        let config_path = directory.path().join("ste-lint.toml");
        fs::write(
            &config_path,
            r#"
mode = "procedure"
ignore = ["ste007"]
min-confidence = "medium"

[per-path-ignores]
"CHANGELOG.md" = ["STE001"]
"legal/**" = ["*"]
"#,
        )
        .unwrap();
        let config = load(&["--config", config_path.to_str().unwrap(), "-"]).unwrap();

        assert_eq!(config.mode, Mode::Procedure);
        assert_eq!(config.max_sentence_words, 20);
        assert_eq!(config.filter.min_confidence, Confidence::Medium);
        assert!(!config.filter.is_enabled("STE007"));
        assert!(config.filter.is_enabled("STE001"));

        let changelog = directory.path().join("docs/CHANGELOG.md");
        assert!(!config.filter_for(Some(&changelog)).is_enabled("STE001"));
        assert!(config.filter_for(Some(&changelog)).is_enabled("STE003"));
        let legal = directory.path().join("legal/notice.md");
        assert!(config.filter_for(Some(&legal)).enabled.is_empty());
        assert!(config.filter_for(None).is_enabled("STE001"));
        assert!(
            config
                .filter_for(Some(Path::new("elsewhere/guide.md")))
                .is_enabled("STE001")
        );
    }

    #[test]
    fn command_line_overrides_the_config_file() {
        let directory = tempdir().unwrap();
        let config_path = directory.path().join("ste-lint.toml");
        fs::write(
            &config_path,
            "mode = \"procedure\"\nselect = [\"STE001\"]\n",
        )
        .unwrap();
        let path = config_path.to_str().unwrap();
        let config = load(&[
            "--config",
            path,
            "--mode",
            "description",
            "--select",
            "STE003,STE004",
            "--ignore",
            "STE004",
            "-",
        ])
        .unwrap();

        assert_eq!(config.mode, Mode::Description);
        assert_eq!(config.max_sentence_words, 25);
        let mut enabled: Vec<_> = config.filter.enabled.iter().copied().collect();
        enabled.sort_unstable();
        assert_eq!(enabled, ["STE003"]);
    }

    #[test]
    fn rejects_unknown_codes_and_keys() {
        let error = load(&["--no-config", "--ignore", "STE999", "-"]).unwrap_err();
        assert!(error.to_string().contains("unknown rule code"));

        let directory = tempdir().unwrap();
        let config_path = directory.path().join("ste-lint.toml");
        fs::write(&config_path, "colour = true\n").unwrap();
        let error = load(&["--config", config_path.to_str().unwrap(), "-"]).unwrap_err();
        assert!(format!("{error:#}").contains("unknown field"));
    }
}
