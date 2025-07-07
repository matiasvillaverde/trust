use crate::schema::{trades, trades_balances};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{Currency, DraftTrade, Status};
use model::{Order, Trade, TradeBalance, TradeCategory};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

use super::{WorkerOrder, WorkerTradingVehicle};

/// Worker for handling trade database operations
#[derive(Debug)]
pub struct WorkerTrade;

impl WorkerTrade {
    pub fn create(
        connection: &mut SqliteConnection,
        draft: DraftTrade,
        safety_stop: &Order,
        entry: &Order,
        target: &Order,
    ) -> Result<Trade, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let balance = WorkerTrade::create_balance(connection, &draft.currency, now)?;

        let new_trade = NewTrade {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            category: draft.category.to_string(),
            status: Status::default().to_string(),
            currency: draft.currency.to_string(),
            trading_vehicle_id: draft.trading_vehicle.id.to_string(),
            safety_stop_id: safety_stop.id.to_string(),
            entry_id: entry.id.to_string(),
            target_id: target.id.to_string(),
            account_id: draft.account.id.to_string(),
            balance_id: balance.id.to_string(),
        };

        let trade = diesel::insert_into(trades::table)
            .values(&new_trade)
            .get_result::<TradeSQLite>(connection)
            .map(|trade| trade.domain_model(connection))
            .map_err(|error| {
                error!("Error creating trade: {:?}", error);
                error
            })?;
        Ok(trade)
    }

    pub fn read_balance(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<TradeBalance, diesel::result::Error> {
        trades_balances::table
            .filter(trades_balances::id.eq(&id.to_string()))
            .first(connection)
            .map(|balance: AccountBalanceSQLite| balance.domain_model())
    }

    pub fn read_trade(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<Trade, Box<dyn Error>> {
        let trade = trades::table
            .filter(trades::id.eq(id.to_string()))
            .first::<TradeSQLite>(connection)
            .map(|account| account.domain_model(connection))
            .map_err(|error| {
                error!("Error reading trade: {:?}", error);
                error
            })?;
        Ok(trade)
    }

    pub fn read_all_funded_trades_for_currency(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades: Vec<Trade> = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::currency.eq(currency.to_string()))
            .filter(trades::status.eq(Status::Funded.to_string()))
            .load::<TradeSQLite>(connection)
            .map(|trades: Vec<TradeSQLite>| {
                trades
                    .into_iter()
                    .map(|trade| trade.domain_model(connection))
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading trades: {:?}", error);
                error
            })?;
        Ok(trades)
    }

    pub fn read_all_trades_with_status(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades: Vec<Trade> = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::status.eq(status.to_string()))
            .load::<TradeSQLite>(connection)
            .map(|trades: Vec<TradeSQLite>| {
                trades
                    .into_iter()
                    .map(|trade| trade.domain_model(connection))
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading trades: {:?}", error);
                error
            })?;
        Ok(trades)
    }

    pub fn read_all_trades_with_status_currency(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        status: Status,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades: Vec<Trade> = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::status.eq(status.to_string()))
            .filter(trades::currency.eq(currency.to_string()))
            .load::<TradeSQLite>(connection)
            .map(|trades: Vec<TradeSQLite>| {
                trades
                    .into_iter()
                    .map(|trade| trade.domain_model(connection))
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading trades: {:?}", error);
                error
            })?;
        Ok(trades)
    }

    fn create_balance(
        connection: &mut SqliteConnection,
        currency: &Currency,
        _created_at: NaiveDateTime,
    ) -> Result<TradeBalance, Box<dyn Error>> {
        let new_trade_balance = NewAccountBalance {
            currency: currency.to_string(),
            ..Default::default()
        };

        let balance = diesel::insert_into(trades_balances::table)
            .values(&new_trade_balance)
            .get_result::<AccountBalanceSQLite>(connection)
            .map(|balance| balance.domain_model())
            .map_err(|error| {
                error!("Error creating trade balance: {:?}", error);
                error
            })?;
        Ok(balance)
    }

    pub fn update_trade_balance(
        connection: &mut SqliteConnection,
        trade: &Trade,
        funding: Decimal,
        capital_in_market: Decimal,
        capital_out_market: Decimal,
        taxed: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeBalance, Box<dyn Error>> {
        let balance = diesel::update(trades_balances::table)
            .filter(trades_balances::id.eq(&trade.balance.id.to_string()))
            .set((
                trades_balances::updated_at.eq(Utc::now().naive_utc()),
                trades_balances::funding.eq(funding.to_string()),
                trades_balances::capital_in_market.eq(capital_in_market.to_string()),
                trades_balances::capital_out_market.eq(capital_out_market.to_string()),
                trades_balances::taxed.eq(taxed.to_string()),
                trades_balances::total_performance.eq(total_performance.to_string()),
            ))
            .get_result::<AccountBalanceSQLite>(connection)
            .map(|o| o.domain_model())
            .map_err(|error| {
                error!("Error updating balance: {:?}", error);
                error
            })?;
        Ok(balance)
    }

    pub fn update_trade_status(
        connection: &mut SqliteConnection,
        status: Status,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let trade = diesel::update(trades::table)
            .filter(trades::id.eq(trade.id.to_string()))
            .set((
                trades::updated_at.eq(now),
                trades::status.eq(status.to_string()),
            ))
            .get_result::<TradeSQLite>(connection)
            .map(|trade| trade.domain_model(connection))
            .map_err(|error| {
                error!("Error executing trade: {:?}", error);
                error
            })?;
        Ok(trade)
    }
}

// Trade

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trades)]
struct TradeSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    category: String,
    status: String,
    currency: String,
    trading_vehicle_id: String,
    safety_stop_id: String,
    entry_id: String,
    target_id: String,
    account_id: String,
    balance_id: String,
}

impl TradeSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> Trade {
        let trading_vehicle = WorkerTradingVehicle::read(
            connection,
            Uuid::parse_str(&self.trading_vehicle_id).expect("Failed to parse trading vehicle ID"),
        )
        .expect("Failed to read trading vehicle");
        let safety_stop = WorkerOrder::read(
            connection,
            Uuid::parse_str(&self.safety_stop_id).expect("Failed to parse safety stop ID"),
        )
        .expect("Failed to read safety stop order");
        let entry = WorkerOrder::read(
            connection,
            Uuid::parse_str(&self.entry_id).expect("Failed to parse entry ID"),
        )
        .expect("Failed to read entry order");
        let targets = WorkerOrder::read(
            connection,
            Uuid::parse_str(&self.target_id).expect("Failed to parse target ID"),
        )
        .expect("Failed to read target order");
        let balance = WorkerTrade::read_balance(
            connection,
            Uuid::parse_str(&self.balance_id).expect("Failed to parse balance ID"),
        )
        .expect("Failed to read trade balance");

        Trade {
            id: Uuid::parse_str(&self.id).expect("Failed to parse trade ID"),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            trading_vehicle,
            category: TradeCategory::from_str(&self.category)
                .expect("Failed to parse trade category"),
            status: Status::from_str(&self.status).expect("Failed to parse trade status"),
            currency: Currency::from_str(&self.currency).expect("Failed to parse currency"),
            safety_stop,
            entry,
            target: targets,
            account_id: Uuid::parse_str(&self.account_id).expect("Failed to parse account ID"),
            balance,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = trades)]
#[diesel(treat_none_as_null = true)]
struct NewTrade {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    category: String,
    status: String,
    currency: String,
    trading_vehicle_id: String,
    safety_stop_id: String,
    target_id: String,
    entry_id: String,
    account_id: String,
    balance_id: String,
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trades_balances)]
struct AccountBalanceSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    currency: String,
    funding: String,
    capital_in_market: String,
    capital_out_market: String,
    taxed: String,
    total_performance: String,
}

impl AccountBalanceSQLite {
    fn domain_model(self) -> TradeBalance {
        TradeBalance {
            id: Uuid::parse_str(&self.id).expect("Failed to parse balance ID"),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            currency: Currency::from_str(&self.currency).expect("Failed to parse currency"),
            funding: Decimal::from_str(self.funding.as_str())
                .expect("Failed to parse funding amount"),
            capital_in_market: Decimal::from_str(self.capital_in_market.as_str())
                .expect("Failed to parse capital in market"),
            capital_out_market: Decimal::from_str(self.capital_out_market.as_str())
                .expect("Failed to parse capital out market"),
            taxed: Decimal::from_str(self.taxed.as_str()).expect("Failed to parse taxed amount"),
            total_performance: Decimal::from_str(self.total_performance.as_str())
                .expect("Failed to parse total performance"),
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = trades_balances)]
struct NewAccountBalance {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    currency: String,
    funding: String,
    capital_in_market: String,
    capital_out_market: String,
    taxed: String,
    total_performance: String,
}

impl Default for NewAccountBalance {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        NewAccountBalance {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD.to_string(),
            funding: Decimal::new(0, 0).to_string(),
            capital_in_market: Decimal::new(0, 0).to_string(),
            capital_out_market: Decimal::new(0, 0).to_string(),
            taxed: Decimal::new(0, 0).to_string(),
            total_performance: Decimal::new(0, 0).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {}
