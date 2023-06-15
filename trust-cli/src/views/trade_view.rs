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
        }
    }

    pub fn display(a: &Trade, account_name: &str) {
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
        println!("{}", table);
    }
}

#[derive(Tabled)]
pub struct TradeOverviewView {
    pub funding: String,
    pub capital_in_market: String,
    pub capital_out_market: String,
    pub taxed: String,
    pub total_performance: String,
    pub currency: String,
}

impl TradeOverviewView {
    fn new(overview: &TradeOverview) -> TradeOverviewView {
        TradeOverviewView {
            funding: overview.funding.to_string(),
            capital_in_market: overview.capital_in_market.to_string(),
            capital_out_market: overview.capital_out_market.to_string(),
            taxed: overview.taxed.to_string(),
            total_performance: overview.total_performance.to_string(),
            currency: overview.currency.to_string(),
        }
    }

    pub fn display(overview: &TradeOverview) {
        TradeOverviewView::display_overviews(vec![overview]);
    }

    pub fn display_overviews(overviews: Vec<&TradeOverview>) {
        let views: Vec<TradeOverviewView> =
            overviews.into_iter().map(TradeOverviewView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}
