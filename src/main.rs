mod cli;
mod collector;
mod config;
mod database;
mod logger;
mod models;
mod tui;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match cli.config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Error: Invalid configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let _logger = match logger::setup_logger(&config.log_level) {
        Ok(handle) => Some(handle),
        Err(error) => {
            eprintln!("Warning: logging is unavailable: {error}");
            None
        }
    };
    match cli::run(cli, config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("{error:#}");
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
