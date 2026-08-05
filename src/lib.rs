mod cli;
mod config;
mod diagnostic;
mod discover;
mod document;
mod engine;
mod output;
mod rules;

use std::process::ExitCode;

use clap::Parser;

use crate::{cli::Cli, config::Config};

pub fn run() -> ExitCode {
    match run_inner() {
        Ok(has_findings) if has_findings => ExitCode::from(1),
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run_inner() -> anyhow::Result<bool> {
    let config = Config::try_from(Cli::parse())?;
    let inputs = discover::discover(&config)?;
    let report = engine::lint(inputs, &config)?;
    output::write(&report, config.format)?;
    Ok(!report.diagnostics.is_empty())
}
