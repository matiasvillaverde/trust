use model::TradingVehicle;
use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;

#[derive(Tabled)]
pub struct TradingVehicleView {
    pub category: String,
    pub symbol: String,
    pub broker: String,
    pub isin: String,
}

impl TradingVehicleView {
    fn new(tv: TradingVehicle) -> TradingVehicleView {
        let isin = tv.isin.unwrap_or_else(|| "-".to_string());
        TradingVehicleView {
            category: tv.category.to_string(),
            symbol: tv.symbol.to_uppercase(),
            broker: tv.broker.to_uppercase(),
            isin: isin.to_uppercase(),
        }
    }

    pub fn display(tv: TradingVehicle) {
        println!();
        println!("Trading Vehicle: {}", tv.id);
        TradingVehicleView::display_table(vec![tv]);
        println!();
    }

    pub fn display_table(tvs: Vec<TradingVehicle>) {
        let views: Vec<TradingVehicleView> = tvs.into_iter().map(TradingVehicleView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{table}");
    }
}

#[cfg(test)]
mod tests {
    use super::TradingVehicleView;
    use model::{TradingVehicle, TradingVehicleCategory};

    #[test]
    fn new_uppercases_symbol_broker_and_isin() {
        let tv = TradingVehicle {
            symbol: "aapl".to_string(),
            broker: "nasdaq".to_string(),
            isin: Some("us0378331005".to_string()),
            category: TradingVehicleCategory::Stock,
            ..Default::default()
        };

        let view = TradingVehicleView::new(tv);
        assert_eq!(view.category, "stock");
        assert_eq!(view.symbol, "AAPL");
        assert_eq!(view.broker, "NASDAQ");
        assert_eq!(view.isin, "US0378331005");
    }

    #[test]
    fn new_uses_dash_for_missing_isin() {
        let tv = TradingVehicle {
            isin: None,
            ..Default::default()
        };
        let view = TradingVehicleView::new(tv);
        assert_eq!(view.isin, "-");
    }

    #[test]
    fn display_table_runs_for_smoke_coverage() {
        TradingVehicleView::display_table(vec![TradingVehicle::default()]);
    }
}
