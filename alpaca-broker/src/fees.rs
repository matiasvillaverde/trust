use crate::keys;
use apca::api::v2::account_activities::{Activity, ActivityReq, ActivityType, Direction, Get};
use apca::Client;
use chrono::{DateTime, Utc};
use model::{Account, FeeActivity, Trade};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tokio::runtime::Runtime;
use uuid::Uuid;

const FEE_PAGE_SIZE: usize = 100;
const MAX_FEE_PAGES: usize = 16;

fn num_to_decimal(n: &num_decimal::Num) -> Result<Decimal, Box<dyn Error>> {
    Decimal::from_str(&n.to_string())
        .map_err(|e| format!("failed to parse num as decimal: {e}").into())
}

fn fee_activity_from_activity(
    activity: Activity,
    symbol: &str,
    account_id: Uuid,
) -> Result<Option<FeeActivity>, Box<dyn Error>> {
    let Activity::NonTrade(non_trade) = activity else {
        return Ok(None);
    };

    if let Some(activity_symbol) = &non_trade.symbol {
        if activity_symbol != symbol {
            return Ok(None);
        }
    }

    let amount = num_to_decimal(&non_trade.net_amount)?.abs();
    if amount <= Decimal::ZERO {
        return Ok(None);
    }

    Ok(Some(FeeActivity {
        broker: "alpaca".to_string(),
        broker_activity_id: non_trade.id,
        account_id,
        broker_order_id: None,
        symbol: non_trade.symbol,
        activity_type: format!("{:?}", non_trade.type_),
        amount,
        occurred_at: non_trade.date.naive_utc(),
        raw_json: None,
    }))
}

fn fee_activity_request(after: Option<DateTime<Utc>>, page_token: Option<String>) -> ActivityReq {
    ActivityReq {
        types: vec![ActivityType::Fee, ActivityType::PassThruCharge],
        direction: Direction::Ascending,
        after,
        until: None,
        page_size: Some(FEE_PAGE_SIZE),
        page_token,
        ..Default::default()
    }
}

fn next_page_token(activities: &[Activity]) -> Option<String> {
    activities.last().map(|activity| activity.id().to_string())
}

fn append_fee_activities_from_activities(
    out: &mut Vec<FeeActivity>,
    activities: Vec<Activity>,
    symbol: &str,
    account_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    for activity in activities {
        if let Some(fee) = fee_activity_from_activity(activity, symbol, account_id)? {
            out.push(fee);
        }
    }
    Ok(())
}

fn should_stop_after_page(page_len: usize) -> bool {
    page_len < FEE_PAGE_SIZE
}

pub fn fetch_fee_activities(
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
    crate::ensure_trade_account(trade, account)?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let symbol = trade.trading_vehicle.symbol.clone();
    let rt = Runtime::new().map_err(|e| Box::new(e) as Box<dyn Error>)?;
    rt.block_on(async move {
        let mut page_token: Option<String> = None;
        let mut out: Vec<FeeActivity> = vec![];

        for _ in 0..MAX_FEE_PAGES {
            let req = fee_activity_request(after, page_token.clone());
            let activities: Vec<Activity> = client
                .issue::<Get>(&req)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error>)?;

            let page_len = activities.len();
            if page_len == 0 {
                break;
            }
            page_token = next_page_token(&activities);

            append_fee_activities_from_activities(&mut out, activities, &symbol, account.id)?;

            if should_stop_after_page(page_len) {
                break;
            }
        }

        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        append_fee_activities_from_activities, fee_activity_from_activity, fee_activity_request,
        next_page_token, num_to_decimal, should_stop_after_page, FEE_PAGE_SIZE,
    };
    use apca::api::v2::account_activities::Activity;
    use chrono::{DateTime, Utc};
    use std::str::FromStr;
    use uuid::Uuid;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    fn fee_activity(id: &str, symbol: Option<&str>, amount: &str) -> Activity {
        let symbol_field = symbol
            .map(|symbol| format!(r#","symbol":"{symbol}""#))
            .unwrap_or_default();
        let raw = format!(
            r#"{{
                "activity_type": "FEE",
                "id": "{id}",
                "date": "2026-05-07",
                "net_amount": "{amount}"{symbol_field}
            }}"#
        );
        serde_json::from_str(&raw).expect("valid fee")
    }

    #[test]
    fn num_to_decimal_parses_valid_num() {
        let n = num_decimal::Num::from_str("123.45").expect("num parse");
        let d = num_to_decimal(&n).expect("decimal parse");
        assert_eq!(d.to_string(), "123.45");
    }

    #[test]
    fn fee_activity_from_activity_maps_matching_non_trade_fee() {
        let account_id = Uuid::new_v4();
        let raw = r#"{
            "activity_type": "FEE",
            "id": "fee-1",
            "date": "2026-05-07",
            "net_amount": "-0.34",
            "symbol": "AAPL"
        }"#;
        let activity: Activity = serde_json::from_str(raw).expect("valid fee");

        let fee = fee_activity_from_activity(activity, "AAPL", account_id)
            .expect("conversion succeeds")
            .expect("matching fee");

        assert_eq!(fee.broker, "alpaca");
        assert_eq!(fee.broker_activity_id, "fee-1");
        assert_eq!(fee.account_id, account_id);
        assert_eq!(fee.broker_order_id, None);
        assert_eq!(fee.symbol, Some("AAPL".to_string()));
        assert_eq!(fee.activity_type, "Fee");
        assert_eq!(fee.amount.to_string(), "0.34");
        assert_eq!(fee.occurred_at.to_string(), "2026-05-07 00:00:00");
        assert_eq!(fee.raw_json, None);
    }

    #[test]
    fn fee_activity_from_activity_accepts_account_level_fee_without_symbol() {
        let account_id = Uuid::new_v4();
        let raw = r#"{
            "activity_type": "PTC",
            "id": "fee-2",
            "date": "2026-05-07",
            "net_amount": "0.02"
        }"#;
        let activity: Activity = serde_json::from_str(raw).expect("valid account-level fee");

        let fee = fee_activity_from_activity(activity, "AAPL", account_id)
            .expect("conversion succeeds")
            .expect("account-level fee");

        assert_eq!(fee.symbol, None);
        assert_eq!(fee.activity_type, "PassThruCharge");
        assert_eq!(fee.amount.to_string(), "0.02");
    }

    #[test]
    fn fee_activity_from_activity_ignores_mismatched_zero_or_trade_activity() {
        let account_id = Uuid::new_v4();
        let mismatched_raw = r#"{
            "activity_type": "FEE",
            "id": "fee-3",
            "date": "2026-05-07",
            "net_amount": "-0.34",
            "symbol": "MSFT"
        }"#;
        let zero_raw = r#"{
            "activity_type": "FEE",
            "id": "fee-4",
            "date": "2026-05-07",
            "net_amount": "0",
            "symbol": "AAPL"
        }"#;
        let trade_raw = r#"{
            "activity_type": "FILL",
            "cum_qty": "1",
            "id": "fill-1",
            "leaves_qty": "0",
            "price": "184.125",
            "qty": "1",
            "side": "buy",
            "symbol": "AAPL",
            "transaction_time": "2026-05-07T13:00:06Z",
            "order_id": "904837e3-3b76-47ec-b432-046db621571b",
            "type": "fill"
        }"#;

        let mismatched: Activity = serde_json::from_str(mismatched_raw).expect("valid fee");
        let zero: Activity = serde_json::from_str(zero_raw).expect("valid zero fee");
        let trade: Activity = serde_json::from_str(trade_raw).expect("valid trade");

        assert!(fee_activity_from_activity(mismatched, "AAPL", account_id)
            .expect("conversion succeeds")
            .is_none());
        assert!(fee_activity_from_activity(zero, "AAPL", account_id)
            .expect("conversion succeeds")
            .is_none());
        assert!(fee_activity_from_activity(trade, "AAPL", account_id)
            .expect("conversion succeeds")
            .is_none());
    }

    #[test]
    fn fee_activity_request_uses_fee_paging_contract() {
        let after = utc("2026-05-07T12:00:00Z");
        let req = fee_activity_request(Some(after), Some("fee-page".to_string()));

        assert_eq!(
            req.types,
            vec![
                apca::api::v2::account_activities::ActivityType::Fee,
                apca::api::v2::account_activities::ActivityType::PassThruCharge,
            ]
        );
        assert_eq!(
            req.direction,
            apca::api::v2::account_activities::Direction::Ascending
        );
        assert_eq!(req.after, Some(after));
        assert_eq!(req.until, None);
        assert_eq!(req.page_size, Some(FEE_PAGE_SIZE));
        assert_eq!(req.page_token.as_deref(), Some("fee-page"));
    }

    #[test]
    fn next_page_token_uses_last_raw_fee_activity_id() {
        let activities = vec![
            fee_activity("fee-a", Some("AAPL"), "-0.01"),
            fee_activity("fee-b", Some("MSFT"), "-0.02"),
        ];

        assert_eq!(next_page_token(&activities).as_deref(), Some("fee-b"));
        assert_eq!(next_page_token(&[]), None);
    }

    #[test]
    fn append_fee_activities_filters_symbol_without_affecting_page_stop_logic() {
        let account_id = Uuid::new_v4();
        let mut out = Vec::new();
        let activities = vec![
            fee_activity("fee-a", Some("AAPL"), "-0.01"),
            fee_activity("fee-b", Some("MSFT"), "-0.02"),
            fee_activity("fee-c", None, "-0.03"),
            fee_activity("fee-zero", Some("AAPL"), "0"),
        ];

        append_fee_activities_from_activities(&mut out, activities, "AAPL", account_id)
            .expect("activities should filter");

        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|fee| fee.broker_activity_id == "fee-a"));
        assert!(out.iter().any(|fee| fee.broker_activity_id == "fee-c"));
        assert!(should_stop_after_page(FEE_PAGE_SIZE - 1));
        assert!(!should_stop_after_page(FEE_PAGE_SIZE));
    }
}
