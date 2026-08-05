use std::io::{self, Write};

use crate::{cli::Format, diagnostic::Report};

pub fn write(report: &Report, format: Format) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    match format {
        Format::Json => {
            serde_json::to_writer_pretty(&mut output, report)?;
            writeln!(output)?;
        }
        Format::Text => write_text(&mut output, report)?,
    }
    Ok(())
}

fn write_text(output: &mut impl Write, report: &Report) -> io::Result<()> {
    let mut previous_path = None;
    for diagnostic in &report.diagnostics {
        if previous_path != Some(diagnostic.path.as_str()) {
            if previous_path.is_some() {
                writeln!(output)?;
            }
            writeln!(output, "! {}", diagnostic.path)?;
            previous_path = Some(diagnostic.path.as_str());
        }
        writeln!(
            output,
            "  Warning line {} [{}]: {}",
            diagnostic.line, diagnostic.code, diagnostic.message
        )?;
        for line in diagnostic.source_line.lines() {
            writeln!(output, "  > {line}")?;
        }
        if let Some(help) = &diagnostic.help {
            writeln!(output, "    help: {help}")?;
        }
    }

    if !report.diagnostics.is_empty() {
        writeln!(output)?;
    }
    writeln!(output, "=======================================")?;
    if report.diagnostics.is_empty() {
        writeln!(output, "✓ No warnings found!")?;
        writeln!(output, "Files checked: {}", report.files_checked)?;
    } else {
        writeln!(
            output,
            "! Found {} warning(s) in {} file(s){}",
            report.diagnostics.len(),
            report.files_checked,
            if report.truncated {
                " (output truncated)"
            } else {
                ""
            }
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{Diagnostic, Report, Severity};

    use super::write_text;

    #[test]
    fn renders_source_below_grouped_diagnostics() {
        let report = Report {
            diagnostics: vec![Diagnostic {
                path: "guide.md".to_owned(),
                line: 4,
                column: 5,
                end_column: 14,
                source_line: "The valve is opened.".to_owned(),
                severity: Severity::Warning,
                code: "STE003",
                message: "possible passive voice".to_owned(),
                help: Some("Name the actor.".to_owned()),
            }],
            files_checked: 1,
            truncated: false,
        };
        let mut output = Vec::new();

        write_text(&mut output, &report).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("! guide.md\n"));
        assert!(output.contains("  Warning line 4 [STE003]: possible passive voice\n"));
        assert!(output.contains("  > The valve is opened.\n"));
        assert!(output.contains("    help: Name the actor.\n"));
    }

    #[test]
    fn gives_each_excerpt_line_a_gutter() {
        let report = Report {
            diagnostics: vec![Diagnostic {
                path: "guide.md".to_owned(),
                line: 4,
                column: 1,
                end_column: 2,
                source_line: "First line\nsecond line.".to_owned(),
                severity: Severity::Warning,
                code: "STE001",
                message: "long sentence".to_owned(),
                help: None,
            }],
            files_checked: 1,
            truncated: false,
        };
        let mut output = Vec::new();

        write_text(&mut output, &report).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("  > First line\n  > second line.\n"));
    }
}
