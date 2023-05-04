pub mod account_view;
pub mod rule_view;
pub mod transaction_view;

pub use rule_view::RuleView;

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
