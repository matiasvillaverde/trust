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
    fn display_handles_empty_and_populated_execution_lists() {
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
            broker_order_id: Some(Uuid::new_v4()),
            symbol: "AAPL".to_string(),
            side: ExecutionSide::Buy,
            qty: dec!(1.5),
            price: dec!(200.25),
            executed_at: Utc::now().naive_utc(),
            raw_json: Some("{\"status\":\"ok\"}".to_string()),
        };

        ExecutionView::display(&[exec]);
    }
}
