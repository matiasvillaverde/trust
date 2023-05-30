use crate::schema::{trades, trades_overviews};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use trust_model::{Account, Currency};
use trust_model::{Order, Trade, TradeCategory, TradeOverview, TradingVehicle};
use uuid::Uuid;

use super::{WorkerOrder, WorkerPrice, WorkerTarget, WorkerTradingVehicle};
pub struct WorkerTrade;

impl WorkerTrade {
    pub fn create(
        connection: &mut SqliteConnection,
        category: &TradeCategory,
        currency: &Currency,
        trading_vehicle: &TradingVehicle,
        safety_stop: &Order,
        entry: &Order,
        account: &Account,
    ) -> Result<Trade, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let overview = WorkerTrade::create_overview(connection, currency, now)?;

        let new_trade = NewTrade {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            category: category.to_string(),
            currency: currency.to_string(),
            trading_vehicle_id: trading_vehicle.id.to_string(),
            safety_stop_id: safety_stop.id.to_string(),
            entry_id: entry.id.to_string(),
            account_id: account.id.to_string(),
            approved_at: None,
            rejected_at: None,
            opened_at: None,
            failed_at: None,
            closed_at: None,
            rejected_by_rule_id: None,
            overview_id: overview.id.to_string(),
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

    pub fn read_overview(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<TradeOverview, diesel::result::Error> {
        trades_overviews::table
            .filter(trades_overviews::id.eq(&id.to_string()))
            .first(connection)
            .map(|overview: TradeOverviewSQLite| overview.domain_model(connection))
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

    pub fn read_all_new_trades(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades: Vec<Trade> = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::approved_at.is_null())
            .filter(trades::rejected_at.is_null())
            .filter(trades::opened_at.is_null())
            .filter(trades::failed_at.is_null())
            .filter(trades::closed_at.is_null())
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

    pub fn read_all_approved_trades(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades: Vec<Trade> = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::approved_at.is_not_null())
            .filter(trades::rejected_at.is_null())
            .filter(trades::opened_at.is_null())
            .filter(trades::failed_at.is_null())
            .filter(trades::closed_at.is_null())
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

    pub fn read_all_approved_trades_for_currency(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades: Vec<Trade> = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::currency.eq(currency.to_string()))
            .filter(trades::approved_at.is_not_null())
            .filter(trades::rejected_at.is_null())
            .filter(trades::opened_at.is_null())
            .filter(trades::failed_at.is_null())
            .filter(trades::closed_at.is_null())
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

    pub fn read_all_open_trades(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        let trades: Vec<Trade> = trades::table
            .filter(trades::deleted_at.is_null())
            .filter(trades::account_id.eq(account_id.to_string()))
            .filter(trades::approved_at.is_not_null())
            .filter(trades::rejected_at.is_null())
            .filter(trades::opened_at.is_not_null())
            .filter(trades::failed_at.is_null())
            .filter(trades::closed_at.is_null())
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

    fn create_overview(
        connection: &mut SqliteConnection,
        currency: &Currency,
        created_at: NaiveDateTime,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        let funding_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let capital_in_market_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let capital_out_market_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let taxed_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let total_performance_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();

        let new_trade_overview = NewTradeOverview {
            id: Uuid::new_v4().to_string(),
            created_at,
            updated_at: created_at,
            deleted_at: None,
            funding_id,
            capital_in_market_id,
            capital_out_market_id,
            taxed_id,
            total_performance_id,
        };

        let overview = diesel::insert_into(trades_overviews::table)
            .values(&new_trade_overview)
            .get_result::<TradeOverviewSQLite>(connection)
            .map(|overview| overview.domain_model(connection))
            .map_err(|error| {
                error!("Error creating trade overview: {:?}", error);
                error
            })?;
        Ok(overview)
    }

    pub fn update_trade_overview(
        connection: &mut SqliteConnection,
        trade: &Trade,
        funding: Decimal,
        capital_in_market: Decimal,
        capital_out_market: Decimal,
        taxed: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        WorkerPrice::update(connection, trade.overview.funding, funding)?;
        WorkerPrice::update(
            connection,
            trade.overview.capital_in_market,
            capital_in_market,
        )?;
        WorkerPrice::update(
            connection,
            trade.overview.capital_out_market,
            capital_out_market,
        )?;
        WorkerPrice::update(connection, trade.overview.taxed, taxed)?;
        WorkerPrice::update(
            connection,
            trade.overview.total_performance,
            total_performance,
        )?;
        let overview = WorkerTrade::read_overview(connection, trade.overview.id)?;
        Ok(overview)
    }

    pub fn approve_trade(
        connection: &mut SqliteConnection,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let trade = diesel::update(trades::table)
            .filter(trades::id.eq(trade.id.to_string()))
            .set((trades::updated_at.eq(now), trades::approved_at.eq(now)))
            .get_result::<TradeSQLite>(connection)
            .map(|trade| trade.domain_model(connection))
            .map_err(|error| {
                error!("Error approving trade: {:?}", error);
                error
            })?;
        Ok(trade)
    }

    pub fn update_opened_at(
        connection: &mut SqliteConnection,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let trade = diesel::update(trades::table)
            .filter(trades::id.eq(trade.id.to_string()))
            .set((trades::updated_at.eq(now), trades::opened_at.eq(now)))
            .get_result::<TradeSQLite>(connection)
            .map(|trade| trade.domain_model(connection))
            .map_err(|error| {
                error!("Error executing trade: {:?}", error);
                error
            })?;
        Ok(trade)
    }

    pub fn update_closed_at(
        connection: &mut SqliteConnection,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let trade = diesel::update(trades::table)
            .filter(trades::id.eq(trade.id.to_string()))
            .set((trades::updated_at.eq(now), trades::closed_at.eq(now)))
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
    currency: String,
    trading_vehicle_id: String,
    safety_stop_id: String,
    entry_id: String,
    account_id: String,
    approved_at: Option<NaiveDateTime>,
    rejected_at: Option<NaiveDateTime>,
    opened_at: Option<NaiveDateTime>,
    failed_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
    rejected_by_rule_id: Option<String>,
    overview_id: String,
}

impl TradeSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> Trade {
        let trading_vehicle = WorkerTradingVehicle::read(
            connection,
            Uuid::parse_str(&self.trading_vehicle_id).unwrap(),
        )
        .unwrap();
        let safety_stop =
            WorkerOrder::read(connection, Uuid::parse_str(&self.safety_stop_id).unwrap()).unwrap();
        let entry =
            WorkerOrder::read(connection, Uuid::parse_str(&self.entry_id).unwrap()).unwrap();
        let overview =
            WorkerTrade::read_overview(connection, Uuid::parse_str(&self.overview_id).unwrap())
                .unwrap();

        let targets =
            WorkerTarget::read_all(Uuid::parse_str(&self.id).unwrap(), connection).unwrap();

        Trade {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            trading_vehicle,
            category: TradeCategory::from_str(&self.category).unwrap(),
            currency: Currency::from_str(&self.currency).unwrap(),
            safety_stop,
            entry,
            exit_targets: targets,
            account_id: Uuid::parse_str(&self.account_id).unwrap(),
            approved_at: self.approved_at,
            rejected_at: self.rejected_at,
            opened_at: self.opened_at,
            failed_at: self.failed_at,
            closed_at: self.closed_at,
            rejected_by_rule_id: self
                .rejected_by_rule_id
                .map(|id| Uuid::parse_str(&id).unwrap()),
            overview,
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
    currency: String,
    trading_vehicle_id: String,
    safety_stop_id: String,
    entry_id: String,
    account_id: String,
    approved_at: Option<NaiveDateTime>,
    rejected_at: Option<NaiveDateTime>,
    opened_at: Option<NaiveDateTime>,
    failed_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
    rejected_by_rule_id: Option<String>,
    overview_id: String,
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trades_overviews)]
struct TradeOverviewSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    funding_id: String,
    capital_in_market_id: String,
    capital_out_market_id: String,
    taxed_id: String,
    total_performance_id: String,
}

impl TradeOverviewSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> TradeOverview {
        let funding =
            WorkerPrice::read(connection, Uuid::parse_str(&self.funding_id).unwrap()).unwrap();
        let capital_in_market = WorkerPrice::read(
            connection,
            Uuid::parse_str(&self.capital_in_market_id).unwrap(),
        )
        .unwrap();
        let capital_out_market = WorkerPrice::read(
            connection,
            Uuid::parse_str(&self.capital_out_market_id).unwrap(),
        )
        .unwrap();
        let taxed =
            WorkerPrice::read(connection, Uuid::parse_str(&self.taxed_id).unwrap()).unwrap();
        let total_performance = WorkerPrice::read(
            connection,
            Uuid::parse_str(&self.total_performance_id).unwrap(),
        )
        .unwrap();

        TradeOverview {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            funding,
            capital_in_market,
            capital_out_market,
            taxed,
            total_performance,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = trades_overviews)]
struct NewTradeOverview {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    funding_id: String,
    capital_in_market_id: String,
    capital_out_market_id: String,
    taxed_id: String,
    total_performance_id: String,
}

#[cfg(test)]
mod tests {}
