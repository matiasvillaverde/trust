//! One balance lookup after a TEMP schema-version check. Referencing main
//! accounts establishes the same database snapshot used by data_version; TEMP
//! state alone is insufficient. Schema guards also cover rolled-back TEMP DDL.
use super::schema;
use diesel::{
    prelude::*,
    query_builder::{AstPass, Query, QueryFragment, QueryId},
    sql_types::{BigInt, Nullable, Text},
    sqlite::Sqlite,
};
use rust_decimal::Decimal;
use std::error::Error;

struct VerifiedCash<'a> {
    account: &'a str,
    currency: &'a str,
    schema_version: i64,
}

impl QueryId for VerifiedCash<'_> {
    type QueryId = VerifiedCash<'static>;
    const HAS_STATIC_QUERY_ID: bool = true;
}

impl Query for VerifiedCash<'_> {
    type SqlType = Nullable<Text>;
}

impl RunQueryDsl<SqliteConnection> for VerifiedCash<'_> {}

impl QueryFragment<Sqlite> for VerifiedCash<'_> {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Sqlite>) -> QueryResult<()> {
        out.push_sql(
            "SELECT cash.total FROM main.accounts AS account
      CROSS JOIN temp.trust_verified_cash AS cash
      WHERE account.id = ",
        );
        out.push_bind_param::<Text, _>(&self.account)?;
        out.push_sql(" AND cash.account_id = account.id AND cash.currency = ");
        out.push_bind_param::<Text, _>(&self.currency)?;
        out.push_sql(
            "
        AND cash.data_version = (SELECT data_version FROM pragma_data_version)
        AND cash.pending_id IS NULL
        AND (SELECT temp_store FROM pragma_temp_store) = 2
        AND NOT sqlite_compileoption_used('TEMP_STORE=0')
        AND (SELECT query_only FROM pragma_query_only) = 0
        AND cash.schema_version = ",
        );
        out.push_bind_param::<BigInt, _>(&self.schema_version)?;
        Ok(())
    }
}

pub(super) fn read(
    conn: &mut SqliteConnection,
    account: &str,
    currency: &str,
) -> Result<Option<Decimal>, Box<dyn Error>> {
    // A new connection or temp_store change may have no TEMP table. The full
    // verification path below is also authoritative for all other SQL failures;
    // none are turned into an available balance.
    let schema_version = schema::version(conn)?;
    let cached = match (VerifiedCash {
        account,
        currency,
        schema_version,
    })
    .get_result::<Option<String>>(conn)
    .optional()
    {
        Ok(cached) => cached.flatten(),
        Err(_) => return Ok(None),
    };
    cached
        .map(|value| Decimal::from_str_exact(&value).map_err(Into::into))
        .transpose()
}
