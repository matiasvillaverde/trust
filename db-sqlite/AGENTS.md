# SQLite crate guide

This file extends the repository `AGENTS.md` for `db-sqlite/`.

## Structure

`SqliteDatabase` implements `model::DatabaseFactory`. Specialized readers/writers share one `Arc<Mutex<SqliteConnection>>`.

- Wire factory trait accessors in `src/database.rs`.
- Put Diesel row types and CRUD/query logic in the matching `src/workers/*.rs` module, registered by `src/workers.rs`.
- Keep ordinary failures as `Result`; do not introduce production panics.

## Persistence invariants

- Persisted decimals and quantities are string-backed. Convert to/from `Decimal` without floating point and preserve checked arithmetic.
- Normal active reads should filter `deleted_at IS NULL`; backup/restore intentionally includes soft-deleted rows.
- Preserve foreign-key enforcement and connection configuration.
- Use Diesel transactions or validated named savepoints for atomic multi-write operations. Savepoint names allow only nonempty ASCII alphanumeric/underscore values.
- Add rollback tests when changing transfers, distributions, trade accounting, or any other multi-write operation.

## Migrations and backups

- Never hand-edit `src/schema.rs`.
- Add paired `up.sql` and `down.sql` files under a timestamped directory in `migrations/`. SQLite table rebuilds must preserve foreign keys; extend `src/migration_fk_safety_tests.rs` when referenced tables change.
- `build.rs` tracks migration SQL. Use the repository setup/migration workflow; do not assume an existing database is migrated merely by opening it.
- When persisted tables change, audit `src/backup.rs` end to end: versioned envelope rows, export, validation, compatibility defaults, insertion/clear ordering, strict/replace behavior, size limits, and tests. Do not claim a backup covers every table unless verified.

## Tests

Use `SqliteDatabase::new_in_memory()` for isolated tests. DB tests should be serialized.

```bash
cargo test -p db-sqlite -- --test-threads=1
cargo test -p db-sqlite migration_fk_safety_tests -- --test-threads=1
```
