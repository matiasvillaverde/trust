//! Real persistence for admission contracts; the broker must remain untouched.
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerLog, Currency, DraftTrade, Order, OrderIds, RuleLevel, RuleName, Status,
    Trade, TradeCategory, TradingVehicle, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub struct Fixture {
    pub trust: TrustFacade,
    pub account: Account,
    pub vehicle: TradingVehicle,
    database: SqliteDatabase,
    directory: tempfile::TempDir,
    broker_calls: Arc<AtomicUsize>,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("admission.db");
        let url = path.to_str().unwrap();
        let broker_calls = Arc::new(AtomicUsize::new(0));
        let mut trust = TrustFacade::new(
            Box::new(SqliteDatabase::new(url)),
            Box::new(ForbiddenBroker(broker_calls.clone())),
        );
        let account = trust
            .create_account(
                "admission",
                "boundary contract",
                model::Environment::Paper,
                dec!(0),
                dec!(0),
            )
            .unwrap();
        trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(50000),
                &Currency::USD,
            )
            .unwrap();
        for name in [RuleName::RiskPerMonth(20.0), RuleName::RiskPerTrade(2.0)] {
            trust
                .create_rule(&account, &name, "admission policy", &RuleLevel::Error)
                .unwrap();
        }
        let vehicle = trust
            .upsert_trading_vehicle(model::database::TradingVehicleUpsert {
                symbol: "AAPL".into(),
                isin: Some("US0378331005".into()),
                category: TradingVehicleCategory::Stock,
                broker: "alpaca".into(),
                broker_asset_id: Some("test-aapl".into()),
                exchange: Some("NASDAQ".into()),
                broker_asset_class: Some("us_equity".into()),
                broker_asset_status: Some("active".into()),
                tradable: Some(true),
                marginable: Some(true),
                shortable: Some(true),
                easy_to_borrow: Some(true),
                fractionable: Some(true),
                fixed_income: None,
            })
            .unwrap();
        Self {
            trust,
            account,
            vehicle,
            database: SqliteDatabase::new(url),
            directory,
            broker_calls,
        }
    }

    pub fn draft(&self, category: TradeCategory, quantity: Decimal) -> DraftTrade {
        DraftTrade {
            account: self.account.clone(),
            trading_vehicle: self.vehicle.clone(),
            quantity,
            currency: Currency::USD,
            category,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        }
    }

    pub fn tables(&self) -> serde_json::Value {
        let path = self.directory.path().join("snapshot.json");
        self.database.export_backup_to_path(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        value.get("tables").expect("backup tables").clone()
    }

    pub fn assert_no_broker_calls(&self) {
        assert_eq!(
            self.broker_calls.load(Ordering::SeqCst),
            0,
            "creation/funding must not call a broker"
        );
    }
}

struct ForbiddenBroker(Arc<AtomicUsize>);
impl ForbiddenBroker {
    fn reject<T>(&self) -> Result<T, Box<dyn Error>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err("unexpected broker call in admission contract".into())
    }
}
impl Broker for ForbiddenBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        self.reject()
    }
    fn sync_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        self.reject()
    }
    fn close_trade(&self, _: &Trade, _: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        self.reject()
    }
    fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn Error>> {
        self.reject()
    }
    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        self.reject()
    }
    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        self.reject()
    }
}
