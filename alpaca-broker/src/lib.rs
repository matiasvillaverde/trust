use model::{Account, Broker, BrokerLog, Environment, Order, OrderIds, Status, Trade};
use std::error::Error;

mod cancel_trade;
mod close_trade;
mod keys;
mod modify_trade;
mod order_mapper;
mod submit_trade;
mod sync_trade;
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
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        sync_trade::sync(trade, account)
    }

    fn close_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        close_trade::close(trade, account)
    }

    fn cancel_trade(&self, trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
        println!("Canceling trade: {:?}", trade);
        cancel_trade::cancel(trade, account)
    }

    fn modify_stop(
        &self,
        trade: &Trade,
        account: &Account,
        new_stop_price: rust_decimal::Decimal,
    ) -> Result<BrokerLog, Box<dyn Error>> {
        modify_trade::modify_stop(trade, account, new_stop_price)
    }

    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        new_target_price: rust_decimal::Decimal,
    ) -> Result<BrokerLog, Box<dyn Error>> {
        unimplemented!("Alpaca does not support modifying target prices")
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
