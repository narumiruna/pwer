use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use rusqlite::{Connection, params};
use tempfile::TempDir;

fn write_config(directory: &TempDir) -> (PathBuf, PathBuf) {
    let config_path = directory.path().join("config.toml");
    let database_path = directory.path().join("pwer.db");
    fs::write(
        &config_path,
        format!(
            "[tui]\ninterval = 1.0\nstats_limit = 100\nchart_limit = 60\n\n[database]\npath = {:?}\n\n[cli]\ndefault_history_limit = 20\ndefault_export_limit = 1000\n\n[logging]\nlevel = \"INFO\"\n",
            database_path.display().to_string()
        ),
    )
    .unwrap();
    (config_path, database_path)
}

fn command(config_path: &Path, home: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("pwer");
    command.env("PWER_CONFIG", config_path).env("HOME", home);
    command
}

fn seed_database(path: &Path) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE power_readings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME NOT NULL,
                watts_actual REAL NOT NULL,
                watts_negotiated INTEGER NOT NULL,
                voltage REAL NOT NULL,
                amperage REAL NOT NULL,
                current_capacity INTEGER NOT NULL,
                max_capacity INTEGER NOT NULL,
                battery_percent INTEGER NOT NULL,
                is_charging INTEGER NOT NULL,
                external_connected INTEGER NOT NULL,
                charger_name TEXT,
                charger_manufacturer TEXT
            );",
        )
        .unwrap();
    for index in 0..5 {
        connection
            .execute(
                "INSERT INTO power_readings VALUES (NULL, ?, ?, 67, 20, 2, 3500, 4709, 74, 1, 1, ?, ?)",
                params![
                    format!("2026-01-0{}T12:00:00+00:00", index + 1),
                    40.0 + f64::from(index),
                    "USB-C Power Adapter",
                    "Apple Inc."
                ],
            )
            .unwrap();
    }
}

#[test]
fn config_init_show_and_validate_work() {
    let directory = TempDir::new().unwrap();
    let config_path = directory.path().join("nested/config.toml");

    command(&config_path, directory.path())
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created config file"));
    assert!(config_path.exists());

    command(&config_path, directory.path())
        .args(["config", "validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Config file is valid"));

    command(&config_path, directory.path())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Effective Configuration"))
        .stdout(predicate::str::contains("Stats history limit"));
}

#[test]
fn config_validate_rejects_unknown_keys_and_bad_values() {
    let directory = TempDir::new().unwrap();
    let config_path = directory.path().join("config.toml");
    fs::write(&config_path, "[tui]\ninterval = 0\ntyop = true\n").unwrap();

    command(&config_path, directory.path())
        .args(["config", "validate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown key 'tyop'"))
        .stderr(predicate::str::contains(
            "collection_interval must be positive",
        ));
}

#[test]
fn history_stats_and_exports_use_configured_database() {
    let directory = TempDir::new().unwrap();
    let (config_path, database_path) = write_config(&directory);
    seed_database(&database_path);

    command(&config_path, directory.path())
        .args(["history", "--limit", "3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Recent Power Readings (Last 3)"))
        .stdout(predicate::str::contains("Charging"));

    command(&config_path, directory.path())
        .arg("stats")
        .assert()
        .success()
        .stdout(predicate::str::contains("Database Statistics"))
        .stdout(predicate::str::contains("Total readings    5"));

    let csv_path = directory.path().join("readings.csv");
    command(&config_path, directory.path())
        .args(["export", csv_path.to_str().unwrap(), "--limit", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Exported 2 readings"));
    let csv = fs::read_to_string(csv_path).unwrap();
    assert!(csv.starts_with("timestamp,watts_actual,watts_negotiated"));
    assert_eq!(csv.lines().count(), 3);

    let json_path = directory.path().join("readings.json");
    command(&config_path, directory.path())
        .args(["export", json_path.to_str().unwrap(), "--limit", "3"])
        .assert()
        .success();
    let json: serde_json::Value = serde_json::from_slice(&fs::read(json_path).unwrap()).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 3);
}

#[test]
fn cleanup_days_and_health_are_scriptable() {
    let directory = TempDir::new().unwrap();
    let (config_path, database_path) = write_config(&directory);
    seed_database(&database_path);

    command(&config_path, directory.path())
        .args(["health", "--days", "3650"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Battery Health Analysis"));

    command(&config_path, directory.path())
        .args(["cleanup", "--days", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted 5 old readings"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn default_tui_invocation_reports_platform_requirement() {
    let directory = TempDir::new().unwrap();
    let (config_path, _) = write_config(&directory);
    command(&config_path, directory.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("pwer only supports macOS"));
}
