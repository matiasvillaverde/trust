use model::{Order, TradingVehicleCategory};
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
        let precision =
            crate::display_precision::DisplayPrecision::for_category(TradingVehicleCategory::Stock);
        OrderView {
            unit_price: if crate::zen::is_enabled() {
                crate::zen::amount(order.unit_price)
            } else {
                precision.format_price(order.unit_price)
            },
            average_filled_price: order
                .average_filled_price
                .map(|price| {
                    if crate::zen::is_enabled() {
                        crate::zen::amount(price)
                    } else {
                        precision.format_price(price)
                    }
                })
                .unwrap_or_default(),
            quantity: precision.format_quantity(order.quantity),
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
        crate::zen::set_enabled(false);
        let order = Order {
            unit_price: dec!(101.25),
            average_filled_price: Some(dec!(100.75)),
            quantity: 10.into(),
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
    fn new_hides_prices_in_zen_mode() {
        crate::zen::set_enabled(true);
        let order = Order {
            unit_price: dec!(101.25),
            average_filled_price: Some(dec!(100.75)),
            ..Default::default()
        };

        let view = OrderView::new(order);
        assert_eq!(view.unit_price, "hidden");
        assert_eq!(view.average_filled_price, "hidden");
        crate::zen::set_enabled(false);
    }

    #[test]
    fn display_orders_runs_for_smoke_coverage() {
        crate::zen::set_enabled(false);
        OrderView::display_orders(vec![Order::default()]);
    }
}
