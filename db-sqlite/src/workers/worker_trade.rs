use crate::error::{ConversionError, IntoDomainModel};
use crate::schema::{trades, trades_balances};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::ClosedTradePerformance;
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
            thesis: draft.thesis.clone(),
            sector: draft.sector.clone(),
            asset_class: draft.asset_class.clone(),
            context: draft.context.clone(),
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

    pub fn read_trade_status(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<Status, Box<dyn Error>> {
        let status_string = trades::table
            .filter(trades::id.eq(id.to_string()))
            .select(trades::status)
            .first::<String>(connection)
            .map_err(|error| {
                error!("Error reading trade status: {:?}", error);
                error
            })?;

        Status::from_str(&status_string)
            .map_err(|_| ConversionError::new("status", "Failed to parse status").into())
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

    pub fn read_recent_closed_trade_performances(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
        cutoff: NaiveDateTime,
    ) -> Result<Vec<ClosedTradePerformance>, Box<dyn Error>> {
        // Avoid loading full trade graphs when only `(updated_at, total_performance)` is required.
        #[derive(Queryable)]
        struct Row {
            trade_id: String,
            total_performance: String,
        }

        let closed_target = Status::ClosedTarget.to_string();
        let closed_stop = Status::ClosedStopLoss.to_string();

        let account_id_string = account_id.to_string();
        let currency_string = currency.to_string();

        let rows = trades::table
            .inner_join(trades_balances::table.on(trades_balances::id.eq(trades::balance_id)))
            .filter(trades::deleted_at.is_null())
            .filter(trades_balances::deleted_at.is_null())
            .filter(trades::account_id.eq(&account_id_string))
            .filter(trades::currency.eq(&currency_string))
            .filter(trades::updated_at.ge(cutoff))
            .filter(trades::status.eq_any([closed_target, closed_stop]))
            .select((trades::id, trades_balances::total_performance))
            .order_by(trades::updated_at.desc())
            .load::<Row>(connection)
            .map_err(|error| {
                error!("Error reading closed trade performances: {:?}", error);
                error
            })?;

        let mut out: Vec<ClosedTradePerformance> = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(ClosedTradePerformance {
                trade_id: Uuid::parse_str(&row.trade_id)
                    .map_err(|_| ConversionError::new("trade_id", "Failed to parse trade id"))?,
                total_performance: Decimal::from_str(&row.total_performance).map_err(|_| {
                    ConversionError::new("total_performance", "Failed to parse performance")
                })?,
            });
        }
        Ok(out)
    }

    pub fn read_recent_closed_trade_performance_points(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
        cutoff: NaiveDateTime,
    ) -> Result<Vec<(NaiveDateTime, Decimal)>, Box<dyn Error>> {
        #[derive(Queryable)]
        struct Row {
            updated_at: NaiveDateTime,
            total_performance: String,
        }

        let closed_target = Status::ClosedTarget.to_string();
        let closed_stop = Status::ClosedStopLoss.to_string();

        let account_id_string = account_id.to_string();
        let currency_string = currency.to_string();

        let rows = trades::table
            .inner_join(trades_balances::table.on(trades_balances::id.eq(trades::balance_id)))
            .filter(trades::deleted_at.is_null())
            .filter(trades_balances::deleted_at.is_null())
            .filter(trades::account_id.eq(&account_id_string))
            .filter(trades::currency.eq(&currency_string))
            .filter(trades::updated_at.ge(cutoff))
            .filter(trades::status.eq_any([closed_target, closed_stop]))
            .select((trades::updated_at, trades_balances::total_performance))
            .order_by(trades::updated_at.asc())
            .load::<Row>(connection)
            .map_err(|error| {
                error!("Error reading closed trade performance points: {:?}", error);
                error
            })?;

        let mut out: Vec<(NaiveDateTime, Decimal)> = Vec::with_capacity(rows.len());
        for row in rows {
            out.push((
                row.updated_at,
                Decimal::from_str(&row.total_performance).map_err(|_| {
                    ConversionError::new("total_performance", "Failed to parse performance")
                })?,
            ));
        }
        Ok(out)
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
        let now = Utc::now().naive_utc();
        diesel::update(trades_balances::table)
            .filter(trades_balances::id.eq(&trade.balance.id.to_string()))
            .set((
                trades_balances::updated_at.eq(now),
                trades_balances::funding.eq(funding.to_string()),
                trades_balances::capital_in_market.eq(capital_in_market.to_string()),
                trades_balances::capital_out_market.eq(capital_out_market.to_string()),
                trades_balances::taxed.eq(taxed.to_string()),
                trades_balances::total_performance.eq(total_performance.to_string()),
            ))
            .execute(connection)
            .map_err(|error| {
                error!("Error updating balance: {:?}", error);
                error
            })?;

        let mut balance = trade.balance.clone();
        balance.updated_at = now;
        balance.funding = funding;
        balance.capital_in_market = capital_in_market;
        balance.capital_out_market = capital_out_market;
        balance.taxed = taxed;
        balance.total_performance = total_performance;
        Ok(balance)
    }

    pub fn update_trade_status(
        connection: &mut SqliteConnection,
        status: Status,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        diesel::update(trades::table)
            .filter(trades::id.eq(trade.id.to_string()))
            .set((
                trades::updated_at.eq(now),
                trades::status.eq(status.to_string()),
            ))
            .execute(connection)
            .map_err(|error| {
                error!("Error executing trade: {:?}", error);
                error
            })?;

        let mut updated = trade.clone();
        updated.updated_at = now;
        updated.status = status;
        Ok(updated)
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
    thesis: Option<String>,
    sector: Option<String>,
    asset_class: Option<String>,
    context: Option<String>,
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
                    format!("Failed to read trading vehicle: {e}"),
                )
            })?;

        let safety_stop_id = Uuid::parse_str(&self.safety_stop_id).map_err(|_| {
            ConversionError::new("safety_stop_id", "Failed to parse safety stop ID")
        })?;
        let safety_stop = WorkerOrder::read(connection, safety_stop_id).map_err(|e| {
            ConversionError::new(
                "safety_stop",
                format!("Failed to read safety stop order: {e}"),
            )
        })?;

        let entry_id = Uuid::parse_str(&self.entry_id)
            .map_err(|_| ConversionError::new("entry_id", "Failed to parse entry ID"))?;
        let entry = WorkerOrder::read(connection, entry_id).map_err(|e| {
            ConversionError::new("entry", format!("Failed to read entry order: {e}"))
        })?;

        let target_id = Uuid::parse_str(&self.target_id)
            .map_err(|_| ConversionError::new("target_id", "Failed to parse target ID"))?;
        let targets = WorkerOrder::read(connection, target_id).map_err(|e| {
            ConversionError::new("target", format!("Failed to read target order: {e}"))
        })?;

        let balance_id = Uuid::parse_str(&self.balance_id)
            .map_err(|_| ConversionError::new("balance_id", "Failed to parse balance ID"))?;
        let balance = WorkerTrade::read_balance(connection, balance_id).map_err(|e| {
            ConversionError::new("balance", format!("Failed to read trade balance: {e}"))
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
            thesis: self.thesis,
            sector: self.sector,
            asset_class: self.asset_class,
            context: self.context,
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
    thesis: Option<String>,
    sector: Option<String>,
    asset_class: Option<String>,
    context: Option<String>,
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
mod tests {
    use super::*;
    use crate::SqliteDatabase;
    use chrono::Duration;
    use diesel_migrations::*;
    use model::{
        Account, DatabaseFactory, Environment, OrderAction, OrderCategory, TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;
    use std::sync::{Arc, Mutex};

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_database_with_connection() -> (SqliteDatabase, Arc<Mutex<SqliteConnection>>) {
        let connection = Arc::new(Mutex::new(establish_connection()));
        (SqliteDatabase::new_from(connection.clone()), connection)
    }

    fn create_account(database: &SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create(name, name, Environment::Paper, dec!(0), dec!(0))
            .expect("account should be created")
    }

    fn create_trade(
        connection: &mut SqliteConnection,
        account: &Account,
        symbol: &str,
        currency: Currency,
    ) -> Trade {
        let vehicle = WorkerTradingVehicle::create(
            connection,
            symbol,
            Some(symbol),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .expect("trading vehicle should be created");
        let stop = WorkerOrder::create(
            connection,
            dec!(90),
            &currency,
            10,
            &OrderAction::Sell,
            &OrderCategory::Stop,
            &vehicle,
        )
        .expect("stop order should be created");
        let entry = WorkerOrder::create(
            connection,
            dec!(100),
            &currency,
            10,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &vehicle,
        )
        .expect("entry order should be created");
        let target = WorkerOrder::create(
            connection,
            dec!(120),
            &currency,
            10,
            &OrderAction::Sell,
            &OrderCategory::Limit,
            &vehicle,
        )
        .expect("target order should be created");
        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10,
            currency,
            category: TradeCategory::Long,
            thesis: Some("breakout continuation".to_string()),
            sector: Some("technology".to_string()),
            asset_class: Some("equity".to_string()),
            context: Some("daily trend".to_string()),
        };

        WorkerTrade::create(connection, draft, &stop, &entry, &target)
            .expect("trade should be created")
    }

    fn set_trade_state(
        connection: &mut SqliteConnection,
        trade: &Trade,
        status: Status,
        updated_at: NaiveDateTime,
        deleted_at: Option<NaiveDateTime>,
    ) {
        diesel::update(trades::table.filter(trades::id.eq(trade.id.to_string())))
            .set((
                trades::status.eq(status.to_string()),
                trades::updated_at.eq(updated_at),
                trades::deleted_at.eq(deleted_at),
            ))
            .execute(connection)
            .expect("trade state should be updated");
    }

    fn set_trade_performance(
        connection: &mut SqliteConnection,
        trade: &Trade,
        performance: Decimal,
        deleted_at: Option<NaiveDateTime>,
    ) {
        diesel::update(
            trades_balances::table.filter(trades_balances::id.eq(trade.balance.id.to_string())),
        )
        .set((
            trades_balances::total_performance.eq(performance.to_string()),
            trades_balances::deleted_at.eq(deleted_at),
        ))
        .execute(connection)
        .expect("trade performance should be updated");
    }

    fn assert_single_trade(trades: Vec<Trade>, trade_id: Uuid) {
        let mut trades = trades.iter();
        let trade = trades.next().expect("one trade should be returned");
        assert!(trades.next().is_none());
        assert_eq!(trade.id, trade_id);
    }

    fn trade_row(trade: &Trade) -> TradeSQLite {
        TradeSQLite {
            id: trade.id.to_string(),
            created_at: trade.created_at,
            updated_at: trade.updated_at,
            deleted_at: trade.deleted_at,
            category: trade.category.to_string(),
            status: trade.status.to_string(),
            currency: trade.currency.to_string(),
            trading_vehicle_id: trade.trading_vehicle.id.to_string(),
            safety_stop_id: trade.safety_stop.id.to_string(),
            entry_id: trade.entry.id.to_string(),
            target_id: trade.target.id.to_string(),
            account_id: trade.account_id.to_string(),
            balance_id: trade.balance.id.to_string(),
            thesis: trade.thesis.clone(),
            sector: trade.sector.clone(),
            asset_class: trade.asset_class.clone(),
            context: trade.context.clone(),
        }
    }

    fn assert_trade_conversion_error(
        connection: &Arc<Mutex<SqliteConnection>>,
        trade: &Trade,
        mutate: impl FnOnce(&mut TradeSQLite),
        field: &str,
    ) {
        let mut row = trade_row(trade);
        mutate(&mut row);
        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let err = row
            .try_into_domain_model(&mut connection)
            .expect_err("conversion should fail");
        assert!(err.to_string().contains(field));
    }

    fn balance_row() -> AccountBalanceSQLite {
        let now = Utc::now().naive_utc();
        AccountBalanceSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD.to_string(),
            funding: dec!(100).to_string(),
            capital_in_market: dec!(80).to_string(),
            capital_out_market: dec!(20).to_string(),
            taxed: dec!(5).to_string(),
            total_performance: dec!(15).to_string(),
        }
    }

    fn assert_balance_conversion_error(row: AccountBalanceSQLite, field: &str) {
        let err = TradeBalance::try_from(row).expect_err("conversion should fail");
        assert!(err.to_string().contains(field));
    }

    #[derive(Debug)]
    struct PerformanceWindow {
        now: NaiveDateTime,
        older: NaiveDateTime,
        cutoff: NaiveDateTime,
        too_old: NaiveDateTime,
    }

    #[derive(Debug)]
    struct TradeStateSpec {
        symbol: &'static str,
        currency: Currency,
        status: Status,
        updated_at: NaiveDateTime,
        performance: Decimal,
        deleted_at: Option<NaiveDateTime>,
    }

    fn performance_window() -> PerformanceWindow {
        let now = Utc::now().naive_utc();
        PerformanceWindow {
            now,
            older: now
                .checked_sub_signed(Duration::days(1))
                .expect("older date should be representable"),
            cutoff: now
                .checked_sub_signed(Duration::days(2))
                .expect("cutoff date should be representable"),
            too_old: now
                .checked_sub_signed(Duration::days(3))
                .expect("too old date should be representable"),
        }
    }

    fn create_trade_with_state(
        connection: &mut SqliteConnection,
        account: &Account,
        spec: TradeStateSpec,
    ) -> Trade {
        let trade = create_trade(connection, account, spec.symbol, spec.currency);
        set_trade_state(
            connection,
            &trade,
            spec.status,
            spec.updated_at,
            spec.deleted_at,
        );
        set_trade_performance(connection, &trade, spec.performance, None);
        trade
    }

    fn insert_excluded_performance_rows(
        connection: &mut SqliteConnection,
        account: &Account,
        other_account: &Account,
        window: &PerformanceWindow,
    ) {
        let _ = create_trade_with_state(
            connection,
            account,
            TradeStateSpec {
                symbol: "PERFTOOOLD",
                currency: Currency::USD,
                status: Status::ClosedTarget,
                updated_at: window.too_old,
                performance: dec!(100),
                deleted_at: None,
            },
        );
        let _ = create_trade_with_state(
            connection,
            account,
            TradeStateSpec {
                symbol: "PERFOPEN",
                currency: Currency::USD,
                status: Status::Filled,
                updated_at: window.now,
                performance: dec!(99),
                deleted_at: None,
            },
        );
        let _ = create_trade_with_state(
            connection,
            account,
            TradeStateSpec {
                symbol: "PERFEUR",
                currency: Currency::EUR,
                status: Status::ClosedTarget,
                updated_at: window.now,
                performance: dec!(98),
                deleted_at: None,
            },
        );
        let _ = create_trade_with_state(
            connection,
            other_account,
            TradeStateSpec {
                symbol: "PERFOTHER",
                currency: Currency::USD,
                status: Status::ClosedTarget,
                updated_at: window.now,
                performance: dec!(97),
                deleted_at: None,
            },
        );
        let _ = create_trade_with_state(
            connection,
            account,
            TradeStateSpec {
                symbol: "PERFDELETE",
                currency: Currency::USD,
                status: Status::ClosedTarget,
                updated_at: window.now,
                performance: dec!(96),
                deleted_at: Some(window.now),
            },
        );
    }

    fn assert_recent_performances(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        cutoff: NaiveDateTime,
        older_trade_id: Uuid,
        newer_trade_id: Uuid,
    ) {
        let performances = WorkerTrade::read_recent_closed_trade_performances(
            connection,
            account_id,
            &Currency::USD,
            cutoff,
        )
        .expect("closed trade performances should read");
        let mut performances = performances.iter();
        let newest = performances
            .next()
            .expect("newest performance should exist");
        let oldest = performances
            .next()
            .expect("oldest performance should exist");
        assert!(performances.next().is_none());
        assert_eq!(newest.trade_id, newer_trade_id);
        assert_eq!(newest.total_performance, dec!(25));
        assert_eq!(oldest.trade_id, older_trade_id);
        assert_eq!(oldest.total_performance, dec!(-7));
    }

    fn assert_recent_points(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        window: &PerformanceWindow,
    ) {
        let points = WorkerTrade::read_recent_closed_trade_performance_points(
            connection,
            account_id,
            &Currency::USD,
            window.cutoff,
        )
        .expect("closed trade performance points should read");
        assert_eq!(
            points,
            vec![(window.older, dec!(-7)), (window.now, dec!(25))]
        );
    }

    fn corrupt_total_performance(connection: &mut SqliteConnection, trade: &Trade) {
        diesel::update(
            trades_balances::table.filter(trades_balances::id.eq(trade.balance.id.to_string())),
        )
        .set(trades_balances::total_performance.eq("not-decimal"))
        .execute(connection)
        .expect("balance should be corrupted for conversion coverage");
    }

    #[test]
    fn create_read_update_and_status_queries_preserve_trade_graph() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-roundtrip-account");
        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let trade = create_trade(&mut connection, &account, "TRADEROUND", Currency::USD);
        assert_eq!(trade.status, Status::New);
        assert_eq!(trade.account_id, account.id);
        assert_eq!(trade.thesis.as_deref(), Some("breakout continuation"));

        let read = WorkerTrade::read_trade(&mut connection, trade.id).expect("trade should read");
        assert_eq!(read.id, trade.id);
        assert_eq!(read.trading_vehicle.symbol, "TRADEROUND");
        assert_eq!(
            WorkerTrade::read_trade_status(&mut connection, trade.id).expect("status should read"),
            Status::New
        );

        let funded = WorkerTrade::update_trade_status(&mut connection, Status::Funded, &trade)
            .expect("status should update");
        assert_eq!(funded.status, Status::Funded);
        assert_single_trade(
            WorkerTrade::read_all_funded_trades_for_currency(
                &mut connection,
                account.id,
                &Currency::USD,
            )
            .expect("funded trades should read"),
            trade.id,
        );
        assert_single_trade(
            WorkerTrade::read_all_trades_with_status(&mut connection, account.id, Status::Funded)
                .expect("status trades should read"),
            trade.id,
        );
        assert!(WorkerTrade::read_all_trades_with_status_currency(
            &mut connection,
            account.id,
            Status::Funded,
            &Currency::EUR,
        )
        .expect("currency-filtered trades should read")
        .is_empty());

        let balance = WorkerTrade::update_trade_balance(
            &mut connection,
            &funded,
            dec!(1000),
            dec!(900),
            dec!(100),
            dec!(30),
            dec!(75),
        )
        .expect("trade balance should update");
        let persisted =
            WorkerTrade::read_balance(&mut connection, balance.id).expect("balance should read");
        assert_eq!(persisted.funding, dec!(1000));
        assert_eq!(persisted.capital_in_market, dec!(900));
        assert_eq!(persisted.capital_out_market, dec!(100));
        assert_eq!(persisted.taxed, dec!(30));
        assert_eq!(persisted.total_performance, dec!(75));
    }

    #[test]
    fn debug_representation_is_stable() {
        assert_eq!(format!("{WorkerTrade:?}"), "WorkerTrade");
    }

    #[test]
    fn trade_worker_reports_missing_trade_table_errors() {
        let mut connection = establish_connection();
        diesel::sql_query("DROP TABLE trades")
            .execute(&mut connection)
            .expect("trades table should drop");
        let trade_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let cutoff = Utc::now().naive_utc();

        let read_error = WorkerTrade::read_trade(&mut connection, trade_id)
            .expect_err("missing table should fail trade read");
        assert!(read_error.to_string().contains("trades"));

        let status_error = WorkerTrade::read_trade_status(&mut connection, trade_id)
            .expect_err("missing table should fail trade status read");
        assert!(status_error.to_string().contains("trades"));

        let funded_error = WorkerTrade::read_all_funded_trades_for_currency(
            &mut connection,
            account_id,
            &Currency::USD,
        )
        .expect_err("missing table should fail funded trade read");
        assert!(funded_error.to_string().contains("trades"));

        let status_list_error =
            WorkerTrade::read_all_trades_with_status(&mut connection, account_id, Status::Funded)
                .expect_err("missing table should fail status trade read");
        assert!(status_list_error.to_string().contains("trades"));

        let status_currency_error = WorkerTrade::read_all_trades_with_status_currency(
            &mut connection,
            account_id,
            Status::Funded,
            &Currency::USD,
        )
        .expect_err("missing table should fail status/currency trade read");
        assert!(status_currency_error.to_string().contains("trades"));

        let performance_error = WorkerTrade::read_recent_closed_trade_performances(
            &mut connection,
            account_id,
            &Currency::USD,
            cutoff,
        )
        .expect_err("missing table should fail recent performance read");
        assert!(performance_error.to_string().contains("trades"));

        let point_error = WorkerTrade::read_recent_closed_trade_performance_points(
            &mut connection,
            account_id,
            &Currency::USD,
            cutoff,
        )
        .expect_err("missing table should fail recent performance point read");
        assert!(point_error.to_string().contains("trades"));
    }

    #[test]
    fn trade_worker_reports_missing_trade_balance_table_errors() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-balance-error-account");
        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let trade = create_trade(&mut connection, &account, "TRADEBALERR", Currency::USD);
        diesel::sql_query("DROP TABLE trades_balances")
            .execute(&mut *connection)
            .expect("trades_balances table should drop");

        let create_balance_error =
            WorkerTrade::create_balance(&mut connection, &Currency::USD, Utc::now().naive_utc())
                .expect_err("missing table should fail trade balance create");
        assert!(create_balance_error.to_string().contains("trades_balances"));

        let read_balance_error = WorkerTrade::read_balance(&mut connection, trade.balance.id)
            .expect_err("missing table should fail trade balance read");
        assert!(read_balance_error.to_string().contains("trades_balances"));

        let update_balance_error = WorkerTrade::update_trade_balance(
            &mut connection,
            &trade,
            dec!(100),
            dec!(80),
            dec!(20),
            dec!(5),
            dec!(15),
        )
        .expect_err("missing table should fail trade balance update");
        assert!(update_balance_error.to_string().contains("trades_balances"));
    }

    #[test]
    fn create_reports_trade_insert_errors_after_balance_create() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-create-error-account");
        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let trade = create_trade(&mut connection, &account, "TRADECRTERR", Currency::USD);
        diesel::sql_query("DROP TABLE trades")
            .execute(&mut *connection)
            .expect("trades table should drop");
        let draft = DraftTrade {
            account,
            trading_vehicle: trade.trading_vehicle.clone(),
            quantity: 10,
            currency: trade.currency,
            category: trade.category,
            thesis: trade.thesis.clone(),
            sector: trade.sector.clone(),
            asset_class: trade.asset_class.clone(),
            context: trade.context.clone(),
        };

        let update_error =
            WorkerTrade::update_trade_status(&mut connection, Status::Funded, &trade)
                .expect_err("missing table should fail trade status update");
        assert!(update_error.to_string().contains("trades"));

        let error = WorkerTrade::create(
            &mut connection,
            draft,
            &trade.safety_stop,
            &trade.entry,
            &trade.target,
        )
        .expect_err("missing table should fail trade insert");

        assert!(error.to_string().contains("trades"));
    }

    #[test]
    fn read_trade_status_reports_invalid_status_strings() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-status-error-account");
        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let trade = create_trade(&mut connection, &account, "TRADESTATERR", Currency::USD);
        diesel::sql_query("PRAGMA ignore_check_constraints = ON")
            .execute(&mut *connection)
            .expect("check constraints should be disabled for corruption test");
        diesel::update(trades::table.filter(trades::id.eq(trade.id.to_string())))
            .set(trades::status.eq("not-a-status"))
            .execute(&mut *connection)
            .expect("trade status should be corrupted");

        let error = WorkerTrade::read_trade_status(&mut connection, trade.id)
            .expect_err("corrupt status should fail");

        assert!(error.to_string().contains("status"));
    }

    #[test]
    fn recent_closed_performance_queries_filter_sort_and_validate_decimals() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-performance-account");
        let other_account = create_account(&database, "trade-performance-other");
        let window = performance_window();

        let mut connection = connection
            .lock()
            .expect("connection lock should be acquired");
        let older_closed = create_trade_with_state(
            &mut connection,
            &account,
            TradeStateSpec {
                symbol: "PERFOLD",
                currency: Currency::USD,
                status: Status::ClosedStopLoss,
                updated_at: window.older,
                performance: dec!(-7),
                deleted_at: None,
            },
        );
        let newer_closed = create_trade_with_state(
            &mut connection,
            &account,
            TradeStateSpec {
                symbol: "PERFNEW",
                currency: Currency::USD,
                status: Status::ClosedTarget,
                updated_at: window.now,
                performance: dec!(25),
                deleted_at: None,
            },
        );
        insert_excluded_performance_rows(&mut connection, &account, &other_account, &window);

        assert_recent_performances(
            &mut connection,
            account.id,
            window.cutoff,
            older_closed.id,
            newer_closed.id,
        );
        assert_recent_points(&mut connection, account.id, &window);

        corrupt_total_performance(&mut connection, &older_closed);
        let err = WorkerTrade::read_recent_closed_trade_performances(
            &mut connection,
            account.id,
            &Currency::USD,
            window.cutoff,
        )
        .expect_err("corrupt performance should fail");
        assert!(err.to_string().contains("total_performance"));

        let err = WorkerTrade::read_recent_closed_trade_performance_points(
            &mut connection,
            account.id,
            &Currency::USD,
            window.cutoff,
        )
        .expect_err("corrupt performance point should fail");
        assert!(err.to_string().contains("total_performance"));
    }

    #[test]
    fn trade_sqlite_conversion_reports_corrupt_fields() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-conversion-account");
        let trade = {
            let mut connection = connection
                .lock()
                .expect("connection lock should be acquired");
            create_trade(&mut connection, &account, "TRADECONVERT", Currency::USD)
        };

        assert_trade_conversion_error(&connection, &trade, |row| row.id = "bad".to_string(), "id");
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.trading_vehicle_id = "bad".to_string(),
            "trading_vehicle_id",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.safety_stop_id = "bad".to_string(),
            "safety_stop_id",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.entry_id = "bad".to_string(),
            "entry_id",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.target_id = "bad".to_string(),
            "target_id",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.balance_id = "bad".to_string(),
            "balance_id",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.category = "bad".to_string(),
            "category",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.status = "bad".to_string(),
            "status",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.currency = "bad".to_string(),
            "currency",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.account_id = "bad".to_string(),
            "account_id",
        );
    }

    #[test]
    fn trade_sqlite_conversion_reports_missing_related_rows() {
        let (database, connection) = create_database_with_connection();
        let account = create_account(&database, "trade-missing-related-account");
        let trade = {
            let mut connection = connection
                .lock()
                .expect("connection lock should be acquired");
            create_trade(&mut connection, &account, "TRADEMISSING", Currency::USD)
        };

        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.trading_vehicle_id = Uuid::new_v4().to_string(),
            "trading_vehicle",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.safety_stop_id = Uuid::new_v4().to_string(),
            "safety_stop",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.entry_id = Uuid::new_v4().to_string(),
            "entry",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.target_id = Uuid::new_v4().to_string(),
            "target",
        );
        assert_trade_conversion_error(
            &connection,
            &trade,
            |row| row.balance_id = Uuid::new_v4().to_string(),
            "balance",
        );
    }

    #[test]
    fn trade_balance_sqlite_conversion_reports_corrupt_fields() {
        let mut row = balance_row();
        row.id = "bad".to_string();
        assert_balance_conversion_error(row, "id");

        let mut row = balance_row();
        row.currency = "bad".to_string();
        assert_balance_conversion_error(row, "currency");

        let mut row = balance_row();
        row.funding = "bad".to_string();
        assert_balance_conversion_error(row, "funding");

        let mut row = balance_row();
        row.capital_in_market = "bad".to_string();
        assert_balance_conversion_error(row, "capital_in_market");

        let mut row = balance_row();
        row.capital_out_market = "bad".to_string();
        assert_balance_conversion_error(row, "capital_out_market");

        let mut row = balance_row();
        row.taxed = "bad".to_string();
        assert_balance_conversion_error(row, "taxed");

        let mut row = balance_row();
        row.total_performance = "bad".to_string();
        assert_balance_conversion_error(row, "total_performance");
    }
}
