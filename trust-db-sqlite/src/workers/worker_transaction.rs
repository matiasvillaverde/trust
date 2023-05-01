// use rust_decimal::Decimal;
// use uuid::Uuid;
// use trust_model::{Currency, TransactionCategory};
// use trust_model::Account;
// use diesel::prelude::*;
// use std::error::Error;
// use tracing::error;
// use chrono::{NaiveDateTime, Utc};

// pub struct WorkerTransaction;

// impl WorkerTransaction {
//     pub fn create_transaction(
//         connection: &SqliteConnection,
//         account_id: Uuid,
//         amount: Decimal,
//         currency: Currency,
//         category: TransactionCategory,
//         trade: Option<&Trade>,
//     ) {
//         let now = Utc::now().naive_utc();

//         let new_transaction = NewTransaction {
//             id: Uuid::new_v4().to_string(),
//             created_at: now,
//             updated_at: now,
//             deleted_at: None,
//             account_id: account_id.to_string(),
//             amount: amount.to_string(),
//             currency: currency.to_string(),
//             category: category.to_string(),
//             trade_id: trade.map(|trade| trade.id.to_string()),
//         };

//         diesel::insert_into(transactions::table)
//             .values(&new_transaction)
//             .execute(connection)
//             .map_err(|error| {
//                 error!("Error creating transaction: {:?}", error);
//                 error
//             })
//             .unwrap();
//     }
// }
