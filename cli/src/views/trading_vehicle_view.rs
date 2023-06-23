use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use model::TradingVehicle;

#[derive(Tabled)]
pub struct TradingVehicleView {
    pub category: String,
    pub symbol: String,
    pub broker: String,
    pub isin: String,
}

impl TradingVehicleView {
    fn new(tv: TradingVehicle) -> TradingVehicleView {
        TradingVehicleView {
            category: tv.category.to_string(),
            symbol: tv.symbol.to_uppercase(),
            broker: tv.broker.to_uppercase(),
            isin: tv.isin.to_uppercase(),
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
        println!("{}", table);
    }
}
