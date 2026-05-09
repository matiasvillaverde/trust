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

fn assert_orders_fk_still_points_to_trading_vehicles(conn: &mut SqliteConnection) {
    let fks: Vec<ForeignKeyListRow> = sql_query("PRAGMA foreign_key_list('orders')")
        .load(conn)
        .unwrap();
    assert_eq!(fks.len(), 1);
    assert_eq!(fks[0].table, "trading_vehicles");

    let fk_check: Vec<ForeignKeyCheckRow> =
        sql_query("PRAGMA foreign_key_check").load(conn).unwrap();
    assert!(
        fk_check.is_empty(),
        "foreign_key_check must be empty, got: {fk_check:?}"
    );
}

fn create_enhanced_trading_vehicle_schema(conn: &mut SqliteConnection, category_check: &str) {
    exec_script(
        conn,
        &format!(
            r#"
            CREATE TABLE trading_vehicles (
                id              TEXT NOT NULL PRIMARY KEY,
                created_at      DATETIME NOT NULL,
                updated_at      DATETIME NOT NULL,
                deleted_at      DATETIME,
                symbol          TEXT NOT NULL,
                isin            TEXT,
                category        TEXT CHECK(category IN ({category_check})) NOT NULL,
                broker          TEXT NOT NULL,
                broker_asset_id     TEXT,
                exchange            TEXT,
                broker_asset_class  TEXT,
                broker_asset_status TEXT,
                tradable            BOOLEAN,
                marginable          BOOLEAN,
                shortable           BOOLEAN,
                easy_to_borrow      BOOLEAN,
                fractionable        BOOLEAN
            );

            CREATE UNIQUE INDEX trading_vehicles_broker_symbol_unique
            ON trading_vehicles (broker, symbol);

            CREATE UNIQUE INDEX trading_vehicles_broker_asset_id_unique
            ON trading_vehicles (broker, broker_asset_id)
            WHERE broker_asset_id IS NOT NULL;

            CREATE TABLE orders (
                id TEXT NOT NULL PRIMARY KEY,
                trading_vehicle_id TEXT NOT NULL REFERENCES trading_vehicles(id)
            );
            "#
        ),
    );
}

fn insert_minimal_trading_vehicle(
    conn: &mut SqliteConnection,
    id: &str,
    symbol: &str,
    category: &str,
) {
    sql_query(
        "INSERT INTO trading_vehicles (
            id, created_at, updated_at, deleted_at, symbol, isin, category, broker
        ) VALUES (
            ?, '2020-01-01', '2020-01-01', NULL, ?, NULL, ?, 'ibkr'
        )",
    )
    .bind::<Text, _>(id)
    .bind::<Text, _>(symbol)
    .bind::<Text, _>(category)
    .execute(conn)
    .unwrap();
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
    assert_orders_fk_still_points_to_trading_vehicles(&mut conn);
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

#[test]
fn multi_asset_category_migration_preserves_foreign_keys_and_accepts_new_categories() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    sql_query("PRAGMA foreign_keys=ON;")
        .execute(&mut conn)
        .unwrap();

    create_enhanced_trading_vehicle_schema(&mut conn, "'crypto', 'fiat', 'stock'");
    insert_minimal_trading_vehicle(&mut conn, "tv1", "AAPL", "stock");
    sql_query("INSERT INTO orders (id, trading_vehicle_id) VALUES ('o1', 'tv1')")
        .execute(&mut conn)
        .unwrap();

    let migration_sql = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/2026-05-06-120000_expand_trading_vehicle_categories/up.sql"
    ));
    exec_script(&mut conn, migration_sql);

    assert_orders_fk_still_points_to_trading_vehicles(&mut conn);
    insert_minimal_trading_vehicle(&mut conn, "tv2", "SPY", "etf");
    insert_minimal_trading_vehicle(&mut conn, "tv3", "9128285M8", "bond");
}

#[derive(QueryableByName, Debug)]
struct CategoryRow {
    #[diesel(sql_type = Text)]
    category: String,
}

#[test]
fn multi_asset_category_down_migration_maps_etf_and_bond_to_stock() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    sql_query("PRAGMA foreign_keys=ON;")
        .execute(&mut conn)
        .unwrap();

    create_enhanced_trading_vehicle_schema(&mut conn, "'crypto', 'fiat', 'stock', 'etf', 'bond'");
    insert_minimal_trading_vehicle(&mut conn, "tv1", "SPY", "etf");
    insert_minimal_trading_vehicle(&mut conn, "tv2", "9128285M8", "bond");
    sql_query("INSERT INTO orders (id, trading_vehicle_id) VALUES ('o1', 'tv1')")
        .execute(&mut conn)
        .unwrap();

    let migration_sql = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/2026-05-06-120000_expand_trading_vehicle_categories/down.sql"
    ));
    exec_script(&mut conn, migration_sql);

    assert_orders_fk_still_points_to_trading_vehicles(&mut conn);

    let categories: Vec<CategoryRow> =
        sql_query("SELECT category FROM trading_vehicles ORDER BY id")
            .load(&mut conn)
            .unwrap();
    assert!(categories.iter().all(|row| row.category == "stock"));

    assert!(sql_query(
        "INSERT INTO trading_vehicles (
            id, created_at, updated_at, deleted_at, symbol, isin, category, broker
        ) VALUES (
            'tv3', '2020-01-01', '2020-01-01', NULL, 'TLT', NULL, 'etf', 'ibkr'
        )"
    )
    .execute(&mut conn)
    .is_err());
}

#[test]
fn fixed_income_terms_migration_adds_and_removes_bond_columns() {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    sql_query("PRAGMA foreign_keys=ON;")
        .execute(&mut conn)
        .unwrap();

    create_enhanced_trading_vehicle_schema(&mut conn, "'crypto', 'fiat', 'stock', 'etf', 'bond'");
    insert_minimal_trading_vehicle(&mut conn, "tv1", "9128285M8", "bond");
    sql_query("INSERT INTO orders (id, trading_vehicle_id) VALUES ('o1', 'tv1')")
        .execute(&mut conn)
        .unwrap();

    let up_sql = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/2026-05-07-120000_add_fixed_income_terms_to_trading_vehicles/up.sql"
    ));
    exec_script(&mut conn, up_sql);

    sql_query(
        "UPDATE trading_vehicles
        SET fixed_income_face_value = '1000',
            fixed_income_coupon_rate_pct = '4.25',
            fixed_income_maturity_date = '2030-12-31',
            fixed_income_coupon_frequency_per_year = 2
        WHERE id = 'tv1'",
    )
    .execute(&mut conn)
    .unwrap();

    assert_orders_fk_still_points_to_trading_vehicles(&mut conn);

    let down_sql = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/2026-05-07-120000_add_fixed_income_terms_to_trading_vehicles/down.sql"
    ));
    exec_script(&mut conn, down_sql);

    assert_orders_fk_still_points_to_trading_vehicles(&mut conn);
    assert!(sql_query(
        "UPDATE trading_vehicles
        SET fixed_income_face_value = '1000'
        WHERE id = 'tv1'"
    )
    .execute(&mut conn)
    .is_err());
}
