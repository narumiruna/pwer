use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use toml::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct PwerConfig {
    pub collection_interval: f64,
    pub stats_history_limit: usize,
    pub chart_history_limit: usize,
    pub database_path: PathBuf,
    pub default_history_limit: usize,
    pub default_export_limit: usize,
    pub log_level: String,
}

impl Default for PwerConfig {
    fn default() -> Self {
        Self {
            collection_interval: 1.0,
            stats_history_limit: 100,
            chart_history_limit: 60,
            database_path: default_data_dir().join("pwer.db"),
            default_history_limit: 20,
            default_export_limit: 1000,
            log_level: "INFO".into(),
        }
    }
}

impl PwerConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        collection_interval: f64,
        stats_history_limit: usize,
        chart_history_limit: usize,
        database_path: PathBuf,
        default_history_limit: usize,
        default_export_limit: usize,
        log_level: String,
    ) -> Result<Self, String> {
        let config = Self {
            collection_interval,
            stats_history_limit,
            chart_history_limit,
            database_path: expand_tilde(database_path),
            default_history_limit,
            default_export_limit,
            log_level: log_level.to_uppercase(),
        };
        config.validate()?;
        if config.collection_interval < 0.1 {
            eprintln!(
                "WARNING: Very short collection interval ({}s) may cause high CPU usage. Recommended minimum: 0.5s",
                config.collection_interval
            );
        }
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.collection_interval.is_finite() || self.collection_interval <= 0.0 {
            return Err(format!(
                "collection_interval must be positive, got {}",
                self.collection_interval
            ));
        }
        for (name, value) in [
            ("stats_history_limit", self.stats_history_limit),
            ("chart_history_limit", self.chart_history_limit),
            ("default_history_limit", self.default_history_limit),
            ("default_export_limit", self.default_export_limit),
        ] {
            if value == 0 {
                return Err(format!("{name} must be positive, got {value}"));
            }
        }
        if !["DEBUG", "INFO", "WARNING", "ERROR"].contains(&self.log_level.as_str()) {
            return Err(format!(
                "log_level must be one of DEBUG, ERROR, INFO, WARNING, got {}.",
                self.log_level
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ConfigValidationResult {
    pub path: PathBuf,
    pub errors: Vec<String>,
}

impl ConfigValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    tui: Option<TuiConfig>,
    database: Option<DatabaseConfig>,
    cli: Option<CliConfig>,
    logging: Option<LoggingConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct TuiConfig {
    interval: Option<Value>,
    stats_limit: Option<Value>,
    chart_limit: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
struct DatabaseConfig {
    path: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
struct CliConfig {
    default_history_limit: Option<Value>,
    default_export_limit: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
struct LoggingConfig {
    level: Option<Value>,
}

pub fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pwer")
}

pub fn get_config_path() -> PathBuf {
    std::env::var_os("PWER_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_data_dir().join("config.toml"))
}

pub fn default_config_toml() -> String {
    let config = PwerConfig::default();
    let db_path = config
        .database_path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "# pwer configuration file\n\n[tui]\n# Data collection interval in seconds\ninterval = {:.1}\n# Number of readings for statistics\nstats_limit = {}\n# Number of readings to display in chart\nchart_limit = {}\n\n[database]\n# Database file location\npath = \"{}\"\n\n[cli]\n# Default limit for history command\ndefault_history_limit = {}\n# Default limit for export command\ndefault_export_limit = {}\n\n[logging]\n# Logging level: DEBUG, INFO, WARNING, ERROR\nlevel = \"{}\"\n",
        config.collection_interval,
        config.stats_history_limit,
        config.chart_history_limit,
        db_path,
        config.default_history_limit,
        config.default_export_limit,
        config.log_level
    )
}

pub fn load_config() -> PwerConfig {
    load_config_from(&get_config_path())
}

pub fn load_config_from(path: &Path) -> PwerConfig {
    let defaults = PwerConfig::default();
    if !path.exists() {
        return defaults;
    }

    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!(
                "WARNING: Failed to read config file {}: {error}",
                path.display()
            );
            return defaults;
        }
    };
    let raw: Value = match toml::from_str(&source) {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "WARNING: Failed to parse TOML config from {}: {error}",
                path.display()
            );
            return defaults;
        }
    };
    for issue in structure_issues(&raw, path) {
        eprintln!("WARNING: {issue}");
    }
    let file: FileConfig = match raw.try_into() {
        Ok(config) => config,
        Err(error) => {
            eprintln!(
                "WARNING: Failed to load config fields from {}: {error}",
                path.display()
            );
            return defaults;
        }
    };

    let interval = number(
        file.tui.as_ref().and_then(|v| v.interval.as_ref()),
        "tui.interval",
        defaults.collection_interval,
    );
    let stats_limit = positive_integer(
        file.tui.as_ref().and_then(|v| v.stats_limit.as_ref()),
        "tui.stats_limit",
        defaults.stats_history_limit,
    );
    let chart_limit = positive_integer(
        file.tui.as_ref().and_then(|v| v.chart_limit.as_ref()),
        "tui.chart_limit",
        defaults.chart_history_limit,
    );
    let history_limit = positive_integer(
        file.cli
            .as_ref()
            .and_then(|v| v.default_history_limit.as_ref()),
        "cli.default_history_limit",
        defaults.default_history_limit,
    );
    let export_limit = positive_integer(
        file.cli
            .as_ref()
            .and_then(|v| v.default_export_limit.as_ref()),
        "cli.default_export_limit",
        defaults.default_export_limit,
    );
    let database_path = string_value(
        file.database.as_ref().and_then(|v| v.path.as_ref()),
        "database.path",
        defaults.database_path.display().to_string(),
    );
    let log_level = string_value(
        file.logging.as_ref().and_then(|v| v.level.as_ref()),
        "logging.level",
        defaults.log_level.clone(),
    );

    match PwerConfig::new(
        interval,
        stats_limit,
        chart_limit,
        PathBuf::from(database_path),
        history_limit,
        export_limit,
        log_level,
    ) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("WARNING: Invalid configuration values: {error}; using defaults");
            defaults
        }
    }
}

pub fn validate_config_file(path: &Path) -> ConfigValidationResult {
    let mut errors = Vec::new();
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            errors.push(if path.exists() {
                format!("Failed to read config file {}: {error}", path.display())
            } else {
                format!("Config file does not exist: {}", path.display())
            });
            return ConfigValidationResult {
                path: path.into(),
                errors,
            };
        }
    };
    let raw: Value = match toml::from_str(&source) {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!(
                "Failed to parse TOML config from {}: {error}",
                path.display()
            ));
            return ConfigValidationResult {
                path: path.into(),
                errors,
            };
        }
    };
    errors.extend(structure_issues(&raw, path));

    let file: FileConfig = match raw.clone().try_into() {
        Ok(file) => file,
        Err(error) => {
            errors.push(format!(
                "Failed to load config fields from {}: {error}",
                path.display()
            ));
            return ConfigValidationResult {
                path: path.into(),
                errors,
            };
        }
    };
    let defaults = PwerConfig::default();
    let interval = strict_number(
        file.tui.as_ref().and_then(|v| v.interval.as_ref()),
        "tui.interval",
        defaults.collection_interval,
        &mut errors,
    );
    let stats_limit = strict_integer(
        file.tui.as_ref().and_then(|v| v.stats_limit.as_ref()),
        "tui.stats_limit",
        defaults.stats_history_limit,
        &mut errors,
    );
    let chart_limit = strict_integer(
        file.tui.as_ref().and_then(|v| v.chart_limit.as_ref()),
        "tui.chart_limit",
        defaults.chart_history_limit,
        &mut errors,
    );
    let history_limit = strict_integer(
        file.cli
            .as_ref()
            .and_then(|v| v.default_history_limit.as_ref()),
        "cli.default_history_limit",
        defaults.default_history_limit,
        &mut errors,
    );
    let export_limit = strict_integer(
        file.cli
            .as_ref()
            .and_then(|v| v.default_export_limit.as_ref()),
        "cli.default_export_limit",
        defaults.default_export_limit,
        &mut errors,
    );
    let database_path = strict_string(
        file.database.as_ref().and_then(|v| v.path.as_ref()),
        "database.path",
        defaults.database_path.display().to_string(),
        &mut errors,
    );
    let log_level = strict_string(
        file.logging.as_ref().and_then(|v| v.level.as_ref()),
        "logging.level",
        defaults.log_level,
        &mut errors,
    );

    if let Err(error) = PwerConfig::new(
        interval,
        stats_limit,
        chart_limit,
        database_path.into(),
        history_limit,
        export_limit,
        log_level,
    ) {
        errors.push(error);
    }
    ConfigValidationResult {
        path: path.into(),
        errors,
    }
}

pub fn init_config(path: &Path) -> Result<()> {
    if path.exists() {
        anyhow::bail!("Config file already exists at {}", path.display());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, default_config_toml())
        .with_context(|| format!("failed to write {}", path.display()))
}

fn structure_issues(value: &Value, path: &Path) -> Vec<String> {
    let schema: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::from([
        (
            "tui",
            BTreeSet::from(["interval", "stats_limit", "chart_limit"]),
        ),
        ("database", BTreeSet::from(["path"])),
        (
            "cli",
            BTreeSet::from(["default_history_limit", "default_export_limit"]),
        ),
        ("logging", BTreeSet::from(["level"])),
    ]);
    let Some(root) = value.as_table() else {
        return vec![format!(
            "Config file {} must contain TOML tables",
            path.display()
        )];
    };
    let mut issues = Vec::new();
    for (section, section_value) in root {
        let Some(valid_keys) = schema.get(section.as_str()) else {
            issues.push(format!(
                "Unknown config section [{section}] in {} - ignoring",
                path.display()
            ));
            continue;
        };
        let Some(table) = section_value.as_table() else {
            issues.push(format!(
                "Config section [{section}] in {} must be a table - ignoring section",
                path.display()
            ));
            continue;
        };
        for key in table.keys() {
            if !valid_keys.contains(key.as_str()) {
                issues.push(format!(
                    "Unknown key '{key}' in [{section}] section of {} - ignoring (valid keys: {})",
                    path.display(),
                    valid_keys.iter().copied().collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    issues
}

fn number(value: Option<&Value>, field: &str, default: f64) -> f64 {
    match value.and_then(value_as_f64) {
        Some(value) => value,
        None if value.is_some() => {
            eprintln!(
                "WARNING: Invalid '{field}' value; expected a number - using default value {default}"
            );
            default
        }
        None => default,
    }
}

fn positive_integer(value: Option<&Value>, field: &str, default: usize) -> usize {
    match value.and_then(value_as_usize) {
        Some(value) => value,
        None if value.is_some() => {
            eprintln!(
                "WARNING: Invalid '{field}' value; expected an integer - using default value {default}"
            );
            default
        }
        None => default,
    }
}

fn string_value(value: Option<&Value>, field: &str, default: String) -> String {
    match value.and_then(Value::as_str) {
        Some(value) => value.into(),
        None if value.is_some() => {
            eprintln!(
                "WARNING: Invalid '{field}' value; expected a string - using default value {default:?}"
            );
            default
        }
        None => default,
    }
}

fn strict_number(
    value: Option<&Value>,
    field: &str,
    default: f64,
    errors: &mut Vec<String>,
) -> f64 {
    match value.and_then(value_as_f64) {
        Some(value) => value,
        None if value.is_some() => {
            errors.push(format!("Invalid '{field}' value; expected a number"));
            default
        }
        None => default,
    }
}

fn strict_integer(
    value: Option<&Value>,
    field: &str,
    default: usize,
    errors: &mut Vec<String>,
) -> usize {
    match value.and_then(value_as_usize) {
        Some(value) => value,
        None if value.is_some() => {
            errors.push(format!("Invalid '{field}' value; expected an integer"));
            default
        }
        None => default,
    }
}

fn strict_string(
    value: Option<&Value>,
    field: &str,
    default: String,
    errors: &mut Vec<String>,
) -> String {
    match value.and_then(Value::as_str) {
        Some(value) => value.into(),
        None if value.is_some() => {
            errors.push(format!("Invalid '{field}' value; expected a string"));
            default
        }
        None => default,
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|v| v as f64))
        .or_else(|| value.as_str()?.parse().ok())
}

fn value_as_usize(value: &Value) -> Option<usize> {
    value
        .as_integer()
        .and_then(|v| usize::try_from(v).ok())
        .or_else(|| value.as_str()?.parse().ok())
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(rest) = text.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_validation_rejects_invalid_values() {
        assert!(
            PwerConfig::new(0.0, 100, 60, "x.db".into(), 20, 1000, "INFO".into())
                .unwrap_err()
                .contains("collection_interval")
        );
        assert!(
            PwerConfig::new(1.0, 0, 60, "x.db".into(), 20, 1000, "INFO".into())
                .unwrap_err()
                .contains("stats_history_limit")
        );
        assert!(
            PwerConfig::new(1.0, 100, 60, "x.db".into(), 20, 1000, "TRACE".into())
                .unwrap_err()
                .contains("log_level")
        );
    }

    #[test]
    fn load_config_preserves_valid_fields_and_defaults_invalid_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[tui]\ninterval = 'bad'\nstats_limit = 200\n[logging]\nlevel = 'debug'\n",
        )
        .unwrap();
        let config = load_config_from(&path);
        assert_eq!(config.collection_interval, 1.0);
        assert_eq!(config.stats_history_limit, 200);
        assert_eq!(config.log_level, "DEBUG");
    }

    #[test]
    fn strict_validation_reports_structure_and_value_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[tui]\ninterval = 0\ntyop = true\n[unknown]\nkey = 1\n",
        )
        .unwrap();
        let result = validate_config_file(&path);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Unknown key 'tyop'"))
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("Unknown config section [unknown]"))
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("collection_interval must be positive"))
        );
    }

    #[test]
    fn default_config_is_valid_toml() {
        let value: Value = toml::from_str(&default_config_toml()).unwrap();
        assert_eq!(value["tui"]["interval"].as_float(), Some(1.0));
        assert_eq!(value["logging"]["level"].as_str(), Some("INFO"));
    }
}
