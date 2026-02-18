use crate::keys;
use apca::api::v2::account_activities::{Activity, ActivityReq, ActivityType, Direction, Get};
use apca::Client;
use chrono::{DateTime, Utc};
use model::{Account, FeeActivity, Trade};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tokio::runtime::Runtime;

fn num_to_decimal(n: &num_decimal::Num) -> Result<Decimal, Box<dyn Error>> {
    Decimal::from_str(&n.to_string())
        .map_err(|e| format!("failed to parse num as decimal: {e}").into())
}

pub fn fetch_fee_activities(
    trade: &Trade,
    account: &Account,
    after: Option<DateTime<Utc>>,
) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify that the trade is for the account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let symbol = trade.trading_vehicle.symbol.clone();
    let rt = Runtime::new().map_err(|e| Box::new(e) as Box<dyn Error>)?;
    rt.block_on(async move {
        let mut page_token: Option<String> = None;
        let mut out: Vec<FeeActivity> = vec![];

        for _ in 0..16usize {
            let req = ActivityReq {
                types: vec![ActivityType::Fee, ActivityType::PassThruCharge],
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
            page_token = activities.last().map(|a| a.id().to_string());

            for activity in activities {
                let Activity::NonTrade(non_trade) = activity else {
                    continue;
                };

                if let Some(activity_symbol) = &non_trade.symbol {
                    if activity_symbol != &symbol {
                        continue;
                    }
                }

                let amount = num_to_decimal(&non_trade.net_amount)?.abs();
                if amount <= Decimal::ZERO {
                    continue;
                }

                out.push(FeeActivity {
                    broker: "alpaca".to_string(),
                    broker_activity_id: non_trade.id,
                    account_id: account.id,
                    broker_order_id: None,
                    symbol: non_trade.symbol,
                    activity_type: format!("{:?}", non_trade.type_),
                    amount,
                    occurred_at: non_trade.date.naive_utc(),
                    raw_json: None,
                });
            }

            if page_len < 100 {
                break;
            }
        }

        Ok(out)
    })
}
