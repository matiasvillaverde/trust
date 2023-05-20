use crate::schema::{trades, trades_lifecycle, trades_overviews};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use rust_decimal_macros::dec;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use trust_model::{Account, Currency};
use trust_model::{Order, Trade, TradeCategory, TradeLifecycle, TradeOverview, TradingVehicle};
use uuid::Uuid;

use super::{WorkerAccount, WorkerOrder, WorkerPrice, WorkerTradingVehicle};
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

        let lifecycle = WorkerTrade::create_lifecycle(connection, now)?;
        let overview = WorkerTrade::create_overview(connection, currency, now)?;

        let new_trade = NewTrade {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            category: category.to_string(),
            trading_vehicle_id: trading_vehicle.id.to_string(),
            safety_stop_id: safety_stop.id.to_string(),
            entry_id: entry.id.to_string(),
            account_id: account.id.to_string(),
            lifecycle_id: lifecycle.id.to_string(),
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

    pub fn read_lifecycle(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<TradeLifecycle, diesel::result::Error> {
        trades_lifecycle::table
            .filter(trades_lifecycle::id.eq(&id.to_string()))
            .first(connection)
            .map(|lifecycle: TradeLifecycleSQLite| lifecycle.domain_model(connection))
    }

    fn create_lifecycle(
        connection: &mut SqliteConnection,
        created_at: NaiveDateTime,
    ) -> Result<TradeLifecycle, Box<dyn Error>> {
        let new_trade_lifecycle = NewTradeLifecycle {
            id: Uuid::new_v4().to_string(),
            created_at: created_at,
            updated_at: created_at,
            deleted_at: None,
            approved_at: None,
            rejected_at: None,
            executed_at: None,
            failed_at: None,
            closed_at: None,
            rejected_by_rule_id: None,
        };

        let lifecycle = diesel::insert_into(trades_lifecycle::table)
            .values(&new_trade_lifecycle)
            .get_result::<TradeLifecycleSQLite>(connection)
            .map(|lifecycle| lifecycle.domain_model(connection))
            .map_err(|error| {
                error!("Error creating trade lifecycle: {:?}", error);
                error
            })?;
        Ok(lifecycle)
    }

    fn create_overview(
        connection: &mut SqliteConnection,
        currency: &Currency,
        created_at: NaiveDateTime,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        let total_input_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let total_in_market_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let total_out_market_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let total_taxable_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();
        let total_performance_id = WorkerPrice::create(connection, currency, dec!(0))?
            .id
            .to_string();

        let new_trade_overview = NewTradeOverview {
            id: Uuid::new_v4().to_string(),
            created_at: created_at,
            updated_at: created_at,
            deleted_at: None,
            total_input_id: total_input_id,
            total_in_market_id: total_in_market_id,
            total_out_market_id: total_out_market_id,
            total_taxable_id: total_taxable_id,
            total_performance_id: total_performance_id,
            currency: currency.to_string(),
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
    trading_vehicle_id: String,
    safety_stop_id: String,
    entry_id: String,
    account_id: String,
    lifecycle_id: String,
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
        let lifecycle =
            WorkerTrade::read_lifecycle(connection, Uuid::parse_str(&self.lifecycle_id).unwrap())
                .unwrap();
        let overview =
            WorkerTrade::read_overview(connection, Uuid::parse_str(&self.overview_id).unwrap())
                .unwrap();

        Trade {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            trading_vehicle,
            category: TradeCategory::from_str(&self.category).unwrap(),
            safety_stop: safety_stop,
            entry: entry,
            exit_targets: vec![], // TODO: read exit targets
            account_id: Uuid::parse_str(&self.account_id).unwrap(),
            lifecycle,
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
    trading_vehicle_id: String,
    safety_stop_id: String,
    entry_id: String,
    account_id: String,
    lifecycle_id: String,
    overview_id: String,
}

// Lifecycle

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trades_lifecycle)]
struct TradeLifecycleSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    approved_at: Option<NaiveDateTime>,
    rejected_at: Option<NaiveDateTime>,
    executed_at: Option<NaiveDateTime>,
    failed_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
    rejected_by_rule_id: Option<String>,
}

impl TradeLifecycleSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> TradeLifecycle {
        TradeLifecycle {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            approved_at: self.approved_at,
            rejected_at: self.rejected_at,
            executed_at: self.executed_at,
            failed_at: self.failed_at,
            closed_at: self.closed_at,
            rejected_by_rule_id: self
                .rejected_by_rule_id
                .map(|id| Uuid::parse_str(&id).unwrap()),
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = trades_lifecycle)]
struct NewTradeLifecycle {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    approved_at: Option<NaiveDateTime>,
    rejected_at: Option<NaiveDateTime>,
    executed_at: Option<NaiveDateTime>,
    failed_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
    rejected_by_rule_id: Option<String>,
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trades_overviews)]
struct TradeOverviewSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    total_input_id: String,
    total_in_market_id: String,
    total_out_market_id: String,
    total_taxable_id: String,
    total_performance_id: String,
    currency: String,
}

impl TradeOverviewSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> TradeOverview {
        let total_input =
            WorkerPrice::read(connection, Uuid::parse_str(&self.total_input_id).unwrap()).unwrap();
        let total_in_market = WorkerPrice::read(
            connection,
            Uuid::parse_str(&self.total_in_market_id).unwrap(),
        )
        .unwrap();
        let total_out_market = WorkerPrice::read(
            connection,
            Uuid::parse_str(&self.total_out_market_id).unwrap(),
        )
        .unwrap();
        let total_taxable =
            WorkerPrice::read(connection, Uuid::parse_str(&self.total_taxable_id).unwrap())
                .unwrap();
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
            total_input,
            total_in_market,
            total_out_market,
            total_taxable,
            total_performance,
            currency: Currency::from_str(&self.currency).unwrap(),
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
    total_input_id: String,
    total_in_market_id: String,
    total_out_market_id: String,
    total_taxable_id: String,
    total_performance_id: String,
    currency: String,
}

#[cfg(test)]
mod tests {}
