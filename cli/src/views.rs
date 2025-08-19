mod account_view;
mod log_view;
mod order_view;
mod performance_view;
mod rule_view;
mod trade_view;
mod trading_vehicle_view;
mod transaction_view;

pub use account_view::{AccountBalanceView, AccountView};
pub use log_view::LogView;
pub use order_view::OrderView;
pub use performance_view::PerformanceView;
pub use rule_view::RuleView;
pub use trade_view::{TradeBalanceView, TradeView};
pub use trading_vehicle_view::TradingVehicleView;
pub use transaction_view::TransactionView;

fn uppercase_first(data: &str) -> String {
    // Uppercase first letter.
    let mut result = String::new();
    let mut first = true;
    for value in data.chars() {
        if first {
            result.push(value.to_ascii_uppercase());
            first = false;
        } else {
            result.push(value);
        }
    }
    result
}
