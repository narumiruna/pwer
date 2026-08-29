use std::fs;

use anyhow::{Context, Result};
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming};

use crate::config::default_data_dir;

pub fn setup_logger(level: &str) -> Result<LoggerHandle> {
    let directory = default_data_dir();
    fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create {}", directory.display()))?;
    Logger::try_with_str(level)?
        .log_to_file(
            FileSpec::default()
                .directory(directory)
                .basename("pwer")
                .suffix("log"),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .rotate(
            Criterion::Size(10_000_000),
            Naming::Timestamps,
            Cleanup::KeepLogFiles(7),
        )
        .start()
        .context("failed to initialize logging")
}
