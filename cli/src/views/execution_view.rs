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

