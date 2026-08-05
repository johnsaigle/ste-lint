use std::{ffi::OsStr, path::PathBuf};

use anyhow::{Context, bail, ensure};
use ignore::WalkBuilder;

use crate::config::Config;

#[derive(Debug, Clone)]
pub enum Input {
    File(PathBuf),
    Stdin,
}

pub fn discover(config: &Config) -> anyhow::Result<Vec<Input>> {
    let has_stdin = config.paths.iter().any(|path| path == OsStr::new("-"));
    if has_stdin {
        ensure!(
            config.paths.len() == 1,
            "standard input '-' cannot be combined with other inputs"
        );
        return Ok(vec![Input::Stdin]);
    }

    for path in &config.paths {
        ensure!(path.exists(), "input does not exist: {}", path.display());
    }

    let mut builder = WalkBuilder::new(&config.paths[0]);
    for path in &config.paths[1..] {
        builder.add(path);
    }
    builder
        .max_depth(Some(config.max_depth))
        .follow_links(false)
        .hidden(!config.no_ignore)
        .ignore(!config.no_ignore)
        .git_ignore(!config.no_ignore)
        .git_global(!config.no_ignore)
        .git_exclude(!config.no_ignore);

    let mut inputs = Vec::new();
    for entry in builder.build() {
        let entry = entry.context("failed while traversing inputs")?;
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() || !supported(entry.path(), config) {
            continue;
        }
        if inputs.len() == config.max_files {
            bail!(
                "file limit exceeded (maximum {}); narrow the inputs or increase --max-files",
                config.max_files
            );
        }
        inputs.push(Input::File(entry.into_path()));
    }

    inputs.sort_by(|left, right| match (left, right) {
        (Input::File(left), Input::File(right)) => left.cmp(right),
        _ => std::cmp::Ordering::Equal,
    });
    Ok(inputs)
}

fn supported(path: &std::path::Path, config: &Config) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| config.extensions.contains(&extension))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs, path::PathBuf};

    use tempfile::tempdir;

    use crate::{cli::Format, config::Config};

    use super::{Input, discover};

    fn config(path: PathBuf) -> Config {
        Config {
            paths: vec![path],
            max_sentence_words: 25,
            max_paragraph_sentences: 6,
            max_files: 10,
            max_file_bytes: 1_024,
            max_depth: 4,
            max_diagnostics: 100,
            jobs: 1,
            extensions: HashSet::from(["md".to_owned(), "txt".to_owned()]),
            no_ignore: false,
            format: Format::Text,
        }
    }

    #[test]
    fn includes_only_supported_files() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("guide.md"), "Text.").unwrap();
        fs::write(directory.path().join("data.json"), "{}").unwrap();

        let inputs = discover(&config(directory.path().to_owned())).unwrap();
        assert_eq!(inputs.len(), 1);
        assert!(matches!(&inputs[0], Input::File(path) if path.ends_with("guide.md")));
    }

    #[test]
    fn fails_instead_of_silently_truncating_file_list() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("one.txt"), "One.").unwrap();
        fs::write(directory.path().join("two.txt"), "Two.").unwrap();
        let mut settings = config(directory.path().to_owned());
        settings.max_files = 1;

        let error = discover(&settings).unwrap_err();
        assert!(error.to_string().contains("file limit exceeded"));
    }
}
