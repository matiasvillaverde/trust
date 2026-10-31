#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::Connection;
use serde_json::json;
use std::collections::BTreeSet;
use trust_service::{
    auth::{Grant, Identity, Role, SCOPES},
    error::ServiceError,
    journal::{Claim, Journal, Reply},
};
use uuid::Uuid;

struct Fixture {
    directory: tempfile::TempDir,
    journal: Journal,
    db: Connection,
    identity: Identity,
    account: Uuid,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("control.db");
        let journal = Journal::open(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        let scopes: BTreeSet<String> = SCOPES.iter().map(|s| (*s).into()).collect();
        let until = u64::try_from(chrono::Utc::now().timestamp()).unwrap() + 600;
        Self {
            directory,
            journal,
            db,
            identity: Identity {
                subject: "journal-test-owner".into(),
                client_id: None,
                expires_at: until,
                owner_authenticated_until: Some(until),
                scopes: scopes.clone(),
                grant: Grant {
                    role: Role::Owner,
                    accounts: BTreeSet::new(),
                    all_accounts: true,
                    scopes,
                    allow_live: false,
                },
            },
            account: Uuid::new_v4(),
        }
    }

    fn claim(&mut self, key: &str) -> Result<Claim, ServiceError> {
        self.journal.claim(
            &self.identity,
            key,
            "capital_deposit",
            Some(self.account),
            &json!({"amount":"1"}),
            Uuid::new_v4(),
            &[self.account].into(),
        )
    }

    fn pending(&mut self, key: &str) -> Uuid {
        let Claim::New(id) = self.claim(key).unwrap() else {
            panic!("expected a fresh intent")
        };
        id
    }

    fn finish(&mut self, id: Uuid, unknown: bool) {
        let result = if unknown {
            Err(ServiceError::new(
                "outcome_unknown",
                "Injected lost broker response.",
                "Reconcile before recovery.",
            ))
        } else {
            Ok(json!({"completed":true}))
        };
        self.journal
            .finish(
                &self.identity,
                "capital_deposit",
                Some(self.account),
                &Reply::from_result(Uuid::new_v4(), Some(id), result),
            )
            .unwrap();
    }
}

#[test]
fn malformed_pending_metadata_aborts_restart_and_rolls_back_recovery() {
    for fault in [
        "UPDATE operations SET id='invalid-uuid' WHERE request_key='second'",
        "UPDATE operations SET account_id='invalid-uuid' WHERE request_key='second'",
        "UPDATE operations SET account_ids='not-json' WHERE request_key='second'",
        "UPDATE operations SET tool=x'FF' WHERE request_key='second'",
    ] {
        let mut f = Fixture::new();
        f.pending("first");
        f.pending("second");
        let before = f.journal.verify().unwrap();
        f.db.execute_batch(fault).unwrap();
        drop(f.journal);
        assert!(Journal::open(&f.directory.path().join("control.db")).is_err());
        let running: i64 =
            f.db.query_row(
                "SELECT COUNT(*) FROM operations WHERE state='running'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let audit: i64 =
            f.db.query_row("SELECT COUNT(*) FROM audit", [], |r| r.get(0))
                .unwrap();
        assert_eq!(running, 2, "partial recovery for {fault}");
        assert_eq!(
            Some(audit),
            before.get("events").and_then(serde_json::Value::as_i64),
            "partial recovery audit for {fault}"
        );
    }
}

#[test]
fn corrupt_completed_intents_never_become_successful_replays() {
    for fault in [
        "UPDATE operations SET id='invalid-uuid'",
        "UPDATE operations SET account_ids='not-json'",
        "UPDATE operations SET response='not-json'",
        "UPDATE operations SET response=x'FF'",
    ] {
        let mut f = Fixture::new();
        let id = f.pending("intent");
        f.finish(id, false);
        f.db.execute_batch(fault).unwrap();
        assert_eq!(
            f.claim("intent").err().unwrap().code,
            "service_unavailable",
            "{fault}"
        );
        let count: i64 =
            f.db.query_row("SELECT COUNT(*) FROM operations", [], |r| r.get(0))
                .unwrap();
        assert_eq!(count, 1, "corruption must not create a second intent");
    }
}

#[test]
fn operation_inspection_rejects_corrupt_scope_and_result_metadata() {
    for fault in [
        "UPDATE operations SET account_id='invalid-uuid'",
        "UPDATE operations SET account_ids='not-json'",
        "UPDATE operations SET response='not-json'",
        "UPDATE operations SET response=x'FF'",
    ] {
        let mut f = Fixture::new();
        assert_eq!(
            f.journal.operation(&f.identity, "absent").unwrap_err().code,
            "not_found"
        );
        let id = f.pending("intent");
        f.finish(id, false);
        f.db.execute_batch(fault).unwrap();
        assert_eq!(
            f.journal.operation(&f.identity, "intent").unwrap_err().code,
            "service_unavailable",
            "{fault}"
        );
    }
}

#[test]
fn malformed_unknown_outcomes_cannot_disappear_from_admission_checks() {
    for fault in [
        "UPDATE operations SET id='invalid-uuid'",
        "UPDATE operations SET account_id='invalid-uuid'",
        "UPDATE operations SET account_ids='not-json'",
        "UPDATE operations SET account_ids=x'FF'",
    ] {
        let mut f = Fixture::new();
        let id = f.pending("uncertain");
        f.finish(id, true);
        f.db.execute_batch(fault).unwrap();
        assert_eq!(
            f.journal
                .unresolved_for(&[f.account].into())
                .unwrap_err()
                .code,
            "service_unavailable",
            "{fault}"
        );
    }
}

#[test]
fn resolution_conflicts_roll_back_audit_and_corrupt_evidence_blocks_recovery() {
    let mut f = Fixture::new();
    let id = f.pending("uncertain");
    f.finish(id, true);
    f.journal
        .resolve_operation(
            &f.identity,
            Uuid::new_v4(),
            f.account,
            id,
            "Broker state independently verified.",
        )
        .unwrap();
    let before = f.journal.verify().unwrap();
    assert!(f
        .journal
        .resolve_operation(
            &f.identity,
            Uuid::new_v4(),
            f.account,
            id,
            "Conflicting replacement."
        )
        .is_err());
    assert_eq!(f.journal.verify().unwrap(), before);
    f.db.execute("UPDATE operation_resolutions SET evidence='not-json'", [])
        .unwrap();
    assert_eq!(
        f.journal
            .unresolved_for(&[f.account].into())
            .unwrap_err()
            .code,
        "service_unavailable"
    );
    assert_eq!(
        f.journal
            .operation(&f.identity, "uncertain")
            .unwrap_err()
            .code,
        "service_unavailable"
    );
}

#[test]
fn audit_reads_reject_malformed_records_and_out_of_range_cursors() {
    for fault in [
        "UPDATE audit SET event='not-json'",
        "UPDATE audit SET event=json_set(event,'$.account_id','invalid-uuid')",
        "UPDATE audit SET event=json_set(event,'$.account_ids',json('[\"invalid-uuid\"]'))",
        "UPDATE audit SET event=x'FF'",
    ] {
        let mut f = Fixture::new();
        f.pending("intent");
        assert_eq!(
            f.journal.list(&f.identity, u64::MAX, 10).unwrap_err().code,
            "invalid_argument"
        );
        // Bypass the append-only trigger to simulate damaged persisted bytes.
        f.db.execute_batch("DROP TRIGGER audit_no_update").unwrap();
        f.db.execute_batch(fault).unwrap();
        assert_eq!(
            f.journal.list(&f.identity, 0, 10).unwrap_err().code,
            "service_unavailable",
            "{fault}"
        );
        assert!(f.journal.verify().is_err());
    }
}

#[test]
fn corrupt_safety_json_is_not_returned_as_current_or_historical_evidence() {
    use model::account_safety::{AccountSafetyState, BrokerAccountScope};
    let mut f = Fixture::new();
    let scope = BrokerAccountScope {
        broker: "ibkr".into(),
        environment: "paper".into(),
        account_id: "DU_TEST".into(),
    };
    f.journal
        .save_safety(
            &f.identity,
            Uuid::new_v4(),
            f.account,
            &scope,
            &AccountSafetyState::default(),
        )
        .unwrap();
    f.db.execute("UPDATE account_safety SET state='not-json'", [])
        .unwrap();
    assert_eq!(
        f.journal.safety_state(&scope).unwrap_err().code,
        "service_unavailable"
    );
    f.db.execute("UPDATE account_safety_history SET state='not-json'", [])
        .unwrap();
    assert_eq!(
        f.journal.list(&f.identity, 0, 10).unwrap_err().code,
        "service_unavailable"
    );
}

#[test]
fn missing_control_storage_cannot_be_silently_reinitialized() {
    let f = Fixture::new();
    assert!(Journal::open(&f.directory.path().join("missing/control.db")).is_err());
    f.db.execute_batch("DROP TABLE operations").unwrap();
    assert!(Journal::open(&f.directory.path().join("control.db")).is_err());
}

#[test]
fn batch_resolution_checks_validate_every_record_and_reverify_the_next_read() {
    let mut f = Fixture::new();
    let mut operations = Vec::new();
    for index in 0..5 {
        let id = f.pending(&format!("resolved-{index}"));
        f.finish(id, true);
        f.journal
            .resolve_operation(&f.identity, Uuid::new_v4(), f.account, id, "Verified.")
            .unwrap();
        operations.push(id);
    }
    assert!(f
        .journal
        .unresolved_for(&[f.account].into())
        .unwrap()
        .is_empty());
    let last = operations.last().unwrap();
    f.db.execute(
        "UPDATE operation_resolutions SET evidence='{}' WHERE operation_id=?1",
        [last.to_string()],
    )
    .unwrap();
    assert_eq!(
        f.journal
            .unresolved_for(&[f.account].into())
            .unwrap_err()
            .code,
        "audit_integrity_failed"
    );
    f.db.execute(
        "DELETE FROM operation_resolutions WHERE operation_id=?1",
        [last.to_string()],
    )
    .unwrap();
    assert_eq!(
        f.journal.unresolved_for(&[f.account].into()).unwrap(),
        [*last].into()
    );
    f.db.execute_batch(
        "DROP TRIGGER audit_no_update; UPDATE audit SET event='{}' WHERE sequence=1;",
    )
    .unwrap();
    assert_eq!(
        f.journal
            .unresolved_for(&[f.account].into())
            .unwrap_err()
            .code,
        "audit_integrity_failed"
    );
}

#[test]
fn control_index_migration_preserves_evidence_and_indexes_admission_queries() {
    let mut f = Fixture::new();
    let operation = f.pending("migration");
    f.finish(operation, false);
    let before = f.journal.verify().unwrap();
    f.db.execute_batch(include_str!("../migrations/0003_control_indexes/down.sql"))
        .unwrap();
    drop(f.journal);
    let journal = Journal::open(&f.directory.path().join("control.db")).unwrap();
    assert_eq!(journal.verify().unwrap(), before);
    assert_eq!(
        journal
            .operation(&f.identity, "migration")
            .unwrap()
            .get("state"),
        Some(&json!("completed"))
    );
    assert_eq!(
        f.db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        3
    );
    let migrated = Connection::open(f.directory.path().join("control.db")).unwrap();
    for (query, index) in [
        ("EXPLAIN QUERY PLAN SELECT MAX(audit_sequence) FROM account_safety_history WHERE scope='scope'", "account_safety_history_scope_sequence"),
        ("EXPLAIN QUERY PLAN SELECT id FROM operations WHERE state='outcome_unknown'", "operations_unfinished"),
        ("EXPLAIN QUERY PLAN SELECT id FROM operations WHERE state='running'", "operations_unfinished"),
    ] {
        let plan = migrated.prepare(query).unwrap().query_map([], |row| row.get::<_, String>(3)).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap().join("\n");
        assert!(plan.contains(index), "Admission query must use {index}: {plan}");
    }
}

#[test]
#[ignore = "Offline release performance gate; run via make service-perf"]
fn large_history_resolution_verification_budget() {
    use std::time::Instant;
    let mut f = Fixture::new();
    for _ in 0..10_000 {
        f.journal
            .record(
                &f.identity,
                Uuid::new_v4(),
                "account_readiness",
                Some(f.account),
                "read",
            )
            .unwrap();
    }
    for index in 0..50 {
        let id = f.pending(&format!("resolved-{index}"));
        f.finish(id, true);
        f.journal
            .resolve_operation(
                &f.identity,
                Uuid::new_v4(),
                f.account,
                id,
                "Independently reconciled fixture.",
            )
            .unwrap();
    }
    let before = f.journal.verify().unwrap();
    let mut samples = Vec::new();
    for _ in 0..10 {
        let started = Instant::now();
        assert!(f
            .journal
            .unresolved_for(&[f.account].into())
            .unwrap()
            .is_empty());
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    println!(
        "journal_perf {}",
        json!({"historical_reads":10000,"resolved_outcomes":50,"samples":samples.len(),"median_us":samples.get(5).unwrap().as_micros(),"max_us":samples.last().unwrap().as_micros()})
    );
    assert_eq!(
        f.journal.verify().unwrap(),
        before,
        "Readiness must not mutate its evidence"
    );
    assert!(
        samples.get(5).unwrap() < &std::time::Duration::from_millis(250),
        "Resolved historical incidents must not multiply full-history verification cost"
    );
}
