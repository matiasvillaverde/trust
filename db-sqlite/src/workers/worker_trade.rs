use crate::error::{ConversionError, IntoDomainModel};
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
            .map_err(|error| {
                error!("Error creating trade: {:?}", error);
                error
            })?
            .try_into_domain_model(connection)?;
        Ok(trade)
    }

    pub fn read_balance(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<TradeBalance, Box<dyn Error>> {
        trades_balances::table
            .filter(trades_balances::id.eq(&id.to_string()))
            .first::<AccountBalanceSQLite>(connection)
            .map_err(|e| Box::new(e) as Box<dyn Error>)?
            .into_domain_model()
    }

    pub fn read_trade(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<Trade, Box<dyn Error>> {
        let trade = trades::table
            .filter(trades::id.eq(id.to_string()))
            .first::<TradeSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trade: {:?}", error);
                error
            })?
            .try_into_domain_model(connection)?;
        Ok(trade)
    }

    pub fn read_all_funded_trades_for_currency(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades_sqlite = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::currency.eq(currency.to_string()))
            .filter(trades::status.eq(Status::Funded.to_string()))
            .load::<TradeSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trades: {:?}", error);
                error
            })?;

        let mut trades = Vec::new();
        for trade_sqlite in trades_sqlite {
            trades.push(trade_sqlite.try_into_domain_model(connection)?);
        }
        Ok(trades)
    }

    pub fn read_all_trades_with_status(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades_sqlite = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::status.eq(status.to_string()))
            .load::<TradeSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trades: {:?}", error);
                error
            })?;

        let mut trades = Vec::new();
        for trade_sqlite in trades_sqlite {
            trades.push(trade_sqlite.try_into_domain_model(connection)?);
        }
        Ok(trades)
    }

    pub fn read_all_trades_with_status_currency(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        status: Status,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades_sqlite = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::status.eq(status.to_string()))
            .filter(trades::currency.eq(currency.to_string()))
            .load::<TradeSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trades: {:?}", error);
                error
            })?;

        let mut trades = Vec::new();
        for trade_sqlite in trades_sqlite {
            trades.push(trade_sqlite.try_into_domain_model(connection)?);
        }
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
            .map_err(|error| {
                error!("Error creating trade balance: {:?}", error);
                error
            })?
            .into_domain_model()?;
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
            .map_err(|error| {
                error!("Error updating balance: {:?}", error);
                error
            })?
            .into_domain_model()?;
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
            .map_err(|error| {
                error!("Error executing trade: {:?}", error);
                error
            })?
            .try_into_domain_model(connection)?;
        Ok(trade)
    }
}

// Trade

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
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
    fn try_into_domain_model(
        self,
        connection: &mut SqliteConnection,
    ) -> Result<Trade, Box<dyn Error>> {
        let trading_vehicle_id = Uuid::parse_str(&self.trading_vehicle_id).map_err(|_| {
            ConversionError::new("trading_vehicle_id", "Failed to parse trading vehicle ID")
        })?;
        let trading_vehicle =
            WorkerTradingVehicle::read(connection, trading_vehicle_id).map_err(|e| {
                ConversionError::new(
                    "trading_vehicle",
                    format!("Failed to read trading vehicle: {}", e),
                )
            })?;

        let safety_stop_id = Uuid::parse_str(&self.safety_stop_id).map_err(|_| {
            ConversionError::new("safety_stop_id", "Failed to parse safety stop ID")
        })?;
        let safety_stop = WorkerOrder::read(connection, safety_stop_id).map_err(|e| {
            ConversionError::new(
                "safety_stop",
                format!("Failed to read safety stop order: {}", e),
            )
        })?;

        let entry_id = Uuid::parse_str(&self.entry_id)
            .map_err(|_| ConversionError::new("entry_id", "Failed to parse entry ID"))?;
        let entry = WorkerOrder::read(connection, entry_id).map_err(|e| {
            ConversionError::new("entry", format!("Failed to read entry order: {}", e))
        })?;

        let target_id = Uuid::parse_str(&self.target_id)
            .map_err(|_| ConversionError::new("target_id", "Failed to parse target ID"))?;
        let targets = WorkerOrder::read(connection, target_id).map_err(|e| {
            ConversionError::new("target", format!("Failed to read target order: {}", e))
        })?;

        let balance_id = Uuid::parse_str(&self.balance_id)
            .map_err(|_| ConversionError::new("balance_id", "Failed to parse balance ID"))?;
        let balance = WorkerTrade::read_balance(connection, balance_id).map_err(|e| {
            ConversionError::new("balance", format!("Failed to read trade balance: {}", e))
        })?;

        Ok(Trade {
            id: Uuid::parse_str(&self.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse trade ID"))?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            trading_vehicle,
            category: TradeCategory::from_str(&self.category)
                .map_err(|_| ConversionError::new("category", "Failed to parse trade category"))?,
            status: Status::from_str(&self.status)
                .map_err(|_| ConversionError::new("status", "Failed to parse trade status"))?,
            currency: Currency::from_str(&self.currency)
                .map_err(|_| ConversionError::new("currency", "Failed to parse currency"))?,
            safety_stop,
            entry,
            target: targets,
            account_id: Uuid::parse_str(&self.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            balance,
        })
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

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
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

impl TryFrom<AccountBalanceSQLite> for TradeBalance {
    type Error = ConversionError;

    fn try_from(value: AccountBalanceSQLite) -> Result<Self, Self::Error> {
        Ok(TradeBalance {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse balance ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            currency: Currency::from_str(&value.currency)
                .map_err(|_| ConversionError::new("currency", "Failed to parse currency"))?,
            funding: Decimal::from_str(&value.funding)
                .map_err(|_| ConversionError::new("funding", "Failed to parse funding amount"))?,
            capital_in_market: Decimal::from_str(&value.capital_in_market).map_err(|_| {
                ConversionError::new("capital_in_market", "Failed to parse capital in market")
            })?,
            capital_out_market: Decimal::from_str(&value.capital_out_market).map_err(|_| {
                ConversionError::new("capital_out_market", "Failed to parse capital out market")
            })?,
            taxed: Decimal::from_str(&value.taxed)
                .map_err(|_| ConversionError::new("taxed", "Failed to parse taxed amount"))?,
            total_performance: Decimal::from_str(&value.total_performance).map_err(|_| {
                ConversionError::new("total_performance", "Failed to parse total performance")
            })?,
        })
    }
}

impl IntoDomainModel<TradeBalance> for AccountBalanceSQLite {
    fn into_domain_model(self) -> Result<TradeBalance, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
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
