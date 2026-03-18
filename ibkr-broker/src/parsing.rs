use chrono::{DateTime, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use std::error::Error;

pub(crate) fn parse_ibkr_datetime(value: &str) -> Option<NaiveDateTime> {
    for format in ["%Y%m%d-%H:%M:%S", "%Y%m%d %H:%M:%S", "%y%m%d%H%M%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed);
        }
    }
    None
}

pub(crate) fn trade_timestamp(value: &Value) -> Option<NaiveDateTime> {
    string_field_optional(value, "trade_time")
        .as_deref()
        .and_then(parse_ibkr_datetime)
        .or_else(|| {
            string_field_optional(value, "date_time")
                .as_deref()
                .and_then(parse_ibkr_datetime)
        })
        .or_else(|| timestamp_field(value, "_updated").map(|timestamp| timestamp.naive_utc()))
}

pub(crate) fn timestamp_field(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    parse_epoch_datetime(value.get(key))
}

pub(crate) fn parse_epoch_datetime(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let millis = match value {
        Some(Value::Number(number)) => number.as_i64()?,
        Some(Value::String(text)) => text.parse::<i64>().ok()?,
        _ => return None,
    };
    DateTime::<Utc>::from_timestamp_millis(millis)
}

pub(crate) fn string_field_optional(value: &Value, key: &str) -> Option<String> {
    match value.get(key) {
        Some(Value::String(text)) => Some(text.to_string()),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(flag)) => Some(flag.to_string()),
        _ => None,
    }
}

pub(crate) fn decimal_field(value: &Value, key: &str) -> Result<Decimal, Box<dyn Error>> {
    decimal_field_optional(value, key)
        .ok_or_else(|| format!("IBKR payload missing decimal field '{key}'").into())
}

pub(crate) fn decimal_field_optional(value: &Value, key: &str) -> Option<Decimal> {
    decimal_from_value(value.get(key))
}

pub(crate) fn decimal_field_any(value: &Value, keys: &[&str]) -> Result<Decimal, Box<dyn Error>> {
    decimal_field_optional_any(value, keys)
        .ok_or_else(|| format!("IBKR payload missing decimal field from {:?}", keys).into())
}

pub(crate) fn decimal_field_optional_any(value: &Value, keys: &[&str]) -> Option<Decimal> {
    keys.iter()
        .find_map(|key| decimal_field_optional(value, key))
}

fn decimal_from_value(value: Option<&Value>) -> Option<Decimal> {
    match value {
        Some(Value::String(text)) => {
            let normalized = text.replace(',', "");
            normalized.parse::<Decimal>().ok()
        }
        Some(Value::Number(number)) => number.to_string().parse::<Decimal>().ok(),
        _ => None,
    }
}

pub(crate) fn u64_field_optional(value: &Value, key: &str) -> Option<u64> {
    u64_from_value(value.get(key))
}

pub(crate) fn u64_field_optional_any(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| u64_field_optional(value, key))
}

fn u64_from_value(value: Option<&Value>) -> Option<u64> {
    match value {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.replace(',', "").parse::<u64>().ok(),
        _ => None,
    }
}
