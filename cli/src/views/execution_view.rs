use model::Execution;

pub struct ExecutionView;

impl ExecutionView {
    pub fn display(executions: &[Execution]) {
        if executions.is_empty() {
            println!("No executions recorded yet.");
            return;
        }

        for exec in executions {
            let trade = exec
                .trade_id
                .map(|t| t.to_string())
                .unwrap_or_else(|| "-".to_string());
            let order = exec
                .order_id
                .map(|o| o.to_string())
                .unwrap_or_else(|| "-".to_string());

            println!(
                "{} {} {} @ {} ({}) trade={} order={} src={}",
                exec.executed_at,
                exec.symbol,
                exec.qty,
                exec.price,
                exec.side,
                trade,
                order,
                exec.source
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionView;
    use chrono::Utc;
    use model::{Execution, ExecutionSide, ExecutionSource};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[test]
    fn display_reports_fractional_fills_and_empty_history() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "display_reports_fractional_fills_and_empty_history",
            || {
                ExecutionView::display(&[]);

                let exec = Execution {
                    id: Uuid::new_v4(),
                    created_at: Utc::now().naive_utc(),
                    updated_at: Utc::now().naive_utc(),
                    deleted_at: None,
                    broker: "alpaca".to_string(),
                    source: ExecutionSource::TradeUpdates,
                    account_id: Uuid::new_v4(),
                    trade_id: Some(Uuid::new_v4()),
                    order_id: Some(Uuid::new_v4()),
                    broker_execution_id: "exec-1".to_string(),
                    broker_order_id: Some(Uuid::new_v4().to_string()),
                    symbol: "AAPL".to_string(),
                    side: ExecutionSide::Buy,
                    qty: dec!(1.5),
                    price: dec!(200.25),
                    executed_at: Utc::now().naive_utc(),
                    raw_json: Some("{\"status\":\"ok\"}".to_string()),
                };

                ExecutionView::display(&[exec]);
            },
            |output| {
                assert!(output.contains("No executions recorded yet."), "{output}");
                assert!(output.contains("AAPL 1.5 @ 200.25 (buy)"), "{output}");
                assert!(output.contains("src=trade_updates"), "{output}");
                assert!(
                    !output.contains("status"),
                    "raw broker JSON must not be rendered: {output}"
                );
            },
        );
    }

    #[test]
    fn unlinked_broker_fills_remain_visible_without_invented_links() {
        crate::test_support::capture::assert_stdout(
            module_path!(),
            "unlinked_broker_fills_remain_visible_without_invented_links",
            || {
                let exec = Execution {
                    id: Uuid::new_v4(),
                    created_at: Utc::now().naive_utc(),
                    updated_at: Utc::now().naive_utc(),
                    deleted_at: None,
                    broker: "ibkr".to_string(),
                    source: ExecutionSource::AccountActivities,
                    account_id: Uuid::new_v4(),
                    trade_id: None,
                    order_id: None,
                    broker_execution_id: "exec-2".to_string(),
                    broker_order_id: None,
                    symbol: "MSFT".to_string(),
                    side: ExecutionSide::Sell,
                    qty: dec!(2),
                    price: dec!(199.75),
                    executed_at: Utc::now().naive_utc(),
                    raw_json: None,
                };

                ExecutionView::display(&[exec]);
            },
            |output| {
                assert!(output.contains("MSFT 2 @ 199.75 (sell)"), "{output}");
                assert!(
                    output.contains("trade=- order=- src=account_activities"),
                    "{output}"
                );
            },
        );
    }
}
