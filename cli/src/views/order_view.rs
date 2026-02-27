use model::Order;
use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;

#[derive(Tabled)]
pub struct OrderView {
    pub unit_price: String,
    pub average_filled_price: String,
    pub quantity: String,
    pub category: String,
    pub action: String,
    pub time_in_force: String,
    pub extended_hours: String,
    pub submitted_at: String,
}

impl OrderView {
    fn new(order: Order) -> OrderView {
        OrderView {
            unit_price: order.unit_price.to_string(),
            average_filled_price: order
                .average_filled_price
                .map(|d| d.to_string())
                .unwrap_or_default(),
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

    pub fn display(o: Order) {
        println!();
        println!("Order: {}", o.id);
        OrderView::display_orders(vec![o]);
        println!();
    }

    pub fn display_orders(orders: Vec<Order>) {
        let views: Vec<OrderView> = orders.into_iter().map(OrderView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{table}");
    }
}

#[cfg(test)]
mod tests {
    use super::OrderView;
    use model::{Order, OrderAction, OrderCategory, TimeInForce};
    use rust_decimal_macros::dec;

    #[test]
    fn new_maps_optional_and_scalar_fields() {
        let order = Order {
            unit_price: dec!(101.25),
            average_filled_price: Some(dec!(100.75)),
            quantity: 10,
            category: OrderCategory::Limit,
            action: OrderAction::Buy,
            time_in_force: TimeInForce::UntilCanceled,
            extended_hours: true,
            submitted_at: None,
            ..Default::default()
        };

        let view = OrderView::new(order);
        assert_eq!(view.unit_price, "101.25");
        assert_eq!(view.average_filled_price, "100.75");
        assert_eq!(view.quantity, "10");
        assert_eq!(view.category, "limit");
        assert_eq!(view.action, "buy");
        assert_eq!(view.time_in_force, "until_canceled");
        assert_eq!(view.extended_hours, "true");
        assert_eq!(view.submitted_at, "");
    }

    #[test]
    fn display_orders_runs_for_smoke_coverage() {
        OrderView::display_orders(vec![Order::default()]);
    }
}
