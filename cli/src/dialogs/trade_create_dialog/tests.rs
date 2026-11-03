#![cfg(test)]
use super::TradeDialogBuilder;
use crate::dialogs::io::DialogIo;
use crate::dialogs::io::{scripted_push_input, scripted_push_select, scripted_reset};
use alpaca_broker::AlpacaBroker;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, Environment, OrderCategory, TradeCategory, TradingVehicleCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use std::io::{Error as IoError, ErrorKind};
use uuid::Uuid;

struct ScriptedIo {
    selects: VecDeque<Result<Option<usize>, IoError>>,
    inputs: VecDeque<Result<String, IoError>>,
}

impl ScriptedIo {
    fn with(
        selects: Vec<Result<Option<usize>, IoError>>,
        inputs: Vec<Result<String, IoError>>,
    ) -> Self {
        Self {
            selects: selects.into(),
            inputs: inputs.into(),
        }
    }
}

impl DialogIo for ScriptedIo {
    fn select_index(
        &mut self,
        _prompt: &str,
        _labels: &[String],
        _default: usize,
    ) -> Result<Option<usize>, IoError> {
        self.selects.pop_front().unwrap_or(Ok(None))
    }

    fn confirm(&mut self, _prompt: &str, _default: bool) -> Result<bool, IoError> {
        Ok(false)
    }

    fn input_text(&mut self, _prompt: &str, _allow_empty: bool) -> Result<String, IoError> {
        self.inputs.pop_front().unwrap_or_else(|| Ok(String::new()))
    }
}

fn test_trust() -> TrustFacade {
    let path = std::env::temp_dir().join(format!("trust-test-{}.db", Uuid::new_v4()));
    let db = SqliteDatabase::new(path.to_str().expect("valid temp db path"));
    TrustFacade::new(Box::new(db), Box::<AlpacaBroker>::default())
}

fn account_with_usd_deposit(
    trust: &mut TrustFacade,
    name: &str,
    amount: Decimal,
) -> model::Account {
    let account = trust
        .create_account(name, "desc", Environment::Paper, dec!(20), dec!(10))
        .expect("account should be created");
    trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            amount,
            &Currency::USD,
        )
        .expect("deposit should succeed");
    account
}

fn base_quantity_builder(account: model::Account, entry_price: Decimal) -> TradeDialogBuilder {
    TradeDialogBuilder {
        account: Some(account),
        entry_price: Some(entry_price),
        stop_price: Some(dec!(95)),
        currency: Some(Currency::USD),
        ..TradeDialogBuilder::new()
    }
}

#[test]
fn display_identifies_the_failed_action_and_its_cause() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "display_identifies_the_failed_action_and_its_cause",
        || {
            TradeDialogBuilder {
                account: None,
                trading_vehicle: None,
                category: Some(TradeCategory::Long),
                entry_price: None,
                stop_price: None,
                safety_order_category: None,
                currency: None,
                quantity: None,
                target_price: None,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
                result: Some(Err("synthetic failure".into())),
            }
            .display();
        },
        |output| {
            assert!(output.contains("Error creating trade:"), "{output}");
            assert!(output.contains("synthetic failure"), "{output}");
            assert!(!output.contains("executed:"), "{output}");
        },
    );
}

#[test]
fn build_rejects_when_required_inputs_are_missing() {
    let mut trust = test_trust();
    let before = trust.search_all_accounts().unwrap();
    let rejected = TradeDialogBuilder::new().build(&mut trust);

    let error = rejected
        .result
        .unwrap()
        .expect_err("incomplete input must be rejected");
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(trust.search_all_accounts().unwrap(), before);
}

#[test]
fn build_and_display_successfully_with_valid_inputs() {
    let mut trust = test_trust();
    let account = trust
        .create_account(
            "trade-create",
            "desc",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account should be created");
    let trading_vehicle = trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("trading vehicle should be created");

    let built = TradeDialogBuilder {
        account: Some(account.clone()),
        trading_vehicle: Some(trading_vehicle),
        category: Some(TradeCategory::Long),
        entry_price: Some(dec!(100)),
        stop_price: Some(dec!(95)),
        safety_order_category: Some(OrderCategory::StopLimit),
        currency: Some(Currency::USD),
        quantity: Some(dec!(1)),
        target_price: Some(dec!(110)),
        thesis: Some("breakout".to_string()),
        sector: Some("technology".to_string()),
        asset_class: Some("stocks".to_string()),
        context: Some("range expansion".to_string()),
        result: None,
    }
    .build(&mut trust);

    let trade = built
        .result
        .as_ref()
        .expect("result should be set")
        .as_ref()
        .expect("trade creation should succeed");
    assert_eq!(trade.account_id, account.id);
    assert_eq!(trade.status, model::Status::New);
    assert_eq!(trade.category, TradeCategory::Long);
    assert_eq!(trade.currency, Currency::USD);
    assert_eq!(trade.safety_stop.unit_price, dec!(95));
    assert_eq!(trade.safety_stop.category, OrderCategory::StopLimit);
    assert_eq!(trade.entry.unit_price, dec!(100));
    assert_eq!(trade.target.unit_price, dec!(110));
    assert_eq!(trade.thesis.as_deref(), Some("breakout"));
    assert_eq!(trade.sector.as_deref(), Some("technology"));
    assert_eq!(trade.asset_class.as_deref(), Some("stocks"));
    assert_eq!(trade.context.as_deref(), Some("range expansion"));

    built.display();
}

#[test]
fn build_rejects_when_account_is_missing_after_trading_vehicle_is_set() {
    let mut trust = test_trust();
    let before = trust.search_all_accounts().unwrap();
    let rejected = TradeDialogBuilder {
        account: None,
        trading_vehicle: Some(model::TradingVehicle::default()),
        category: Some(TradeCategory::Long),
        entry_price: Some(dec!(100)),
        stop_price: Some(dec!(95)),
        safety_order_category: None,
        currency: Some(Currency::USD),
        quantity: Some(dec!(1)),
        target_price: Some(dec!(110)),
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
        result: None,
    }
    .build(&mut trust);

    let error = rejected
        .result
        .unwrap()
        .expect_err("incomplete input must be rejected");
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(trust.search_all_accounts().unwrap(), before);
}

#[test]
fn io_backed_setters_cover_success_cancel_and_validation_paths() {
    let mut trust = test_trust();
    let account = trust
        .create_account("trade-io", "desc", Environment::Paper, dec!(20), dec!(10))
        .expect("account should be created");
    let _ = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10_000),
            &Currency::USD,
        )
        .expect("deposit should succeed");

    let mut io = ScriptedIo::with(
        vec![Ok(Some(0)), Ok(Some(0)), Ok(None)],
        vec![
            Ok("100".to_string()),
            Ok("95".to_string()),
            Ok("2".to_string()),
            Ok("110".to_string()),
            Ok("breakout".to_string()),
            Ok("technology".to_string()),
            Ok("stocks".to_string()),
            Ok("s/r".to_string()),
            Ok("bad".to_string()),
            Ok("".to_string()),
            Err(IoError::new(ErrorKind::BrokenPipe, "io failed")),
        ],
    );

    let builder = TradeDialogBuilder {
        account: Some(account),
        ..TradeDialogBuilder::new()
    }
    .category_with_io(&mut io)
    .entry_price_with_io(&mut io)
    .stop_price_with_io(&mut io)
    .currency_with_io(&mut trust, &mut io)
    .quantity_with_io(&mut trust, &mut io)
    .target_price_with_io(&mut io)
    .thesis_with_io(&mut io)
    .sector_with_io(&mut io)
    .asset_class_with_io(&mut io)
    .context_with_io(&mut io);

    assert_eq!(builder.category, Some(TradeCategory::Long));
    assert_eq!(builder.entry_price, Some(dec!(100)));
    assert_eq!(builder.stop_price, Some(dec!(95)));
    assert_eq!(builder.currency, Some(Currency::USD));
    assert_eq!(builder.quantity, Some(dec!(2)));
    assert_eq!(builder.target_price, Some(dec!(110)));
    assert_eq!(builder.thesis.as_deref(), Some("breakout"));
    assert_eq!(builder.sector.as_deref(), Some("technology"));
    assert_eq!(builder.asset_class.as_deref(), Some("stocks"));
    assert_eq!(builder.context.as_deref(), Some("s/r"));

    let unchanged = builder
        .entry_price_with_io(&mut io)
        .thesis_with_io(&mut io)
        .context_with_io(&mut io)
        .category_with_io(&mut io);
    assert_eq!(unchanged.entry_price, Some(dec!(100)));
    assert!(unchanged.thesis.is_none());
    assert_eq!(unchanged.category, Some(TradeCategory::Long));
}

#[test]
fn safety_order_category_with_io_sets_stop_limit_and_ignores_cancel() {
    let mut io = ScriptedIo::with(vec![Ok(Some(1)), Ok(None)], vec![]);

    let builder = TradeDialogBuilder::new().safety_order_category_with_io(&mut io);
    assert_eq!(
        builder.safety_order_category,
        Some(OrderCategory::StopLimit)
    );

    let unchanged = builder.safety_order_category_with_io(&mut io);
    assert_eq!(
        unchanged.safety_order_category,
        Some(OrderCategory::StopLimit)
    );
}

#[test]
fn required_selection_and_price_setters_cover_error_paths() {
    let mut trust = test_trust();
    let account = account_with_usd_deposit(&mut trust, "trade-io-errors", dec!(100));

    let mut category_error = ScriptedIo::with(
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "category failed"))],
        vec![],
    );
    let builder = TradeDialogBuilder::new().category_with_io(&mut category_error);
    assert!(builder.category.is_none());

    let mut entry_invalid = ScriptedIo::with(vec![], vec![Ok("not-a-decimal".to_string())]);
    let builder = TradeDialogBuilder::new().entry_price_with_io(&mut entry_invalid);
    assert!(builder.entry_price.is_none());

    let mut entry_error = ScriptedIo::with(
        vec![],
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "entry failed"))],
    );
    let builder = TradeDialogBuilder::new().entry_price_with_io(&mut entry_error);
    assert!(builder.entry_price.is_none());

    let mut stop_invalid = ScriptedIo::with(vec![], vec![Ok("bad-stop".to_string())]);
    let builder = TradeDialogBuilder::new().stop_price_with_io(&mut stop_invalid);
    assert!(builder.stop_price.is_none());

    let mut stop_error = ScriptedIo::with(
        vec![],
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "stop failed"))],
    );
    let builder = TradeDialogBuilder::new().stop_price_with_io(&mut stop_error);
    assert!(builder.stop_price.is_none());

    let mut safety_order_error = ScriptedIo::with(
        vec![Err(IoError::new(
            ErrorKind::BrokenPipe,
            "safety order type failed",
        ))],
        vec![],
    );
    let builder = TradeDialogBuilder::new().safety_order_category_with_io(&mut safety_order_error);
    assert!(builder.safety_order_category.is_none());

    let mut currency_cancel = ScriptedIo::with(vec![Ok(None)], vec![]);
    let builder = TradeDialogBuilder {
        account: Some(account.clone()),
        ..TradeDialogBuilder::new()
    }
    .currency_with_io(&mut trust, &mut currency_cancel);
    assert!(builder.currency.is_none());

    let mut currency_error = ScriptedIo::with(
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "currency failed"))],
        vec![],
    );
    let builder = TradeDialogBuilder {
        account: Some(account.clone()),
        ..TradeDialogBuilder::new()
    }
    .currency_with_io(&mut trust, &mut currency_error);
    assert!(builder.currency.is_none());

    let mut target_invalid = ScriptedIo::with(vec![], vec![Ok("bad-target".to_string())]);
    let builder = TradeDialogBuilder::new().target_price_with_io(&mut target_invalid);
    assert!(builder.target_price.is_none());

    let mut target_error = ScriptedIo::with(
        vec![],
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "target failed"))],
    );
    let builder = TradeDialogBuilder::new().target_price_with_io(&mut target_error);
    assert!(builder.target_price.is_none());
}

#[test]
fn quantity_with_io_covers_rejection_and_calculation_error_paths() {
    let mut trust = test_trust();
    let account = account_with_usd_deposit(&mut trust, "trade-quantity-errors", dec!(100));

    let base_builder = base_quantity_builder(account.clone(), dec!(100));
    let mut too_large = ScriptedIo::with(vec![], vec![Ok("2".to_string())]);
    let builder = base_builder.quantity_with_io(&mut trust, &mut too_large);
    assert!(builder.quantity.is_none());

    let base_builder = base_quantity_builder(account.clone(), dec!(100));
    let mut zero = ScriptedIo::with(vec![], vec![Ok("0".to_string())]);
    let builder = base_builder.quantity_with_io(&mut trust, &mut zero);
    assert!(builder.quantity.is_none());

    let base_builder = base_quantity_builder(account.clone(), dec!(100));
    let mut invalid_quantity = ScriptedIo::with(vec![], vec![Ok("nan".to_string())]);
    let builder = base_builder.quantity_with_io(&mut trust, &mut invalid_quantity);
    assert!(builder.quantity.is_none());

    let base_builder = base_quantity_builder(account.clone(), dec!(100));
    let mut quantity_error = ScriptedIo::with(
        vec![],
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "quantity failed"))],
    );
    let builder = base_builder.quantity_with_io(&mut trust, &mut quantity_error);
    assert!(builder.quantity.is_none());

    let calc_error_builder = base_quantity_builder(account, dec!(0));
    let mut calc_error = ScriptedIo::with(vec![], vec![Ok("1".to_string())]);
    let builder = calc_error_builder.quantity_with_io(&mut trust, &mut calc_error);
    assert!(builder.quantity.is_none());
}

#[test]
fn optional_metadata_setters_cover_empty_too_long_and_io_errors() {
    let mut default_input = ScriptedIo::with(vec![], vec![]);
    assert_eq!(
        default_input
            .input_text("ignored", true)
            .expect("default text input"),
        ""
    );

    let mut thesis_too_long = ScriptedIo::with(vec![], vec![Ok("x".repeat(201))]);
    let builder = TradeDialogBuilder::new().thesis_with_io(&mut thesis_too_long);
    assert!(builder.thesis.is_none());

    let mut thesis_error = ScriptedIo::with(
        vec![],
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "thesis failed"))],
    );
    let builder = TradeDialogBuilder::new().thesis_with_io(&mut thesis_error);
    assert!(builder.thesis.is_none());

    let mut sector_empty = ScriptedIo::with(vec![], vec![Ok(String::new())]);
    let builder = TradeDialogBuilder::new().sector_with_io(&mut sector_empty);
    assert!(builder.sector.is_none());

    let mut sector_error = ScriptedIo::with(
        vec![],
        vec![Err(IoError::new(ErrorKind::BrokenPipe, "sector failed"))],
    );
    let builder = TradeDialogBuilder::new().sector_with_io(&mut sector_error);
    assert!(builder.sector.is_none());

    let mut asset_class_empty = ScriptedIo::with(vec![], vec![Ok(String::new())]);
    let builder = TradeDialogBuilder::new().asset_class_with_io(&mut asset_class_empty);
    assert!(builder.asset_class.is_none());

    let mut asset_class_error = ScriptedIo::with(
        vec![],
        vec![Err(IoError::new(
            ErrorKind::BrokenPipe,
            "asset class failed",
        ))],
    );
    let builder = TradeDialogBuilder::new().asset_class_with_io(&mut asset_class_error);
    assert!(builder.asset_class.is_none());

    let mut context_empty = ScriptedIo::with(vec![], vec![Ok(String::new())]);
    let builder = TradeDialogBuilder::new().context_with_io(&mut context_empty);
    assert!(builder.context.is_none());
}

#[test]
fn search_wrappers_handle_missing_records_and_scripted_confirm() {
    let mut trust = test_trust();
    scripted_reset();

    let builder = TradeDialogBuilder::new()
        .account(&mut trust)
        .trading_vehicle(&mut trust);
    assert!(builder.account.is_none());
    assert!(builder.trading_vehicle.is_none());

    let mut io = ScriptedIo::with(vec![], vec![]);
    assert!(!io.confirm("ignored", true).expect("confirm should succeed"));
}

#[test]
fn wrapper_methods_use_default_console_io_in_tests() {
    let mut trust = test_trust();
    let account = trust
        .create_account("trade-wrap", "desc", Environment::Paper, dec!(20), dec!(10))
        .expect("account should be created");
    trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10_000),
            &Currency::USD,
        )
        .expect("deposit should succeed");
    let trading_vehicle = trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("trading vehicle should be created");

    scripted_reset();
    scripted_push_select(Ok(Some(0)));
    scripted_push_select(Ok(Some(0)));
    scripted_push_select(Ok(Some(0)));
    scripted_push_select(Ok(Some(1)));
    scripted_push_input(Ok("100".to_string()));
    scripted_push_input(Ok("95".to_string()));
    scripted_push_select(Ok(Some(0)));
    scripted_push_input(Ok("2".to_string()));
    scripted_push_input(Ok("110".to_string()));
    scripted_push_input(Ok("breakout".to_string()));
    scripted_push_input(Ok("technology".to_string()));
    scripted_push_input(Ok("stocks".to_string()));
    scripted_push_input(Ok("s/r".to_string()));

    let builder = TradeDialogBuilder::new()
        .account(&mut trust)
        .trading_vehicle(&mut trust)
        .category()
        .entry_price()
        .stop_price()
        .safety_order_category()
        .currency(&mut trust)
        .quantity(&mut trust)
        .target_price()
        .thesis()
        .sector()
        .asset_class()
        .context();

    assert_eq!(
        builder.account.as_ref().expect("selected account").id,
        account.id
    );
    assert_eq!(
        builder
            .trading_vehicle
            .as_ref()
            .expect("selected trading vehicle")
            .id,
        trading_vehicle.id
    );
    assert_eq!(builder.category, Some(TradeCategory::Long));
    assert_eq!(builder.entry_price, Some(dec!(100)));
    assert_eq!(builder.stop_price, Some(dec!(95)));
    assert_eq!(
        builder.safety_order_category,
        Some(OrderCategory::StopLimit)
    );
    assert_eq!(builder.currency, Some(Currency::USD));
    assert_eq!(builder.quantity, Some(dec!(2)));
    assert_eq!(builder.target_price, Some(dec!(110)));
    assert_eq!(builder.thesis.as_deref(), Some("breakout"));
    assert_eq!(builder.sector.as_deref(), Some("technology"));
    assert_eq!(builder.asset_class.as_deref(), Some("stocks"));
    assert_eq!(builder.context.as_deref(), Some("s/r"));
    scripted_reset();
}
