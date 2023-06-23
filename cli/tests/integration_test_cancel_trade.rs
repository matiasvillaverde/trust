use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::Broker;
use model::{
    Account, BrokerLog, Currency, DraftTrade, Order, OrderIds, Status, Trade, TradeCategory,
    TradingVehicleCategory, TransactionCategory,
};
use rust_decimal_macros::dec;
use std::error::Error;

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(MockBroker))
}

#[test]
fn test_cancel_of_funded_trade() {
    let mut trust = create_trust();

    // 1. Create account
    trust
        .create_account(
            "alpaca",
            "default",
            model::Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .unwrap();
    let account = trust.search_account("alpaca").unwrap();

    // 2. Create transaction deposit
    let (_, _) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100000),
            &Currency::USD,
        )
        .unwrap();

    // 3. Create trading vehicle
    let tv = trust
        .create_trading_vehicle(
            "TSLA",
            "US88160R1014",
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .expect("Failed to create trading vehicle");

    // 4. Create trade
    let trade = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 500,
        currency: Currency::USD,
        category: TradeCategory::Long,
    };

    trust
        .create_trade(trade, dec!(38), dec!(40), dec!(50))
        .expect("Failed to create trade");
    let trade = trust
        .search_trades(account.id, Status::New)
        .expect("Failed to find trade")
        .first()
        .unwrap()
        .clone();

    // 5. Fund trade
    trust.fund_trade(&trade).unwrap();
    let trade = trust
        .search_trades(account.id, Status::Funded)
        .expect("Failed to find trade with status funded")
        .first()
        .unwrap()
        .clone();

    let (trade_o, account_o, tx) = trust.cancel_funded_trade(&trade).unwrap();

    let trade = trust
        .search_trades(account.id, Status::Canceled)
        .expect("Failed to find trade with status canceled")
        .first()
        .unwrap()
        .clone();

    assert_eq!(trade.status, Status::Canceled);
    assert_eq!(tx.category, TransactionCategory::PaymentFromTrade(trade.id));
    assert_eq!(tx.amount, dec!(20000));
    assert_eq!(tx.currency, Currency::USD);
    assert_eq!(account_o.total_balance, dec!(100000));
    assert_eq!(account_o.total_available, dec!(100000));
    assert_eq!(account_o.total_in_trade, dec!(0));
    assert_eq!(trade_o.capital_out_market, dec!(0));
    assert_eq!(trade_o.capital_in_market, dec!(0));
    assert_eq!(trade_o.total_performance, dec!(0));
}

struct MockBroker;
impl Broker for MockBroker {
    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        unimplemented!()
    }

    fn sync_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        unimplemented!()
    }

    fn close_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        unimplemented!()
    }
}
