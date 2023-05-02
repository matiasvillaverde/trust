use crate::schema::account_overviews;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::SqliteConnection;
use rust_decimal_macros::dec;
use std::error::Error;
use tracing::error;
use trust_model::{Account, AccountOverview, Currency, Price};
use uuid::Uuid;

use super::worker_price::WorkerPrice;

pub struct WorkerAccountOverview;

impl WorkerAccountOverview {
    fn new_account_overview(
        connection: &mut SqliteConnection,
        account: &Account,
        currency: Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let total_balance = WorkerPrice::new(connection, &currency, dec!(0))?;
        let total_in_trade = WorkerPrice::new(connection, &currency, dec!(0))?;
        let total_available = WorkerPrice::new(connection, &currency, dec!(0))?;
        let total_taxable = WorkerPrice::new(connection, &currency, dec!(0))?;

        let new_account_overview = NewAccountOverview {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: account.id.to_string(),
            total_balance_id: total_balance.id.to_string(),
            total_in_trade_id: total_in_trade.id.to_string(),
            total_available_id: total_available.id.to_string(),
            total_taxable_id: total_taxable.id.to_string(),
            currency: currency.to_string(),
        };

        let overview = diesel::insert_into(account_overviews::table)
            .values(&new_account_overview)
            .get_result::<AccountOverviewSQLite>(connection)
            .map(|overview| overview.domain_model(connection))
            .map_err(|error| {
                error!("Error creating overview: {:?}", error);
                error
            })?;
        Ok(overview)
    }

    fn update_account_overview(
        &mut self,
        account: &Account,
        overview: &AccountOverview,
    ) -> Result<(), Box<dyn Error>> {
        unimplemented!()
    }
    fn read_account_overview(
        &mut self,
        account: &Account,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        unimplemented!()
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = account_overviews)]
#[diesel(treat_none_as_null = true)]
struct AccountOverviewSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    total_balance_id: String,
    total_in_trade_id: String,
    total_available_id: String,
    total_taxable_id: String,
    currency: String,
}

impl AccountOverviewSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> AccountOverview {
        use std::str::FromStr;
        AccountOverview {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            account_id: Uuid::parse_str(&self.account_id).unwrap(),
            total_balance: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_balance_id).unwrap(),
            )
            .unwrap(),
            total_in_trade: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_in_trade_id).unwrap(),
            )
            .unwrap(),
            total_available: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_available_id).unwrap(),
            )
            .unwrap(),
            total_taxable: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_taxable_id).unwrap(),
            )
            .unwrap(),
            currency: Currency::from_str(&self.currency).unwrap(),
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = account_overviews)]
pub struct NewAccountOverview {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    total_balance_id: String,
    total_in_trade_id: String,
    total_available_id: String,
    total_taxable_id: String,
    currency: String,
}
