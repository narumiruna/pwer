pub fn str_to_key(value: &str) -> u32 {
    value
        .as_bytes()
        .try_into()
        .map(u32::from_be_bytes)
        .unwrap_or(0)
}

pub fn type_to_string(value: u32) -> String {
    String::from_utf8_lossy(&value.to_be_bytes()).into_owned()
}

pub fn bytes_to_float(data: &[u8], data_type: &str) -> Option<f64> {
    let padded = format!("{data_type:<4}");
    let value = match padded.as_str() {
        "sp78" | "sp87" | "sp96" | "spa5" | "spb4" | "spf0" if data.len() >= 2 => {
            i16::from_be_bytes([data[0], data[1]]) as f64 / 256.0
        }
        "fp88" | "fp79" | "fp6a" | "fp4c" if data.len() >= 2 => {
            u16::from_be_bytes([data[0], data[1]]) as f64 / 256.0
        }
        "flt " if data.len() >= 4 => {
            f32::from_le_bytes([data[0], data[1], data[2], data[3]]) as f64
        }
        "ui8 " if !data.is_empty() => data[0] as f64,
        "ui16" if data.len() >= 2 => u16::from_be_bytes([data[0], data[1]]) as f64,
        "ui32" if data.len() >= 4 => {
            u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as f64
        }
        _ => return None,
    };
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smc_key_conversion_round_trips() {
        assert_eq!(type_to_string(str_to_key("PDTR")), "PDTR");
        assert_eq!(str_to_key("bad"), 0);
    }

    #[test]
    fn smc_numeric_types_use_their_wire_byte_order() {
        assert_eq!(
            bytes_to_float(&10_240_i16.to_be_bytes(), "sp78"),
            Some(40.0)
        );
        assert_eq!(bytes_to_float(&4_096_u16.to_be_bytes(), "fp88"), Some(16.0));
        assert_eq!(bytes_to_float(&45.5_f32.to_le_bytes(), "flt "), Some(45.5));
        assert_eq!(bytes_to_float(&[100], "ui8 "), Some(100.0));
        assert_eq!(
            bytes_to_float(&1_000_u16.to_be_bytes(), "ui16"),
            Some(1_000.0)
        );
        assert_eq!(
            bytes_to_float(&100_000_u32.to_be_bytes(), "ui32"),
            Some(100_000.0)
        );
    }

    #[test]
    fn smc_parser_rejects_malformed_and_unknown_values() {
        assert_eq!(
            bytes_to_float(&(-2_688_i16).to_be_bytes(), "sp78"),
            Some(-10.5)
        );
        assert_eq!(bytes_to_float(&[0], "sp78"), None);
        assert_eq!(bytes_to_float(&1_234_u16.to_be_bytes(), "xxxx"), None);
        assert_eq!(bytes_to_float(&f32::NAN.to_le_bytes(), "flt "), None);
    }
}
