use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use trust_model::Order;

#[derive(Tabled)]
pub struct OrderView {
    pub unit_price: String,
    pub quantity: String,
    pub category: String,
    pub action: String,
    pub time_in_force: String,
    pub extended_hours: String,
    pub submitted_at: String,
}

impl OrderView {
    fn new(order: &Order) -> OrderView {
        OrderView {
            unit_price: order.unit_price.to_string(),
            quantity: order.quantity.to_string(),
            category: order.category.to_string(),
            action: order.action.to_string(),
            time_in_force: order.time_in_force.to_string(),
            extended_hours: order.extended_hours.to_string(),
            submitted_at: order
                .submitted_at
                .map(|d| d.to_string())
                .unwrap_or_default(),
        }
    }

    pub fn display(o: &Order) {
        OrderView::display_orders(vec![o]);
    }

    pub fn display_orders(orders: Vec<&Order>) {
        let views: Vec<OrderView> = orders.into_iter().map(OrderView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}