use crate::keys;
use apca::api::v2::account_activities::{Activity, ActivityReq, ActivityType, Direction, Get};
use apca::Client;
use chrono::{DateTime, Utc};
use model::{Account, Execution, ExecutionSide, ExecutionSource, Trade};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tokio::runtime::Runtime;
use uuid::Uuid;

const EXECUTION_PAGE_SIZE: usize = 100;
const MAX_EXECUTION_PAGES: usize = 32;

fn map_side(side: apca::api::v2::account_activities::Side) -> ExecutionSide {
    match side {
        apca::api::v2::account_activities::Side::Buy => ExecutionSide::Buy,
        apca::api::v2::account_activities::Side::Sell => ExecutionSide::Sell,
        apca::api::v2::account_activities::Side::ShortSell => ExecutionSide::SellShort,
        _ => ExecutionSide::Sell, // Fallback for non-exhaustive variants
    }
}

fn num_to_decimal(n: &num_decimal::Num) -> Result<Decimal, Box<dyn Error>> {
    Decimal::from_str(&n.to_string())
        .map_err(|e| format!("failed to parse num as decimal: {e}").into())
}

fn execution_from_activity(
    activity: Activity,
    symbol: &str,
    account_id: Uuid,
) -> Result<Option<Execution>, Box<dyn Error>> {
    let Ok(trade_activity) = activity.into_trade() else {
        return Ok(None);
    };
    if trade_activity.symbol != symbol {
        return Ok(None);
    }

    let broker_order_id = Some(trade_activity.order_id.to_string());
    let qty = num_to_decimal(&trade_activity.quantity)?;
    let price = num_to_decimal(&trade_activity.price)?;

    let mut exec = Execution::new(
        "alpaca".to_string(),
        ExecutionSource::AccountActivities,
        account_id,
        trade_activity.id,
        broker_order_id,
        trade_activity.symbol,
        map_side(trade_activity.side),
        qty,
        price,
        trade_activity.transaction_time.naive_utc(),
    );
    exec.raw_json = None;
    Ok(Some(exec))
}

fn execution_activity_request(
    after: Option<DateTime<Utc>>,
    page_token: Option<String>,
) -> ActivityReq {
    ActivityReq {
        types: vec![ActivityType::Fill],
        direction: Direction::Ascending,
        after,
        until: None,
        page_size: Some(EXECUTION_PAGE_SIZE),
        page_token,
        ..Default::default()
    }
}

fn next_page_token(activities: &[Activity]) -> Option<String> {
    activities.last().map(|activity| activity.id().to_string())
}

fn append_executions_from_activities(
    out: &mut Vec<Execution>,
    activities: Vec<Activity>,
    symbol: &str,
    account_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    for activity in activities {
        if let Some(exec) = execution_from_activity(activity, symbol, account_id)? {
            out.push(exec);
        }
    }
    Ok(())
}

fn should_stop_after_page(page_len: usize) -> bool {
    page_len < EXECUTION_PAGE_SIZE
}

pub fn fetch_executions(
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<Execution>, Box<dyn Error>> {
    crate::ensure_trade_account(trade, account)?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let symbol = trade.trading_vehicle.symbol.clone();

    let rt = Runtime::new().map_err(|e| Box::new(e) as Box<dyn Error>)?;
    rt.block_on(async move {
        let mut page_token: Option<String> = None;
        let mut out: Vec<Execution> = vec![];

        // Safety cap to avoid infinite paging loops.
        for _ in 0..MAX_EXECUTION_PAGES {
            let req = execution_activity_request(after, page_token.clone());
            let activities: Vec<Activity> = client
                .issue::<Get>(&req)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error>)?;

            let page_len = activities.len();
            if page_len == 0 {
                break;
            }

            // The API uses the last activity's `id` as a `page_token`.
            page_token = next_page_token(&activities);

            append_executions_from_activities(&mut out, activities, &symbol, account.id)?;

            // If we received less than a full page, we're done.
            // Note: this checks the raw page size, not the filtered execution count.
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
        append_executions_from_activities, execution_activity_request, execution_from_activity,
        map_side, next_page_token, num_to_decimal, should_stop_after_page, EXECUTION_PAGE_SIZE,
    };
    use apca::api::v2::account_activities::Activity;
    use chrono::{DateTime, Utc};
    use model::{ExecutionSide, ExecutionSource};
    use std::str::FromStr;
    use uuid::Uuid;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    fn fill_activity(id: &str, symbol: &str) -> Activity {
        let raw = format!(
            r#"{{
                "activity_type": "FILL",
                "cum_qty": "1",
                "id": "{id}",
                "leaves_qty": "0",
                "price": "184.125",
                "qty": "1",
                "side": "buy",
                "symbol": "{symbol}",
                "transaction_time": "2026-05-07T13:00:06Z",
                "order_id": "904837e3-3b76-47ec-b432-046db621571b",
                "type": "fill"
            }}"#
        );
        serde_json::from_str(&raw).expect("valid fill")
    }

    #[test]
    fn map_side_converts_known_variants() {
        assert_eq!(
            map_side(apca::api::v2::account_activities::Side::Buy),
            ExecutionSide::Buy
        );
        assert_eq!(
            map_side(apca::api::v2::account_activities::Side::Sell),
            ExecutionSide::Sell
        );
        assert_eq!(
            map_side(apca::api::v2::account_activities::Side::ShortSell),
            ExecutionSide::SellShort
        );
    }

    #[test]
    fn num_to_decimal_parses_valid_num() {
        let n = num_decimal::Num::from_str("0.125").expect("num parse");
        let d = num_to_decimal(&n).expect("decimal parse");
        assert_eq!(d.to_string(), "0.125");
    }

    #[test]
    fn execution_from_activity_maps_matching_fill() {
        let account_id = Uuid::new_v4();
        let raw = r#"{
            "activity_type": "FILL",
            "cum_qty": "2.5",
            "id": "fill-1",
            "leaves_qty": "0",
            "price": "184.125",
            "qty": "1.25",
            "side": "sell_short",
            "symbol": "AAPL",
            "transaction_time": "2026-05-07T13:00:06Z",
            "order_id": "904837e3-3b76-47ec-b432-046db621571b",
            "type": "fill"
        }"#;
        let activity: Activity = serde_json::from_str(raw).expect("valid fill");

        let execution = execution_from_activity(activity, "AAPL", account_id)
            .expect("conversion succeeds")
            .expect("matching fill");

        assert_eq!(execution.broker, "alpaca");
        assert_eq!(execution.source, ExecutionSource::AccountActivities);
        assert_eq!(execution.account_id, account_id);
        assert_eq!(execution.broker_execution_id, "fill-1");
        assert_eq!(
            execution.broker_order_id,
            Some("904837e3-3b76-47ec-b432-046db621571b".to_string())
        );
        assert_eq!(execution.symbol, "AAPL");
        assert_eq!(execution.side, ExecutionSide::SellShort);
        assert_eq!(execution.qty.to_string(), "1.25");
        assert_eq!(execution.price.to_string(), "184.125");
        assert_eq!(execution.raw_json, None);
    }

    #[test]
    fn execution_from_activity_ignores_non_matching_or_non_trade_activity() {
        let account_id = Uuid::new_v4();
        let fill_raw = r#"{
            "activity_type": "FILL",
            "cum_qty": "1",
            "id": "fill-2",
            "leaves_qty": "0",
            "price": "184.125",
            "qty": "1",
            "side": "buy",
            "symbol": "MSFT",
            "transaction_time": "2026-05-07T13:00:06Z",
            "order_id": "904837e3-3b76-47ec-b432-046db621571b",
            "type": "fill"
        }"#;
        let fee_raw = r#"{
            "activity_type": "FEE",
            "id": "fee-1",
            "date": "2026-05-07",
            "net_amount": "-0.01",
            "symbol": "AAPL"
        }"#;
        let mismatched: Activity = serde_json::from_str(fill_raw).expect("valid fill");
        let non_trade: Activity = serde_json::from_str(fee_raw).expect("valid fee");

        assert!(execution_from_activity(mismatched, "AAPL", account_id)
            .expect("conversion succeeds")
            .is_none());
        assert!(execution_from_activity(non_trade, "AAPL", account_id)
            .expect("conversion succeeds")
            .is_none());
    }

    #[test]
    fn execution_activity_request_uses_fill_paging_contract() {
        let after = utc("2026-05-07T12:00:00Z");
        let req = execution_activity_request(Some(after), Some("page-1".to_string()));

        assert_eq!(
            req.types,
            vec![apca::api::v2::account_activities::ActivityType::Fill]
        );
        assert_eq!(
            req.direction,
            apca::api::v2::account_activities::Direction::Ascending
        );
        assert_eq!(req.after, Some(after));
        assert_eq!(req.until, None);
        assert_eq!(req.page_size, Some(EXECUTION_PAGE_SIZE));
        assert_eq!(req.page_token.as_deref(), Some("page-1"));
    }

    #[test]
    fn next_page_token_uses_last_raw_activity_id() {
        let activities = vec![
            fill_activity("fill-a", "AAPL"),
            fill_activity("fill-b", "MSFT"),
        ];

        assert_eq!(next_page_token(&activities).as_deref(), Some("fill-b"));
        assert_eq!(next_page_token(&[]), None);
    }

    #[test]
    fn append_executions_filters_symbol_but_keeps_page_control_independent() {
        let account_id = Uuid::new_v4();
        let mut out = Vec::new();
        let activities = vec![
            fill_activity("fill-a", "AAPL"),
            fill_activity("fill-b", "MSFT"),
        ];

        append_executions_from_activities(&mut out, activities, "AAPL", account_id)
            .expect("activities should filter");

        assert_eq!(out.len(), 1);
        let execution = out.first().expect("one matching execution");
        assert_eq!(execution.broker_execution_id, "fill-a");
        assert_eq!(execution.symbol, "AAPL");
        assert!(should_stop_after_page(EXECUTION_PAGE_SIZE - 1));
        assert!(!should_stop_after_page(EXECUTION_PAGE_SIZE));
    }
}
