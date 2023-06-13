use rust_decimal_macros::dec;
use std::error::Error;
use trust_core::TrustFacade;
use trust_db_sqlite::SqliteDatabase;
use trust_model::Broker;
use trust_model::{
    Account, BrokerLog, Currency, Order, OrderIds, Status, Trade, TransactionCategory,
};

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(MockBroker {}))
}

#[test]
fn test_account_creation() {
    let mut trust = create_trust();

    trust
        .create_account("alpaca", "default", trust_model::Environment::Paper)
        .unwrap();
    let account = trust.search_account("alpaca").unwrap();
    let accounts: Vec<Account> = trust.search_all_accounts().unwrap();

    assert_eq!(account.name, "alpaca");
    assert_eq!(account.description, "default");
    assert_eq!(account.environment, trust_model::Environment::Paper);
    assert_eq!(accounts.len(), 1);
}

#[test]
fn test_transactions() {
    let mut trust = create_trust();

    let account = trust
        .create_account("alpaca", "default", trust_model::Environment::Paper)
        .unwrap();

    let (tx, overview) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(40000),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.amount, dec!(40000));
    assert_eq!(tx.category, TransactionCategory::Deposit);
    assert_eq!(tx.currency, Currency::USD);
    assert_eq!(tx.account_id, account.id);
    assert_eq!(overview.account_id, account.id);
    assert_eq!(overview.currency, Currency::USD);
    assert_eq!(overview.total_available, dec!(40000));
    assert_eq!(overview.total_balance, dec!(40000));
    assert_eq!(overview.total_in_trade, dec!(0));
    assert_eq!(overview.taxed, dec!(0));
}

struct MockBroker {}

impl Broker for MockBroker {
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        unimplemented!()
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
        unimplemented!()
    }
}
