#![allow(clippy::indexing_slicing)]

use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Integer, Text};

#[derive(QueryableByName, Debug)]
struct ForeignKeyListRow {
    #[diesel(sql_type = Integer)]
    #[allow(dead_code)]
    id: i32,
    #[diesel(sql_type = Integer)]
    #[allow(dead_code)]
    seq: i32,
    #[diesel(sql_type = Text)]
    table: String,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    from: String,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    to: String,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    on_update: String,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    on_delete: String,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    r#match: String,
}

#[derive(QueryableByName, Debug)]
struct ForeignKeyCheckRow {
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    table: String,
    #[diesel(sql_type = Integer)]
    #[allow(dead_code)]
    rowid: i32,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    parent: String,
    #[diesel(sql_type = Integer)]
    #[allow(dead_code)]
    fkid: i32,
}

fn exec_script(conn: &mut SqliteConnection, script: &str) {
    // Very small SQL "script runner": split on ';', drop line comments, and execute.
    // This is good enough for our migrations which are plain statements.
    let mut buf = String::new();
    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--") || trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }

    for stmt in buf.split(';') {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        sql_query(s).execute(conn).unwrap();
    }
}

#[test]
fn trading_vehicles_migration_does_not_rewrite_dependent_foreign_keys() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    sql_query("PRAGMA foreign_keys=ON;")
        .execute(&mut conn)
        .unwrap();

    // Minimal pre-migration schema: a referenced `trading_vehicles` table and a dependent table
    // that references it via FK. This reproduces the SQLite behavior where renaming a referenced
    // table can rewrite dependent FK definitions.
    exec_script(
        &mut conn,
        r#"
        CREATE TABLE trading_vehicles (
            id          TEXT NOT NULL PRIMARY KEY,
            created_at  DATETIME NOT NULL,
            updated_at  DATETIME NOT NULL,
            deleted_at  DATETIME,
            symbol      TEXT NOT NULL,
            isin        TEXT NOT NULL UNIQUE,
            category    TEXT CHECK(category IN ('crypto', 'fiat', 'stock')) NOT NULL,
            broker      TEXT NOT NULL
        );

        CREATE TABLE orders (
            id TEXT NOT NULL PRIMARY KEY,
            trading_vehicle_id TEXT NOT NULL REFERENCES trading_vehicles(id)
        );

        INSERT INTO trading_vehicles (id, created_at, updated_at, deleted_at, symbol, isin, category, broker)
        VALUES ('tv1', '2020-01-01', '2020-01-01', NULL, 'AAPL', 'US0378331005', 'stock', 'alpaca');

        INSERT INTO orders (id, trading_vehicle_id) VALUES ('o1', 'tv1');
        "#,
    );

    let migration_sql = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/2026-02-12-150000_enhance_trading_vehicles/up.sql"
    ));
    exec_script(&mut conn, migration_sql);

    // Ensure FK on orders still points to `trading_vehicles` (and not `trading_vehicles_old`).
    let fks: Vec<ForeignKeyListRow> = sql_query("PRAGMA foreign_key_list('orders')")
        .load(&mut conn)
        .unwrap();
    assert_eq!(fks.len(), 1);
    assert_eq!(fks[0].table, "trading_vehicles");

    // Ensure all FK constraints are valid.
    let fk_check: Vec<ForeignKeyCheckRow> = sql_query("PRAGMA foreign_key_check")
        .load(&mut conn)
        .unwrap();
    assert!(
        fk_check.is_empty(),
        "foreign_key_check must be empty, got: {fk_check:?}"
    );
}

#[test]
fn trading_vehicles_migration_allows_same_isin_across_brokers() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    sql_query("PRAGMA foreign_keys=ON;")
        .execute(&mut conn)
        .unwrap();

    exec_script(
        &mut conn,
        r#"
        CREATE TABLE trading_vehicles (
            id          TEXT NOT NULL PRIMARY KEY,
            created_at  DATETIME NOT NULL,
            updated_at  DATETIME NOT NULL,
            deleted_at  DATETIME,
            symbol      TEXT NOT NULL,
            isin        TEXT NOT NULL UNIQUE,
            category    TEXT CHECK(category IN ('crypto', 'fiat', 'stock')) NOT NULL,
            broker      TEXT NOT NULL
        );

        INSERT INTO trading_vehicles (id, created_at, updated_at, deleted_at, symbol, isin, category, broker)
        VALUES ('tv1', '2020-01-01', '2020-01-01', NULL, 'AAPL', 'US0378331005', 'stock', 'alpaca');
        "#,
    );

    let migration_sql = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/2026-02-12-150000_enhance_trading_vehicles/up.sql"
    ));
    exec_script(&mut conn, migration_sql);

    // Same ISIN across a different broker should now be allowed because identity is (broker, symbol).
    sql_query(
        "INSERT INTO trading_vehicles (
            id, created_at, updated_at, deleted_at, symbol, isin, category, broker
        ) VALUES (
            'tv2', '2020-01-01', '2020-01-01', NULL, 'AAPL', 'US0378331005', 'stock', 'ibkr'
        )",
    )
    .execute(&mut conn)
    .unwrap();
}
