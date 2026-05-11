use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::cell::Cell;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn set_enabled(enabled: bool) {
    ENABLED.with(|flag| flag.set(enabled));
}

pub(crate) fn is_enabled() -> bool {
    ENABLED.with(Cell::get)
}

pub(crate) fn decimal_string(value: Decimal) -> String {
    value.round_dp(8).normalize().to_string()
}

pub(crate) fn amount(value: Decimal) -> String {
    if is_enabled() {
        "hidden".to_string()
    } else {
        decimal_string(value)
    }
}

pub(crate) fn amount_share(value: Decimal, basis: Decimal) -> String {
    if is_enabled() {
        percentage_of(value, basis)
    } else {
        decimal_string(value)
    }
}

pub(crate) fn price_relative_to(value: Decimal, basis: Decimal) -> String {
    if is_enabled() {
        percentage_of(value, basis)
    } else {
        decimal_string(value)
    }
}

pub(crate) fn currency(value: Decimal) -> String {
    if is_enabled() {
        "hidden".to_string()
    } else if value >= dec!(0) {
        format!("${value:.2}")
    } else {
        format!("-${:.2}", value.abs())
    }
}

pub(crate) fn currency_share(value: Decimal, basis: Decimal) -> String {
    if is_enabled() {
        percentage_of(value, basis)
    } else {
        currency(value)
    }
}

pub(crate) fn signed_currency(value: Decimal) -> String {
    if is_enabled() {
        "hidden".to_string()
    } else if value >= dec!(0) {
        format!("+${value:.2}")
    } else {
        format!("-${:.2}", value.abs())
    }
}

pub(crate) fn percentage_of(value: Decimal, basis: Decimal) -> String {
    if basis == Decimal::ZERO {
        "0.0%".to_string()
    } else {
        let percentage = value
            .checked_mul(dec!(100))
            .and_then(|v| v.checked_div(basis))
            .unwrap_or(Decimal::ZERO);
        format!("{percentage:.1}%")
    }
}

pub(crate) fn signed_percentage_of(value: Decimal, basis: Decimal) -> String {
    if basis == Decimal::ZERO {
        "0.0%".to_string()
    } else {
        let percentage = value
            .checked_mul(dec!(100))
            .and_then(|v| v.checked_div(basis))
            .unwrap_or(Decimal::ZERO);
        if percentage > Decimal::ZERO {
            format!("+{percentage:.1}%")
        } else {
            format!("{percentage:.1}%")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{amount, amount_share, currency, price_relative_to, set_enabled, signed_currency};
    use rust_decimal_macros::dec;

    #[test]
    fn formatters_keep_amounts_visible_when_disabled() {
        set_enabled(false);

        assert_eq!(amount(dec!(12.3400)), "12.34");
        assert_eq!(amount_share(dec!(25), dec!(100)), "25");
        assert_eq!(price_relative_to(dec!(110), dec!(100)), "110");
        assert_eq!(currency(dec!(12.3)), "$12.30");
        assert_eq!(signed_currency(dec!(-12.3)), "-$12.30");
    }

    #[test]
    fn formatters_hide_amounts_or_show_percentages_when_enabled() {
        set_enabled(true);

        assert_eq!(amount(dec!(12.34)), "hidden");
        assert_eq!(amount_share(dec!(25), dec!(100)), "25.0%");
        assert_eq!(price_relative_to(dec!(110), dec!(100)), "110.0%");
        assert_eq!(currency(dec!(12.3)), "hidden");
        assert_eq!(amount_share(dec!(1), dec!(0)), "0.0%");

        set_enabled(false);
    }
}
