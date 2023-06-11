use apca::Client;
use std::error::Error;
use tokio::runtime::Runtime;
use trust_model::{
    Account, Broker, BrokerLog, Environment, Order, OrderIds, Status, Trade, TradeCategory,
};

mod keys;
mod order_mapper;
mod submit_trade;
mod sync;
pub use keys::Keys;

#[derive(Default)]
pub struct AlpacaBroker;

/// Generic Broker API
impl Broker for AlpacaBroker {
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        submit_trade::submit_sync(trade, account)
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>), Box<dyn Error>> {
        assert!(trade.account_id == account.id); // Verify that the trade is for the account

        let api_info = keys::read_api_key(&account.environment, account)?;
        let client = Client::new(api_info);

        Runtime::new()
            .unwrap()
            .block_on(sync::sync_trade(&client, trade))
    }
}

/// Alpaca-specific Broker API
impl AlpacaBroker {
    pub fn setup_keys(
        key_id: &str,
        secret: &str,
        url: &str,
        environment: &Environment,
        account: &Account,
    ) -> Result<Keys, Box<dyn Error>> {
        let keys = Keys::new(key_id, secret, url);
        let keys = keys.store(environment, &account.name)?;
        Ok(keys)
    }

    pub fn read_keys(environment: &Environment, account: &Account) -> Result<Keys, Box<dyn Error>> {
        let keys = Keys::read(environment, &account.name)?;
        Ok(keys)
    }

    pub fn delete_keys(environment: &Environment, account: &Account) -> Result<(), Box<dyn Error>> {
        Keys::delete(environment, &account.name)?;
        Ok(())
    }
}
