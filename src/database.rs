use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{Connection, params};

use crate::models::PowerReading;

#[derive(Debug, PartialEq)]
pub struct Statistics {
    pub avg_watts: f64,
    pub min_watts: f64,
    pub max_watts: f64,
    pub avg_battery: f64,
    pub earliest: Option<DateTime<Utc>>,
    pub latest: Option<DateTime<Utc>>,
    pub count: usize,
}

impl Default for Statistics {
    fn default() -> Self {
        Self {
            avg_watts: 0.0,
            min_watts: 0.0,
            max_watts: 0.0,
            avg_battery: 0.0,
            earliest: None,
            latest: None,
            count: 0,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct HealthPoint {
    pub date: String,
    pub average_max_capacity: f64,
    pub reading_count: usize,
}

pub struct Database {
    connection: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS power_readings (
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
            );
            CREATE INDEX IF NOT EXISTS idx_timestamp ON power_readings(timestamp DESC);",
        )?;
        Ok(Self {
            connection,
            path: path.into(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn insert_reading(&self, reading: &PowerReading) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO power_readings (
                timestamp, watts_actual, watts_negotiated, voltage, amperage,
                current_capacity, max_capacity, battery_percent, is_charging,
                external_connected, charger_name, charger_manufacturer
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                reading.timestamp.to_rfc3339(),
                reading.watts_actual,
                reading.watts_negotiated,
                reading.voltage,
                reading.amperage,
                reading.current_capacity,
                reading.max_capacity,
                reading.battery_percent,
                reading.is_charging,
                reading.external_connected,
                reading.charger_name,
                reading.charger_manufacturer,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn query_history(&self, limit: Option<usize>) -> Result<Vec<PowerReading>> {
        let sql = if limit.is_some() {
            "SELECT timestamp, watts_actual, watts_negotiated, voltage, amperage,
                    current_capacity, max_capacity, battery_percent, is_charging,
                    external_connected, charger_name, charger_manufacturer
             FROM power_readings ORDER BY datetime(timestamp) DESC, id DESC LIMIT ?"
        } else {
            "SELECT timestamp, watts_actual, watts_negotiated, voltage, amperage,
                    current_capacity, max_capacity, battery_percent, is_charging,
                    external_connected, charger_name, charger_manufacturer
             FROM power_readings ORDER BY datetime(timestamp) DESC, id DESC"
        };
        let mut statement = self.connection.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<PowerReading> {
            let timestamp: String = row.get(0)?;
            let parsed = parse_timestamp(&timestamp).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    error.into(),
                )
            })?;
            Ok(PowerReading {
                timestamp: parsed,
                watts_actual: row.get(1)?,
                watts_negotiated: row.get(2)?,
                voltage: row.get(3)?,
                amperage: row.get(4)?,
                current_capacity: row.get(5)?,
                max_capacity: row.get(6)?,
                battery_percent: row.get(7)?,
                is_charging: row.get(8)?,
                external_connected: row.get(9)?,
                charger_name: row.get(10)?,
                charger_manufacturer: row.get(11)?,
            })
        };
        let readings = if let Some(limit) = limit {
            statement
                .query_map([i64::try_from(limit)?], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            statement
                .query_map([], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(readings)
    }

    pub fn get_statistics(&self, limit: Option<usize>) -> Result<Statistics> {
        let readings = self.query_history(limit)?;
        if readings.is_empty() {
            return Ok(Statistics::default());
        }
        let count = readings.len();
        let avg_watts = readings.iter().map(|r| r.watts_actual).sum::<f64>() / count as f64;
        let avg_battery = readings
            .iter()
            .map(|r| r.battery_percent as f64)
            .sum::<f64>()
            / count as f64;
        Ok(Statistics {
            avg_watts,
            min_watts: readings
                .iter()
                .map(|r| r.watts_actual)
                .fold(f64::INFINITY, f64::min),
            max_watts: readings
                .iter()
                .map(|r| r.watts_actual)
                .fold(f64::NEG_INFINITY, f64::max),
            avg_battery,
            earliest: readings.iter().map(|r| r.timestamp).min(),
            latest: readings.iter().map(|r| r.timestamp).max(),
            count,
        })
    }

    pub fn clear_history(&self) -> Result<usize> {
        Ok(self.connection.execute("DELETE FROM power_readings", [])?)
    }

    pub fn cleanup_old_data(&self, days: u64) -> Result<usize> {
        Ok(self.connection.execute(
            "DELETE FROM power_readings WHERE datetime(timestamp) < datetime('now', ?)",
            [format!("-{days} days")],
        )?)
    }

    pub fn get_battery_health_trend(&self, days: u64) -> Result<Vec<HealthPoint>> {
        if days == 0 {
            anyhow::bail!("days must be a positive integer");
        }
        let mut statement = self.connection.prepare(
            "SELECT date(timestamp), AVG(max_capacity), COUNT(id)
             FROM power_readings
             WHERE datetime(timestamp) >= datetime('now', ?)
             GROUP BY date(timestamp)
             ORDER BY date(timestamp)",
        )?;
        let rows = statement.query_map([format!("-{days} days")], |row| {
            Ok(HealthPoint {
                date: row.get(0)?,
                average_max_capacity: row.get(1)?,
                reading_count: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Ok(timestamp.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%d %H:%M:%S%.f"] {
        if format.ends_with("%:z") {
            if let Ok(timestamp) = DateTime::parse_from_str(value, format) {
                return Ok(timestamp.with_timezone(&Utc));
            }
        } else if let Ok(timestamp) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(timestamp.and_utc());
        }
    }
    DateTime::parse_from_rfc3339(value).map(|timestamp| timestamp.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use tempfile::tempdir;

    use super::*;
    use crate::models::sample_reading;

    fn database() -> (tempfile::TempDir, Database) {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        (dir, db)
    }

    #[test]
    fn insert_and_query_history_returns_newest_first() {
        let (_dir, db) = database();
        let base = sample_reading();
        for index in 0..10 {
            let mut reading = base.clone();
            reading.timestamp += Duration::seconds(index);
            reading.battery_percent -= index;
            db.insert_reading(&reading).unwrap();
        }
        let history = db.query_history(Some(5)).unwrap();
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].battery_percent, 65);
        assert_eq!(history[4].battery_percent, 69);
    }

    #[test]
    fn statistics_and_clear_history_cover_empty_and_populated_data() {
        let (_dir, db) = database();
        assert_eq!(db.get_statistics(None).unwrap(), Statistics::default());
        for watts in [10.0, 20.0, 30.0, 40.0, 50.0] {
            let mut reading = sample_reading();
            reading.watts_actual = watts;
            db.insert_reading(&reading).unwrap();
        }
        let stats = db.get_statistics(None).unwrap();
        assert_eq!(stats.count, 5);
        assert_eq!(stats.avg_watts, 30.0);
        assert_eq!(stats.min_watts, 10.0);
        assert_eq!(stats.max_watts, 50.0);
        assert_eq!(db.clear_history().unwrap(), 5);
        assert!(db.query_history(None).unwrap().is_empty());
    }

    #[test]
    fn legacy_python_timestamps_are_readable() {
        let (_dir, db) = database();
        db.connection.execute(
            "INSERT INTO power_readings VALUES (NULL, ?, 1, 0, 12, 1, 1, 2, 50, 0, 0, NULL, NULL)",
            ["2025-12-28 12:00:00.123456"],
        ).unwrap();
        let reading = db.query_history(None).unwrap().pop().unwrap();
        assert_eq!(
            reading.timestamp.to_rfc3339(),
            "2025-12-28T12:00:00.123456+00:00"
        );
    }

    #[test]
    fn health_trend_groups_readings_by_day() {
        let (_dir, db) = database();
        let now = Utc::now();
        for day in 0..3 {
            for _ in 0..2 {
                let mut reading = sample_reading();
                reading.timestamp = now - Duration::days(day);
                reading.max_capacity = 4700 - day;
                db.insert_reading(&reading).unwrap();
            }
        }
        let trend = db.get_battery_health_trend(7).unwrap();
        assert_eq!(trend.len(), 3);
        assert!(trend.iter().all(|point| point.reading_count == 2));
    }
}
