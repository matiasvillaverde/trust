use ibkr_broker::IbkrBroker;
use model::{
    Account, BrokerKind, Currency, Environment, TimeInForce, Trade, TradeCategory,
    TradingVehicleCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::str::FromStr;

const ENABLE_ENV: &str = "TRUST_IBKR_LIVE_E2E";
const ACCOUNT_ID_ENV: &str = "TRUST_IBKR_ACCOUNT_ID";
const STOCK_SYMBOL_ENV: &str = "TRUST_IBKR_STOCK_SYMBOL";
const ETF_SYMBOL_ENV: &str = "TRUST_IBKR_ETF_SYMBOL";
const BOND_SYMBOL_ENV: &str = "TRUST_IBKR_BOND_SYMBOL";
const STOCK_ENTRY_PRICE_ENV: &str = "TRUST_IBKR_STOCK_ENTRY_PRICE";
const STOCK_STOP_PRICE_ENV: &str = "TRUST_IBKR_STOCK_STOP_PRICE";
const STOCK_TARGET_PRICE_ENV: &str = "TRUST_IBKR_STOCK_TARGET_PRICE";
const ETF_ENTRY_PRICE_ENV: &str = "TRUST_IBKR_ETF_ENTRY_PRICE";
const ETF_STOP_PRICE_ENV: &str = "TRUST_IBKR_ETF_STOP_PRICE";
const ETF_TARGET_PRICE_ENV: &str = "TRUST_IBKR_ETF_TARGET_PRICE";
const BOND_ENTRY_PRICE_ENV: &str = "TRUST_IBKR_BOND_ENTRY_PRICE";
const BOND_STOP_PRICE_ENV: &str = "TRUST_IBKR_BOND_STOP_PRICE";
const BOND_TARGET_PRICE_ENV: &str = "TRUST_IBKR_BOND_TARGET_PRICE";

fn enabled() -> bool {
    matches!(
        std::env::var(ENABLE_ENV)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_env_value(name: &str) -> String {
    env_value(name).unwrap_or_else(|| {
        panic!("live IBKR e2e is enabled but required environment variable {name} is missing")
    })
}

fn required_decimal_env_value(name: &str) -> Decimal {
    Decimal::from_str(&required_env_value(name))
        .unwrap_or_else(|error| panic!("environment variable {name} must be a decimal: {error}"))
}

fn validate_long_bracket_prices(
    symbol: &str,
    category: TradingVehicleCategory,
    entry_price: Decimal,
    stop_price: Decimal,
    target_price: Decimal,
) {
    assert!(
        entry_price > Decimal::ZERO,
        "{symbol} ({category}) entry price must be positive"
    );
    assert!(
        stop_price > Decimal::ZERO,
        "{symbol} ({category}) stop price must be positive"
    );
    assert!(
        target_price > Decimal::ZERO,
        "{symbol} ({category}) target price must be positive"
    );
    assert!(
        stop_price < entry_price,
        "{symbol} ({category}) long stop price must be below entry price"
    );
    assert!(
        entry_price < target_price,
        "{symbol} ({category}) long target price must be above entry price"
    );
}

fn live_account(account_id: String) -> Account {
    Account {
        name: "ibkr-live-e2e".to_string(),
        environment: Environment::Paper,
        broker_kind: BrokerKind::Ibkr,
        broker_account_id: Some(account_id),
        ..Default::default()
    }
}

fn preview_trade(
    account: &Account,
    symbol: &str,
    category: TradingVehicleCategory,
    entry_price: Decimal,
    stop_price: Decimal,
    target_price: Decimal,
) -> Trade {
    let mut trade = Trade {
        account_id: account.id,
        category: TradeCategory::Long,
        ..Default::default()
    };
    trade.trading_vehicle.symbol = symbol.to_string();
    trade.trading_vehicle.category = category;
    trade.trading_vehicle.exchange = Some("SMART".to_string());
    trade.entry.quantity = 1;
    trade.target.quantity = 1;
    trade.safety_stop.quantity = 1;
    trade.entry.currency = Currency::USD;
    trade.target.currency = Currency::USD;
    trade.safety_stop.currency = Currency::USD;
    trade.entry.time_in_force = TimeInForce::Day;
    trade.target.time_in_force = TimeInForce::Day;
    trade.safety_stop.time_in_force = TimeInForce::Day;
    trade.entry.unit_price = entry_price;
    trade.safety_stop.unit_price = stop_price;
    trade.target.unit_price = target_price;
    trade
}

#[test]
fn live_long_bracket_input_validation_accepts_positive_long_geometry() {
    validate_long_bracket_prices(
        "AAPL",
        TradingVehicleCategory::Stock,
        dec!(200),
        dec!(190),
        dec!(220),
    );
}

#[test]
fn live_long_bracket_input_validation_rejects_bad_geometry() {
    let invalid_cases = [
        (dec!(0), dec!(190), dec!(220)),
        (dec!(200), dec!(0), dec!(220)),
        (dec!(200), dec!(190), dec!(0)),
        (dec!(200), dec!(200), dec!(220)),
        (dec!(200), dec!(210), dec!(220)),
        (dec!(200), dec!(190), dec!(200)),
        (dec!(200), dec!(190), dec!(180)),
    ];

    for (entry_price, stop_price, target_price) in invalid_cases {
        assert!(
            std::panic::catch_unwind(|| {
                validate_long_bracket_prices(
                    "AAPL",
                    TradingVehicleCategory::Stock,
                    entry_price,
                    stop_price,
                    target_price,
                );
            })
            .is_err(),
            "expected invalid live long bracket inputs to be rejected: entry={entry_price}, stop={stop_price}, target={target_price}"
        );
    }
}

#[test]
fn live_e2e_inputs_are_present_and_valid_when_enabled() {
    if !enabled() {
        eprintln!("skipping live IBKR input validation: set {ENABLE_ENV}=1 to enable");
        return;
    }

    let cases = [
        (
            required_env_value(STOCK_SYMBOL_ENV),
            TradingVehicleCategory::Stock,
            required_decimal_env_value(STOCK_ENTRY_PRICE_ENV),
            required_decimal_env_value(STOCK_STOP_PRICE_ENV),
            required_decimal_env_value(STOCK_TARGET_PRICE_ENV),
        ),
        (
            required_env_value(ETF_SYMBOL_ENV),
            TradingVehicleCategory::Etf,
            required_decimal_env_value(ETF_ENTRY_PRICE_ENV),
            required_decimal_env_value(ETF_STOP_PRICE_ENV),
            required_decimal_env_value(ETF_TARGET_PRICE_ENV),
        ),
        (
            required_env_value(BOND_SYMBOL_ENV),
            TradingVehicleCategory::Bond,
            required_decimal_env_value(BOND_ENTRY_PRICE_ENV),
            required_decimal_env_value(BOND_STOP_PRICE_ENV),
            required_decimal_env_value(BOND_TARGET_PRICE_ENV),
        ),
    ];

    required_env_value(ACCOUNT_ID_ENV);

    for (symbol, category, entry_price, stop_price, target_price) in cases {
        validate_long_bracket_prices(&symbol, category, entry_price, stop_price, target_price);
    }
}

#[test]
#[ignore = "requires an authenticated IBKR Client Portal Gateway"]
fn live_gateway_preflight_authenticates_and_selects_account() {
    if !enabled() {
        eprintln!("skipping live IBKR preflight: set {ENABLE_ENV}=1 to enable");
        return;
    }

    let account = live_account(required_env_value(ACCOUNT_ID_ENV));
    IbkrBroker::preflight_gateway(&account)
        .unwrap_or_else(|error| panic!("live IBKR preflight failed: {error}"));
}

#[test]
#[ignore = "requires an authenticated IBKR Client Portal Gateway"]
fn live_gateway_resolves_stock_etf_and_bond_contracts() {
    if !enabled() {
        eprintln!("skipping live IBKR e2e: set {ENABLE_ENV}=1 to enable");
        return;
    }

    let account_id = required_env_value(ACCOUNT_ID_ENV);
    let stock_symbol = required_env_value(STOCK_SYMBOL_ENV);
    let etf_symbol = required_env_value(ETF_SYMBOL_ENV);
    let bond_symbol = required_env_value(BOND_SYMBOL_ENV);

    let account = live_account(account_id);
    let cases = [
        (stock_symbol.as_str(), TradingVehicleCategory::Stock, "STK"),
        (etf_symbol.as_str(), TradingVehicleCategory::Etf, "STK"),
        (bond_symbol.as_str(), TradingVehicleCategory::Bond, "BOND"),
    ];

    for (symbol, category, expected_sec_type) in cases {
        let metadata = IbkrBroker::fetch_contract_metadata_for_category(&account, symbol, category)
            .unwrap_or_else(|error| {
                panic!("failed to resolve live IBKR contract for {symbol} ({category}): {error}")
            });

        assert_eq!(metadata.category, category);
        assert_eq!(metadata.sec_type, expected_sec_type);
        assert!(
            !metadata.conid.trim().is_empty(),
            "IBKR conid must be present for {symbol}"
        );
    }
}

#[test]
#[ignore = "requires an authenticated IBKR Client Portal Gateway"]
fn live_gateway_previews_stock_etf_and_bond_brackets_with_whatif() {
    if !enabled() {
        eprintln!("skipping live IBKR what-if preview: set {ENABLE_ENV}=1 to enable");
        return;
    }

    let account = live_account(required_env_value(ACCOUNT_ID_ENV));
    let stock_symbol = required_env_value(STOCK_SYMBOL_ENV);
    let etf_symbol = required_env_value(ETF_SYMBOL_ENV);
    let bond_symbol = required_env_value(BOND_SYMBOL_ENV);

    let cases = [
        (
            stock_symbol.as_str(),
            TradingVehicleCategory::Stock,
            required_decimal_env_value(STOCK_ENTRY_PRICE_ENV),
            required_decimal_env_value(STOCK_STOP_PRICE_ENV),
            required_decimal_env_value(STOCK_TARGET_PRICE_ENV),
        ),
        (
            etf_symbol.as_str(),
            TradingVehicleCategory::Etf,
            required_decimal_env_value(ETF_ENTRY_PRICE_ENV),
            required_decimal_env_value(ETF_STOP_PRICE_ENV),
            required_decimal_env_value(ETF_TARGET_PRICE_ENV),
        ),
        (
            bond_symbol.as_str(),
            TradingVehicleCategory::Bond,
            required_decimal_env_value(BOND_ENTRY_PRICE_ENV),
            required_decimal_env_value(BOND_STOP_PRICE_ENV),
            required_decimal_env_value(BOND_TARGET_PRICE_ENV),
        ),
    ];

    for (symbol, category, entry_price, stop_price, target_price) in cases {
        validate_long_bracket_prices(symbol, category, entry_price, stop_price, target_price);
        let trade = preview_trade(
            &account,
            symbol,
            category,
            entry_price,
            stop_price,
            target_price,
        );
        let preview = IbkrBroker::preview_trade(&account, &trade).unwrap_or_else(|error| {
            panic!("failed live IBKR what-if preview for {symbol} ({category}): {error}")
        });
        assert!(
            preview.is_object(),
            "IBKR what-if preview should return an object for {symbol}"
        );
    }
}
