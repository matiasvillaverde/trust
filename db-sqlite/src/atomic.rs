//! Worker transactions may run inside the facade's named SQLite savepoints.
use diesel::{connection::SimpleConnection, SqliteConnection};
use std::error::Error;

pub(crate) fn savepoint<T>(
    connection: &mut SqliteConnection,
    operation: impl FnOnce(&mut SqliteConnection) -> Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    // Diesel cannot see a transaction begun by a raw named SAVEPOINT. Use a unique
    // nested savepoint too, avoiding BEGIN inside an existing SQLite transaction.
    let name = format!("trust_worker_{}", uuid::Uuid::new_v4().simple());
    connection.batch_execute(&format!("SAVEPOINT {name}"))?;
    let result = operation(connection).and_then(|value| {
        connection.batch_execute(&format!("RELEASE SAVEPOINT {name}"))?;
        Ok(value)
    });
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let rollback = connection.batch_execute(&format!("ROLLBACK TO SAVEPOINT {name}"));
            let release = connection.batch_execute(&format!("RELEASE SAVEPOINT {name}"));
            if rollback.is_err() || release.is_err() {
                return Err("Worker transaction failed and rollback could not be confirmed".into());
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::{prelude::*, sql_types::BigInt};
    #[derive(QueryableByName)]
    struct Count {
        #[diesel(sql_type=BigInt)]
        count: i64,
    }

    #[test]
    fn worker_savepoints_preserve_outer_atomicity_without_a_diesel_test_transaction() {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.batch_execute("CREATE TABLE evidence (id INTEGER PRIMARY KEY); SAVEPOINT facade;")
            .unwrap();
        savepoint(&mut conn, |conn| {
            conn.batch_execute("INSERT INTO evidence VALUES (1)")?;
            Ok(())
        })
        .unwrap();
        let result: Result<(), Box<dyn Error>> = savepoint(&mut conn, |conn| {
            conn.batch_execute("INSERT INTO evidence VALUES (2); INSERT INTO evidence VALUES (1)")?;
            Ok(())
        });
        assert!(result.is_err());
        let count: Count = diesel::sql_query("SELECT count(*) AS count FROM evidence")
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(count.count, 1);
        conn.batch_execute("ROLLBACK TO facade; RELEASE facade")
            .unwrap();
        let count: Count = diesel::sql_query("SELECT count(*) AS count FROM evidence")
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(count.count, 0);
        savepoint(&mut conn, |conn| {
            conn.batch_execute("INSERT INTO evidence VALUES (3)")?;
            Ok(())
        })
        .unwrap();
        let count: Count = diesel::sql_query("SELECT count(*) AS count FROM evidence")
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(count.count, 1);
    }
}
