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

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::{
        decimal_field, decimal_field_any, decimal_field_optional, parse_epoch_datetime,
        parse_ibkr_datetime, string_field_optional, timestamp_field, trade_timestamp,
        u64_field_optional, u64_field_optional_any,
    };
    use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
    use rust_decimal_macros::dec;
    use serde_json::json;

    fn naive_datetime() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 18)
            .expect("valid date")
            .and_hms_opt(12, 34, 56)
            .expect("valid time")
    }

    fn utc_datetime() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-03-18T12:34:56Z")
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn parse_ibkr_datetime_accepts_all_known_shapes() {
        let expected = naive_datetime();

        assert_eq!(parse_ibkr_datetime("20260318-12:34:56"), Some(expected));
        assert_eq!(parse_ibkr_datetime("20260318 12:34:56"), Some(expected));
        assert_eq!(parse_ibkr_datetime("260318123456"), Some(expected));
        assert_eq!(parse_ibkr_datetime("2026-03-18T12:34:56Z"), None);
    }

    #[test]
    fn trade_timestamp_prefers_explicit_trade_time_then_fallbacks() {
        let expected = naive_datetime();
        let updated = utc_datetime();

        assert_eq!(
            trade_timestamp(&json!({
                "trade_time": "20260318-12:34:56",
                "date_time": "20260319-12:34:56",
                "_updated": updated.timestamp_millis()
            })),
            Some(expected)
        );
        assert_eq!(
            trade_timestamp(&json!({
                "date_time": "20260318 12:34:56",
                "_updated": updated.timestamp_millis()
            })),
            Some(expected)
        );
        assert_eq!(
            trade_timestamp(&json!({ "_updated": updated.timestamp_millis() })),
            Some(updated.naive_utc())
        );
        assert_eq!(trade_timestamp(&json!({})), None);
    }

    #[test]
    fn timestamp_and_string_fields_handle_supported_shapes() {
        let updated = utc_datetime();
        let payload = json!({
            "millis": updated.timestamp_millis(),
            "millis_text": updated.timestamp_millis().to_string(),
            "bad_text": "not-a-timestamp",
            "float": 12.34,
            "number_text": 42,
            "flag": false
        });

        assert_eq!(timestamp_field(&payload, "millis"), Some(updated));
        assert_eq!(
            parse_epoch_datetime(payload.get("millis_text")),
            Some(updated)
        );
        assert_eq!(parse_epoch_datetime(payload.get("bad_text")), None);
        assert_eq!(parse_epoch_datetime(payload.get("float")), None);
        assert_eq!(parse_epoch_datetime(payload.get("missing")), None);
        assert_eq!(
            string_field_optional(&payload, "number_text").as_deref(),
            Some("42")
        );
        assert_eq!(
            string_field_optional(&payload, "flag").as_deref(),
            Some("false")
        );
        assert_eq!(string_field_optional(&payload, "missing"), None);
    }

    #[test]
    fn decimal_fields_parse_supported_shapes_and_report_missing_keys() {
        let payload = json!({
            "price": "1,234.56",
            "backup": "99.01",
            "number": 42.5,
            "bad": "not-decimal"
        });

        assert_eq!(
            decimal_field(&payload, "price").expect("comma decimal parses"),
            dec!(1234.56)
        );
        assert_eq!(decimal_field_optional(&payload, "number"), Some(dec!(42.5)));
        assert_eq!(decimal_field_optional(&payload, "bad"), None);
        assert_eq!(
            decimal_field_any(&payload, &["missing", "backup"]).expect("fallback decimal parses"),
            dec!(99.01)
        );

        let missing_decimal =
            decimal_field(&payload, "missing").expect_err("missing decimal should fail");
        assert_eq!(
            missing_decimal.to_string(),
            "IBKR payload missing decimal field 'missing'"
        );
        let missing_any = decimal_field_any(&payload, &["missing", "also_missing"])
            .expect_err("missing decimal alternatives should fail");
        assert_eq!(
            missing_any.to_string(),
            "IBKR payload missing decimal field from [\"missing\", \"also_missing\"]"
        );
    }

    #[test]
    fn u64_fields_parse_supported_shapes_and_search_alternates() {
        let payload = json!({
            "shares": "1,250",
            "number": 15,
            "negative": -1,
            "bad": "not-u64",
            "fallback": "7"
        });

        assert_eq!(u64_field_optional(&payload, "shares"), Some(1250));
        assert_eq!(u64_field_optional(&payload, "number"), Some(15));
        assert_eq!(u64_field_optional(&payload, "negative"), None);
        assert_eq!(u64_field_optional(&payload, "bad"), None);
        assert_eq!(
            u64_field_optional_any(&payload, &["missing", "fallback"]),
            Some(7)
        );
        assert_eq!(u64_field_optional_any(&payload, &["missing", "bad"]), None);
    }
}
