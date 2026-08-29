use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PowerReading {
    pub timestamp: DateTime<Utc>,
    pub watts_actual: f64,
    pub watts_negotiated: i64,
    pub voltage: f64,
    pub amperage: f64,
    pub current_capacity: i64,
    pub max_capacity: i64,
    pub battery_percent: i64,
    pub is_charging: bool,
    pub external_connected: bool,
    pub charger_name: Option<String>,
    pub charger_manufacturer: Option<String>,
}

impl PowerReading {
    #[allow(dead_code)]
    pub fn calculate_watts(voltage: f64, amperage: f64) -> f64 {
        voltage * amperage
    }
}

#[cfg(test)]
pub fn sample_reading() -> PowerReading {
    PowerReading {
        timestamp: "2025-12-28T12:00:00Z"
            .parse()
            .expect("valid test timestamp"),
        watts_actual: 45.5,
        watts_negotiated: 67,
        voltage: 20.0,
        amperage: 2.275,
        current_capacity: 3500,
        max_capacity: 4709,
        battery_percent: 74,
        is_charging: true,
        external_connected: true,
        charger_name: Some("USB-C Power Adapter".into()),
        charger_manufacturer: Some("Apple Inc.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_watts_multiplies_voltage_and_current() {
        assert_eq!(PowerReading::calculate_watts(20.0, 2.275), 45.5);
        assert!(PowerReading::calculate_watts(12.5, -1.216) < 0.0);
    }
}
