//! Tamper-evident audit events for financially significant workflows.

use model::{BrokerLog, DatabaseFactory, Status, Trade};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use uuid::Uuid;

const PREFIX: &str = "trust-audit:v1:";
const GENESIS_HASH: &str = "GENESIS";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Versioned, tamper-evident record of one trade-lifecycle action or outcome.
pub struct AuditEvent {
    /// Audit envelope schema version.
    pub version: u8,
    /// Globally unique event identifier.
    pub event_id: String,
    /// Identifier shared by the attempt and outcome of one operation.
    pub operation_id: String,
    /// RFC 3339 UTC timestamp.
    pub occurred_at: String,
    /// Related trade identifier.
    pub trade_id: String,
    /// Related account identifier.
    pub account_id: String,
    /// Broker adapter responsible for the operation.
    pub broker: String,
    /// Stable machine-readable event type.
    pub event_type: String,
    /// Stable machine-readable operation outcome.
    pub outcome: String,
    /// Persisted status before the event.
    pub prior_status: String,
    /// Persisted status after the event.
    pub resulting_status: String,
    #[serde(default)]
    /// Local actor supplied by `TRUST_AUDIT_ACTOR`, falling back to the OS user.
    pub actor: String,
    #[serde(default)]
    /// Request/correlation identifier supplied by the caller or generated locally.
    pub request_id: String,
    #[serde(default)]
    /// Local process identifier that emitted the event.
    pub process_id: u32,
    /// Redacted event-specific structured attributes.
    pub details: Value,
    /// Hash of the preceding event, or `GENESIS` for the first event.
    pub previous_hash: String,
    /// SHA-256 hash of this canonical event envelope.
    pub event_hash: String,
}

/// Result of verifying one complete trade audit chain.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuditVerification {
    /// Trade whose chain was verified.
    pub trade_id: String,
    /// Number of structured events in the chain.
    pub event_count: usize,
    /// Hash at the head of the verified chain, or `GENESIS` when empty.
    pub head_hash: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuditRecord<'a> {
    pub(crate) operation_id: Uuid,
    pub(crate) event_type: &'a str,
    pub(crate) outcome: &'a str,
    pub(crate) prior_status: Status,
    pub(crate) resulting_status: Status,
    pub(crate) details: &'a Value,
}

pub(crate) fn record(
    database: &mut dyn DatabaseFactory,
    trade: &Trade,
    broker: &str,
    record: AuditRecord<'_>,
) -> Result<BrokerLog, Box<dyn Error>> {
    crate::commands::atomic::with_savepoint(database, "audit_append", |database| {
        let previous_hash = database
            .log_read()
            .read_latest_log_for_trade_with_prefix(trade.id, PREFIX)?
            .map(|log| validated_event_hash(&log))
            .transpose()?
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        let event_id = Uuid::new_v4().to_string();
        let operation_id = record.operation_id.to_string();
        let occurred_at = chrono::Utc::now().to_rfc3339();
        let trade_id = trade.id.to_string();
        let account_id = trade.account_id.to_string();
        let prior_status = record.prior_status.to_string();
        let resulting_status = record.resulting_status.to_string();
        let actor = bounded_identity("TRUST_AUDIT_ACTOR", "USER", "unknown");
        let request_id = bounded_identity(
            "TRUST_AUDIT_REQUEST_ID",
            "TRUST_REQUEST_ID",
            &record.operation_id.to_string(),
        );
        let mut event = AuditEvent {
            version: 2,
            event_id,
            operation_id,
            occurred_at,
            trade_id,
            account_id,
            broker: broker.to_string(),
            event_type: record.event_type.to_string(),
            outcome: record.outcome.to_string(),
            prior_status,
            resulting_status,
            actor,
            request_id,
            process_id: std::process::id(),
            details: record.details.clone(),
            previous_hash,
            event_hash: String::new(),
        };
        event.event_hash = hash_event(&event)?;
        let encoded = format!("{PREFIX}{}", serde_json::to_string(&event)?);
        database.log_write().create_log(&encoded, trade)
    })
}

pub(crate) fn verify(logs: &[BrokerLog]) -> Result<usize, Box<dyn Error>> {
    Ok(ordered_events(logs)?.len())
}

pub(crate) fn ordered_events(logs: &[BrokerLog]) -> Result<Vec<AuditEvent>, Box<dyn Error>> {
    let events = parse_and_validate_events(logs)?;
    let mut by_previous = HashMap::new();
    for event in &events {
        if by_previous
            .insert(event.previous_hash.as_str(), event)
            .is_some()
        {
            return Err(format!("audit chain forks after hash {}", event.previous_hash).into());
        }
    }

    let mut expected_previous = GENESIS_HASH;
    let mut visited = HashSet::new();
    let mut ordered = Vec::with_capacity(events.len());
    while let Some(event) = by_previous.get(expected_previous) {
        if !visited.insert(event.event_hash.as_str()) {
            return Err("audit chain contains a cycle".into());
        }
        ordered.push((*event).clone());
        expected_previous = &event.event_hash;
    }
    if visited.len() != events.len() {
        return Err("audit chain contains an orphaned or disconnected event".into());
    }
    Ok(ordered)
}

pub(crate) fn verification(
    trade_id: Uuid,
    logs: &[BrokerLog],
) -> Result<AuditVerification, Box<dyn Error>> {
    let events = ordered_events(logs)?;
    let head_hash = events.last().map_or_else(
        || GENESIS_HASH.to_string(),
        |event| event.event_hash.clone(),
    );
    Ok(AuditVerification {
        trade_id: trade_id.to_string(),
        event_count: events.len(),
        head_hash,
    })
}

fn bounded_identity(primary: &str, fallback: &str, default: &str) -> String {
    std::env::var(primary)
        .ok()
        .or_else(|| std::env::var(fallback).ok())
        .map(|value| {
            value
                .chars()
                .filter(|character| !character.is_control())
                .take(128)
                .collect::<String>()
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn parse_and_validate_events(logs: &[BrokerLog]) -> Result<Vec<AuditEvent>, Box<dyn Error>> {
    let mut events = Vec::new();
    let mut hashes = HashSet::new();
    for log in logs {
        let Some(encoded) = log.log.strip_prefix(PREFIX) else {
            continue;
        };
        let event: AuditEvent = serde_json::from_str(encoded)?;
        let calculated = hash_event(&event)?;
        if calculated != event.event_hash {
            return Err(format!(
                "audit hash mismatch for event {} (stored {}, calculated {})",
                event.event_id, event.event_hash, calculated
            )
            .into());
        }
        if !hashes.insert(event.event_hash.clone()) {
            return Err(format!("duplicate audit hash for event {}", event.event_id).into());
        }
        events.push(event);
    }
    Ok(events)
}

fn validated_event_hash(log: &BrokerLog) -> Result<String, Box<dyn Error>> {
    let encoded = log
        .log
        .strip_prefix(PREFIX)
        .ok_or("latest audit log has an invalid prefix")?;
    let event: AuditEvent = serde_json::from_str(encoded)?;
    let calculated = hash_event(&event)?;
    if calculated != event.event_hash {
        return Err(format!("audit hash mismatch for event {}", event.event_id).into());
    }
    Ok(event.event_hash)
}

fn hash_event(event: &AuditEvent) -> Result<String, Box<dyn Error>> {
    let mut value = serde_json::to_value(event)?;
    let object = value
        .as_object_mut()
        .ok_or("audit event did not serialize as an object")?;
    object.remove("event_hash");
    if event.version == 1 {
        // Version 1 predates local actor/correlation metadata. Omitting these defaulted fields
        // preserves verification of audit records written before the version 2 envelope.
        object.remove("actor");
        object.remove("request_id");
        object.remove("process_id");
    }
    let canonical = canonicalize_json(value);
    let bytes = serde_json::to_vec(&canonical)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize_json(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_json).collect()),
        primitive => primitive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(text: String) -> BrokerLog {
        BrokerLog {
            log: text,
            ..BrokerLog::default()
        }
    }

    fn encoded_event(mut event: AuditEvent) -> BrokerLog {
        event.event_hash = hash_event(&event).expect("hash should serialize");
        log(format!(
            "{PREFIX}{}",
            serde_json::to_string(&event).expect("event should serialize")
        ))
    }

    #[test]
    fn verification_ignores_legacy_logs_and_detects_tampering() {
        let trade = Trade::default();
        let details = serde_json::json!({"order_role":"entry"});
        let event_id = Uuid::new_v4().to_string();
        let operation_id = Uuid::new_v4().to_string();
        let occurred_at = chrono::Utc::now().to_rfc3339();
        let trade_id = trade.id.to_string();
        let account_id = trade.account_id.to_string();
        let prior_status = Status::Funded.to_string();
        let resulting_status = Status::Submitted.to_string();
        let mut event = AuditEvent {
            version: 2,
            event_id,
            operation_id,
            occurred_at,
            trade_id,
            account_id,
            broker: "ibkr".to_string(),
            event_type: "trade.submit".to_string(),
            outcome: "attempted".to_string(),
            prior_status,
            resulting_status,
            actor: "test-agent".to_string(),
            request_id: Uuid::new_v4().to_string(),
            process_id: 1,
            details,
            previous_hash: GENESIS_HASH.to_string(),
            event_hash: String::new(),
        };
        event.event_hash = hash_event(&event).expect("hash should serialize");
        let mut logs = vec![
            log("legacy broker response".to_string()),
            log(format!(
                "{PREFIX}{}",
                serde_json::to_string(&event).expect("event should serialize")
            )),
        ];
        assert_eq!(verify(&logs).expect("valid chain"), 1);

        let audit_log = logs.get_mut(1).expect("audit log should exist");
        audit_log.log = audit_log.log.replace("attempted", "accepted");
        assert!(verify(&logs).is_err());
    }

    #[test]
    fn verification_reconstructs_unordered_chain_and_rejects_forks() {
        let base = AuditEvent {
            version: 2,
            event_id: Uuid::new_v4().to_string(),
            operation_id: Uuid::new_v4().to_string(),
            occurred_at: chrono::Utc::now().to_rfc3339(),
            trade_id: Uuid::new_v4().to_string(),
            account_id: Uuid::new_v4().to_string(),
            broker: "ibkr".to_string(),
            event_type: "trade.submit".to_string(),
            outcome: "attempted".to_string(),
            prior_status: Status::Funded.to_string(),
            resulting_status: Status::Funded.to_string(),
            actor: "test-agent".to_string(),
            request_id: Uuid::new_v4().to_string(),
            process_id: 1,
            details: serde_json::json!({}),
            previous_hash: GENESIS_HASH.to_string(),
            event_hash: String::new(),
        };
        let mut first = base.clone();
        first.event_hash = hash_event(&first).expect("first event should hash");
        let mut second = base.clone();
        second.event_id = Uuid::new_v4().to_string();
        second.outcome = "acknowledged".to_string();
        second.previous_hash = first.event_hash.clone();
        let unordered = vec![encoded_event(second), encoded_event(first.clone())];
        assert_eq!(
            verify(&unordered).expect("unordered chain should verify"),
            2
        );

        let mut fork = base;
        fork.event_id = Uuid::new_v4().to_string();
        fork.outcome = "retry_safe_failure".to_string();
        assert!(verify(&[encoded_event(first), encoded_event(fork)]).is_err());
    }

    #[test]
    fn version_one_events_without_actor_fields_remain_verifiable() {
        let mut event = AuditEvent {
            version: 1,
            event_id: Uuid::new_v4().to_string(),
            operation_id: Uuid::new_v4().to_string(),
            occurred_at: chrono::Utc::now().to_rfc3339(),
            trade_id: Uuid::new_v4().to_string(),
            account_id: Uuid::new_v4().to_string(),
            broker: "ibkr".to_string(),
            event_type: "trade.submit".to_string(),
            outcome: "attempted".to_string(),
            prior_status: Status::Funded.to_string(),
            resulting_status: Status::Funded.to_string(),
            actor: String::new(),
            request_id: String::new(),
            process_id: 0,
            details: serde_json::json!({}),
            previous_hash: GENESIS_HASH.to_string(),
            event_hash: String::new(),
        };
        event.event_hash = hash_event(&event).expect("legacy event should hash");
        let mut json = serde_json::to_value(&event).expect("legacy event should serialize");
        let object = json.as_object_mut().expect("event should be an object");
        object.remove("actor");
        object.remove("request_id");
        object.remove("process_id");
        let legacy = log(format!("{PREFIX}{json}"));

        assert_eq!(verify(&[legacy]).expect("legacy chain should verify"), 1);
    }
}
