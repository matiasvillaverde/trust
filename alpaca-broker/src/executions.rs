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

pub fn fetch_executions(
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<Execution>, Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let symbol = trade.trading_vehicle.symbol.clone();

    let rt = Runtime::new().map_err(|e| Box::new(e) as Box<dyn Error>)?;
    rt.block_on(async move {
        let mut page_token: Option<String> = None;
        let mut out: Vec<Execution> = vec![];

        // Safety cap to avoid infinite paging loops.
        for _ in 0..32usize {
            let req = ActivityReq {
                types: vec![ActivityType::Fill],
                direction: Direction::Ascending,
                after,
                until: None,
                page_size: Some(100),
                page_token: page_token.clone(),
                ..Default::default()
            };

            let activities: Vec<Activity> = client
                .issue::<Get>(&req)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error>)?;

            let page_len = activities.len();
            if page_len == 0 {
                break;
            }

            // The API uses the last activity's `id` as a `page_token`.
            page_token = activities.last().map(|a| a.id().to_string());

            for activity in activities {
                let Ok(trade_activity) = activity.into_trade() else {
                    continue;
                };
                if trade_activity.symbol != symbol {
                    continue;
                }

                let broker_order_id = Uuid::parse_str(&trade_activity.order_id.to_string()).ok();
                let qty = num_to_decimal(&trade_activity.quantity)?;
                let price = num_to_decimal(&trade_activity.price)?;

                let mut exec = Execution::new(
                    "alpaca".to_string(),
                    ExecutionSource::AccountActivities,
                    account.id,
                    trade_activity.id,
                    broker_order_id,
                    trade_activity.symbol,
                    map_side(trade_activity.side),
                    qty,
                    price,
                    trade_activity.transaction_time.naive_utc(),
                );
                exec.raw_json = None;
                out.push(exec);
            }

            // If we received less than a full page, we're done.
            // Note: this checks the raw page size, not the filtered execution count.
            if page_len < 100 {
                break;
            }
        }

        Ok(out)
    })
}
