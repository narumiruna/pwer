#![cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]

use std::io::Cursor;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::time::Duration;

use chrono::Utc;
use plist::{Dictionary, Value};
use thiserror::Error;

use crate::models::PowerReading;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum CollectorError {
    #[error("pwer only supports macOS")]
    UnsupportedPlatform,
    #[error("ioreg command failed: {0}")]
    CommandFailed(String),
    #[error("failed to parse plist data: {0}")]
    Parse(String),
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("IOKit/SMC operation failed: {0}")]
    Smc(String),
}

#[derive(Default)]
pub struct IoregCollector;

impl IoregCollector {
    pub fn collect(&self) -> Result<PowerReading, CollectorError> {
        #[cfg(not(target_os = "macos"))]
        return Err(CollectorError::UnsupportedPlatform);

        #[cfg(target_os = "macos")]
        {
            let mut child = Command::new("ioreg")
                .args(["-rw0", "-c", "AppleSmartBattery", "-a"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| CollectorError::CommandFailed(error.to_string()))?;
            let started = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let output = child
                            .wait_with_output()
                            .map_err(|error| CollectorError::CommandFailed(error.to_string()))?;
                        if !status.success() {
                            return Err(CollectorError::CommandFailed(
                                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                            ));
                        }
                        return self.parse_plist(&output.stdout);
                    }
                    Ok(None) if started.elapsed() < Duration::from_secs(10) => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(CollectorError::CommandFailed(
                            "ioreg command timed out after 10 seconds".into(),
                        ));
                    }
                    Err(error) => return Err(CollectorError::CommandFailed(error.to_string())),
                }
            }
        }
    }

    pub fn parse_plist(&self, bytes: &[u8]) -> Result<PowerReading, CollectorError> {
        let value = Value::from_reader(Cursor::new(bytes))
            .map_err(|error| CollectorError::Parse(error.to_string()))?;
        let batteries = value.as_array().ok_or_else(|| {
            CollectorError::Parse(
                "unexpected plist structure: expected array with battery data".into(),
            )
        })?;
        let battery = batteries
            .first()
            .and_then(Value::as_dictionary)
            .ok_or_else(|| {
                CollectorError::Parse(
                    "unexpected plist structure: expected array with battery data".into(),
                )
            })?;
        self.parse_battery_data(battery)
    }

    pub fn parse_battery_data(&self, battery: &Dictionary) -> Result<PowerReading, CollectorError> {
        let voltage_mv = integer(battery, "Voltage")
            .ok_or_else(|| CollectorError::MissingField("Voltage".into()))?;
        let amperage_ma = integer(battery, "Amperage")
            .ok_or_else(|| CollectorError::MissingField("Amperage".into()))?;
        let voltage = voltage_mv as f64 / 1000.0;
        let amperage = amperage_ma as f64 / 1000.0;
        let current_capacity = integer(battery, "AppleRawCurrentCapacity")
            .or_else(|| integer(battery, "CurrentCapacity"))
            .unwrap_or(0);
        let max_capacity = integer(battery, "AppleRawMaxCapacity")
            .or_else(|| integer(battery, "MaxCapacity"))
            .unwrap_or(1);
        let battery_percent = if max_capacity > 0 {
            (current_capacity as f64 / max_capacity as f64 * 100.0).round() as i64
        } else {
            0
        };

        let mut watts_negotiated = 0;
        let mut charger_name = None;
        let mut charger_manufacturer = None;
        if let Some(adapter) = battery
            .get("AppleRawAdapterDetails")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_dictionary)
        {
            watts_negotiated = integer(adapter, "Watts").unwrap_or(0);
            charger_name = text(adapter, "Name").map(str::to_owned);
            charger_manufacturer = text(adapter, "Manufacturer").map(str::to_owned);
        }

        Ok(PowerReading {
            timestamp: Utc::now(),
            watts_actual: PowerReading::calculate_watts(voltage, amperage),
            watts_negotiated,
            voltage,
            amperage,
            current_capacity,
            max_capacity,
            battery_percent,
            is_charging: boolean(battery, "IsCharging").unwrap_or(false),
            external_connected: boolean(battery, "ExternalConnected").unwrap_or(false),
            charger_name,
            charger_manufacturer,
        })
    }
}

fn integer(dictionary: &Dictionary, key: &str) -> Option<i64> {
    dictionary.get(key)?.as_signed_integer()
}

fn boolean(dictionary: &Dictionary, key: &str) -> Option<bool> {
    dictionary.get(key)?.as_boolean()
}

fn text<'a>(dictionary: &'a Dictionary, key: &str) -> Option<&'a str> {
    dictionary.get(key)?.as_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery() -> Dictionary {
        let mut battery = Dictionary::new();
        battery.insert("Voltage".into(), 20_000_i64.into());
        battery.insert("Amperage".into(), 2_000_i64.into());
        battery.insert("CurrentCapacity".into(), 3_000_i64.into());
        battery.insert("MaxCapacity".into(), 4_000_i64.into());
        battery.insert("IsCharging".into(), true.into());
        battery.insert("ExternalConnected".into(), true.into());
        battery
    }

    #[test]
    fn parse_battery_converts_units_and_calculates_power() {
        let reading = IoregCollector.parse_battery_data(&battery()).unwrap();
        assert_eq!(reading.voltage, 20.0);
        assert_eq!(reading.amperage, 2.0);
        assert_eq!(reading.watts_actual, 40.0);
        assert_eq!(reading.battery_percent, 75);
    }

    #[test]
    fn parse_battery_supports_discharge_and_adapter_details() {
        let mut data = battery();
        data.insert("Amperage".into(), (-1_500_i64).into());
        let mut adapter = Dictionary::new();
        adapter.insert("Watts".into(), 67_i64.into());
        adapter.insert("Name".into(), "USB-C Power Adapter".into());
        adapter.insert("Manufacturer".into(), "Apple Inc.".into());
        data.insert(
            "AppleRawAdapterDetails".into(),
            vec![Value::Dictionary(adapter)].into(),
        );
        let reading = IoregCollector.parse_battery_data(&data).unwrap();
        assert!(reading.watts_actual < 0.0);
        assert_eq!(reading.watts_negotiated, 67);
        assert_eq!(reading.charger_manufacturer.as_deref(), Some("Apple Inc."));
    }

    #[test]
    fn parse_fixture_accepts_real_ioreg_output() {
        let fixture = include_bytes!("../../tests/fixtures/real_mac.txt");
        let reading = IoregCollector.parse_plist(fixture).unwrap();
        assert!((10.0..=21.0).contains(&reading.voltage));
        assert!((0..=100).contains(&reading.battery_percent));
    }

    #[test]
    fn parse_battery_requires_voltage_and_amperage() {
        let error = IoregCollector
            .parse_battery_data(&Dictionary::new())
            .unwrap_err();
        assert!(error.to_string().contains("Voltage"));
    }
}
