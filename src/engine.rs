use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read},
    path::Path,
};

use anyhow::{Context, ensure};
use rayon::prelude::*;

use crate::{
    config::Config,
    diagnostic::{Diagnostic, Report},
    discover::Input,
    document::Document,
    rules,
};

struct FileResult {
    path: String,
    document: Document,
}

pub fn lint(inputs: Vec<Input>, config: &Config) -> anyhow::Result<Report> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.jobs)
        .build()
        .context("failed to create worker pool")?;

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut truncated = false;
    for batch in inputs.chunks(config.jobs) {
        let results: Vec<anyhow::Result<FileResult>> = pool.install(|| {
            batch
                .par_iter()
                .map(|input| load(input, config.max_file_bytes))
                .collect()
        });

        for result in results {
            let result = result?;
            let remaining = config.max_diagnostics.saturating_sub(diagnostics.len());
            let findings =
                rules::check(&result.document, config, remaining.saturating_add(1).max(1));
            if findings.len() > remaining {
                truncated = true;
            }
            diagnostics.extend(
                findings
                    .into_iter()
                    .take(remaining)
                    .map(|finding| rules::into_diagnostic(&result.path, &result.document, finding)),
            );
        }
    }

    Ok(Report {
        diagnostics,
        files_checked: inputs.len(),
        truncated,
    })
}

fn load(input: &Input, max_bytes: u64) -> anyhow::Result<FileResult> {
    match input {
        Input::File(path) => {
            let file =
                File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
            let source = read_bounded(file, max_bytes)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let markdown = is_markdown(path);
            Ok(FileResult {
                path: path.display().to_string(),
                document: Document::new(source, markdown),
            })
        }
        Input::Stdin => Ok(FileResult {
            path: "<stdin>".to_owned(),
            document: Document::new(read_bounded(io::stdin().lock(), max_bytes)?, false),
        }),
    }
}

fn read_bounded(reader: impl Read, max_bytes: u64) -> anyhow::Result<String> {
    let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024) as usize);
    reader.take(max_bytes + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= max_bytes,
        "input exceeds the {max_bytes}-byte limit"
    );
    String::from_utf8(bytes).context("input is not valid UTF-8")
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "mdown" | "mkdn"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::read_bounded;

    #[test]
    fn rejects_oversized_input() {
        let error = read_bounded(&b"12345"[..], 4).unwrap_err();
        assert!(error.to_string().contains("4-byte limit"));
    }
}
