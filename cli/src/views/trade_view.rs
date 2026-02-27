use model::{Trade, TradeBalance};
use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;

#[derive(Tabled)]
pub struct TradeView {
    pub trading_vehicle: String,
    pub category: String,
    pub account: String,
    pub currency: String,
    pub quantity: String,
    pub stop_price: String,
    pub entry_price: String,
    pub target_price: String,
    pub status: String,
}

impl TradeView {
    fn new(trade: Trade, account_name: &str) -> TradeView {
        TradeView {
            trading_vehicle: trade.trading_vehicle.clone().symbol,
            category: trade.category.to_string(),
            account: crate::views::uppercase_first(account_name),
            currency: trade.currency.to_string(),
            quantity: trade.entry.quantity.to_string(),
            stop_price: trade.safety_stop.unit_price.to_string(),
            entry_price: trade.entry.unit_price.to_string(),
            target_price: trade.target.unit_price.to_string(),
            status: trade.status.to_string(),
        }
    }

    pub fn display(a: &Trade, account_name: &str) {
        println!();
        println!("Trade: {}", a.id);
        TradeView::display_trades(vec![a.clone()], account_name);
        println!();
    }

    pub fn display_trades(trades: Vec<Trade>, account_name: &str) {
        let views: Vec<TradeView> = trades
            .into_iter()
            .map(|x| TradeView::new(x, account_name))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{table}");
    }
}

#[derive(Tabled)]
pub struct TradeBalanceView {
    pub funding: String,
    pub capital_in_market: String,
    pub capital_out_market: String,
    pub taxed: String,
    pub total_performance: String,
    pub currency: String,
}

impl TradeBalanceView {
    fn new(balance: &TradeBalance) -> TradeBalanceView {
        TradeBalanceView {
            funding: balance.funding.to_string(),
            capital_in_market: balance.capital_in_market.to_string(),
            capital_out_market: balance.capital_out_market.to_string(),
            taxed: balance.taxed.to_string(),
            total_performance: balance.total_performance.to_string(),
            currency: balance.currency.to_string(),
        }
    }

    pub fn display(balance: &TradeBalance) {
        TradeBalanceView::display_balances(vec![balance]);
    }

    pub fn display_balances(balances: Vec<&TradeBalance>) {
        let views: Vec<TradeBalanceView> =
            balances.into_iter().map(TradeBalanceView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{table}");
    }
}

#[cfg(test)]
mod tests {
    use super::{TradeBalanceView, TradeView};
    use model::{Status, Trade, TradeBalance, TradeCategory};
    use rust_decimal_macros::dec;

    #[test]
    fn trade_view_new_maps_trade_snapshot_fields() {
        let mut trade = Trade::default();
        trade.trading_vehicle.symbol = "tsla".to_string();
        trade.category = TradeCategory::Short;
        trade.entry.quantity = 20;
        trade.safety_stop.unit_price = dec!(250);
        trade.entry.unit_price = dec!(200);
        trade.target.unit_price = dec!(150);
        trade.status = Status::Submitted;

        let view = TradeView::new(trade, "paper");
        assert_eq!(view.trading_vehicle, "tsla");
        assert_eq!(view.category, "short");
        assert_eq!(view.account, "Paper");
        assert_eq!(view.quantity, "20");
        assert_eq!(view.stop_price, "250");
        assert_eq!(view.entry_price, "200");
        assert_eq!(view.target_price, "150");
        assert_eq!(view.status, "submitted");
    }

    #[test]
    fn trade_balance_view_new_maps_balance_values() {
        let balance = TradeBalance {
            funding: dec!(1000),
            capital_in_market: dec!(500),
            capital_out_market: dec!(500),
            taxed: dec!(50),
            total_performance: dec!(120),
            ..Default::default()
        };

        let view = TradeBalanceView::new(&balance);
        assert_eq!(view.funding, "1000");
        assert_eq!(view.capital_in_market, "500");
        assert_eq!(view.capital_out_market, "500");
        assert_eq!(view.taxed, "50");
        assert_eq!(view.total_performance, "120");
    }

    #[test]
    fn display_helpers_run_for_smoke_coverage() {
        let trade = Trade::default();
        TradeView::display_trades(vec![trade.clone()], "main");
        TradeView::display(&trade, "main");
        TradeBalanceView::display(&TradeBalance::default());
    }
}
