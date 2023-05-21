use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use trust_model::{Trade, TradeOverview};

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
    pub take_profit: String,
}

impl TradeView {
    fn new(trade: &Trade, account_name: &str) -> TradeView {
        TradeView {
            trading_vehicle: trade.trading_vehicle.clone().symbol,
            category: trade.category.to_string(),
            account: crate::views::uppercase_first(account_name),
            currency: trade.currency.to_string(),
            quantity: trade.entry.quantity.to_string(),
            stop_price: trade.safety_stop.unit_price.amount.to_string(),
            entry_price: trade.entry.unit_price.amount.to_string(),
            target_price: trade
                .exit_targets
                .first()
                .unwrap()
                .target_price
                .amount
                .to_string(),
            take_profit: trade
                .exit_targets
                .first()
                .unwrap()
                .order
                .unit_price
                .amount
                .to_string(),
        }
    }

    pub fn display_trade(a: &Trade, account_name: &str) {
        TradeView::display_trades(vec![a], account_name);
    }

    pub fn display_trades(trades: Vec<&Trade>, account_name: &str) {
        let views: Vec<TradeView> = trades
            .into_iter()
            .map(|x| TradeView::new(x, account_name))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}

#[derive(Tabled)]
pub struct TradeOverviewView {
    pub total_input: String,
    pub total_in_market: String,
    pub total_out_market: String,
    pub total_taxable: String,
    pub total_performance: String,
    pub currency: String,
}

impl TradeOverviewView {
    fn new(overview: TradeOverview) -> TradeOverviewView {
        TradeOverviewView {
            total_input: overview.total_input.amount.to_string(),
            total_in_market: overview.total_in_market.amount.to_string(),
            total_out_market: overview.total_out_market.amount.to_string(),
            total_taxable: overview.total_taxable.amount.to_string(),
            total_performance: overview.total_performance.amount.to_string(),
            currency: overview.total_input.currency.to_string(),
        }
    }

    pub fn display(overview: TradeOverview) {
        TradeOverviewView::display_overviews(vec![overview]);
    }

    pub fn display_overviews(overviews: Vec<TradeOverview>) {
        let views: Vec<TradeOverviewView> = overviews
            .into_iter()
            .map(|x| TradeOverviewView::new(x))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}
