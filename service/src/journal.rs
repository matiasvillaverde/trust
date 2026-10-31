use crate::{
    auth::Identity,
    error::{Result, ServiceError},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt::Write, path::Path};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reply {
    pub schema_version: u32,
    pub request_id: Uuid,
    pub operation_id: Option<Uuid>,
    pub data: Option<Value>,
    pub error: Option<ServiceError>,
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub actions: Vec<crate::guidance::NextAction>,
    #[serde(default)]
    pub generated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Reply {
    pub fn from_result(
        request_id: Uuid,
        operation_id: Option<Uuid>,
        result: Result<Value>,
    ) -> Self {
        match result {
            Ok(data) => Self {
                schema_version: 1,
                request_id,
                operation_id,
                data: Some(data),
                error: None,
                next_actions: vec![],
                actions: vec![],
                generated_at: Some(chrono::Utc::now()),
            },
            Err(error) => Self {
                schema_version: 1,
                request_id,
                operation_id,
                next_actions: vec![error.next_action.clone()],
                data: None,
                error: Some(error),
                actions: vec![],
                generated_at: Some(chrono::Utc::now()),
            },
        }
    }
}

pub enum Claim {
    New(Uuid),
    Existing(Reply),
}

#[derive(Clone, Debug)]
pub struct AuditContext {
    pub replayed: bool,
    pub source: &'static str,
    pub queue_wait_ms: u64,
    pub duration_ms: u64,
    pub error_code: Option<String>,
    pub account_ids: BTreeSet<Uuid>,
}

impl Default for AuditContext {
    fn default() -> Self {
        Self {
            replayed: false,
            source: "internal",
            queue_wait_ms: 0,
            duration_ms: 0,
            error_code: None,
            account_ids: BTreeSet::new(),
        }
    }
}

pub struct Journal {
    connection: Connection,
    pub context: AuditContext,
}

pub fn digest(value: &Value) -> Result<String> {
    // serde_json's default map is a BTreeMap, so key order is canonical here.
    let bytes = serde_json::to_vec(value).map_err(|_| ServiceError::internal())?;
    encode_hash(Sha256::digest(bytes).as_ref())
}

fn encode_hash(bytes: &[u8]) -> Result<String> {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").map_err(|_| ServiceError::internal())?;
    }
    Ok(encoded)
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path).map_err(|_| ServiceError::internal())?;
        Self::initialize(connection)
    }

    pub fn open_encrypted(path: &Path, key: &trust_security::DatabaseKey) -> Result<Self> {
        let _file = trust_security::private_file(path, false)
            .map_err(|_| crate::config::storage_error())?;
        let connection = Connection::open(path).map_err(|_| crate::config::storage_error())?;
        crate::storage::unlock(&connection, key)?;
        Self::initialize(connection)
    }

    fn initialize(connection: Connection) -> Result<Self> {
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
            )
            .map_err(|_| ServiceError::internal())?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| ServiceError::internal())?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|_| ServiceError::internal())?;
        if version > 3 {
            return Err(ServiceError::invalid(
                "Control database schema is newer than this service.",
            ));
        }
        if version < 1 {
            connection
                .execute_batch(include_str!("../migrations/0001_control/up.sql"))
                .map_err(|_| ServiceError::internal())?;
        }
        if version < 2 {
            connection
                .execute_batch(include_str!("../migrations/0002_account_safety/up.sql"))
                .map_err(|_| ServiceError::internal())?;
        }
        if version < 3 {
            connection
                .execute_batch(include_str!("../migrations/0003_control_indexes/up.sql"))
                .map_err(|_| ServiceError::internal())?;
        }
        let mut journal = Self {
            connection,
            context: AuditContext::default(),
        };
        journal.verify()?;
        // Recovery and its evidence commit together. Never re-execute durable intent.
        let pending = {
            let mut stmt = journal.connection.prepare("SELECT id,subject,tool,account_id,account_ids FROM operations WHERE state='running'").map_err(|_| ServiceError::internal())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(|_| ServiceError::internal())?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| ServiceError::internal())?
        };
        if !pending.is_empty() {
            let tx = journal
                .connection
                .transaction()
                .map_err(|_| ServiceError::internal())?;
            for (id, subject, tool, account, accounts) in pending {
                let operation = Uuid::parse_str(&id).map_err(|_| ServiceError::internal())?;
                let account = account
                    .as_deref()
                    .map(Uuid::parse_str)
                    .transpose()
                    .map_err(|_| ServiceError::internal())?;
                let context = AuditContext {
                    source: "recovery",
                    error_code: Some("outcome_unknown".into()),
                    account_ids: serde_json::from_str(&accounts)
                        .map_err(|_| ServiceError::internal())?,
                    ..AuditContext::default()
                };
                let identity = Identity {
                    subject,
                    client_id: None,
                    expires_at: 0,
                    owner_authenticated_until: None,
                    scopes: BTreeSet::new(),
                    grant: crate::auth::Grant {
                        role: crate::auth::Role::Agent,
                        accounts: BTreeSet::new(),
                        all_accounts: false,
                        scopes: BTreeSet::new(),
                        allow_live: false,
                    },
                };
                tx.execute(
                    "UPDATE operations SET state='outcome_unknown' WHERE id=?1 AND state='running'",
                    [&id],
                )
                .map_err(|_| ServiceError::internal())?;
                append(
                    &tx,
                    &identity,
                    Uuid::new_v4(),
                    Some(operation),
                    &tool,
                    account,
                    "recovered_unknown",
                    None,
                    &context,
                )?;
                tracing::warn!(event="operation_recovered", operation_id=%operation, tool, "Unfinished operation requires reconciliation");
            }
            tx.commit().map_err(|_| ServiceError::internal())?;
        }
        Ok(journal)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        &mut self,
        identity: &Identity,
        key: &str,
        tool: &str,
        account: Option<Uuid>,
        arguments: &Value,
        request: Uuid,
        accounts: &BTreeSet<Uuid>,
    ) -> Result<Claim> {
        self.context.account_ids.extend(accounts.iter().copied());
        let request_hash = digest(&json!({"tool":tool,"arguments":arguments}))?;
        if let Some((operation_id, hash, state, response, stored_accounts)) = self.connection.query_row(
            "SELECT id, request_hash, state, response, account_ids FROM operations WHERE subject=?1 AND request_key=?2",
            params![identity.subject, key],
            |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,String>(4)?))
        ).optional().map_err(|_| ServiceError::internal())? {
            if hash != request_hash { return Err(ServiceError::new("idempotency_conflict","This key was already used for a different request.","Use the original request to inspect its outcome; use a new key only for a distinct intent.")); }
            let original_accounts:BTreeSet<Uuid>=serde_json::from_str(&stored_accounts).map_err(|_|ServiceError::internal())?;
            for account in &original_accounts { identity.check(crate::catalog::find(tool)?.scope,Some(*account))?; }
            self.context.account_ids.extend(original_accounts);
            let id = Uuid::parse_str(&operation_id).map_err(|_| ServiceError::internal())?;
            self.context.replayed = true;
            let tx = self.connection.transaction().map_err(|_| ServiceError::internal())?;
            append(&tx, identity, request, Some(id), tool, account, "replayed", None, &self.context)?;
            tx.commit().map_err(|_| ServiceError::internal())?;
            if let Some(response) = response {
                let mut reply: Reply = serde_json::from_str(&response).map_err(|_| ServiceError::internal())?;
                reply.request_id = request;
                return Ok(Claim::Existing(reply));
            }
            return Ok(Claim::Existing(Reply::from_result(request, Some(id), Err(ServiceError::new(&state,"The original operation has no confirmed completion record.","Inspect trade state and reconcile with the broker. Do not repeat the mutation with a new key.")))));
        }
        let id = Uuid::new_v4();
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ServiceError::internal())?;
        transaction.execute("INSERT INTO operations (id,subject,request_key,tool,account_id,request_hash,state,created_at,account_ids) VALUES (?1,?2,?3,?4,?5,?6,'running',?7,?8)",params![id.to_string(),identity.subject,key,tool,account.map(|v|v.to_string()),request_hash,chrono::Utc::now().to_rfc3339(),serde_json::to_string(accounts).map_err(|_|ServiceError::internal())?]).map_err(|_| ServiceError::internal())?;
        append(
            &transaction,
            identity,
            request,
            Some(id),
            tool,
            account,
            "attempted",
            Some(&request_hash),
            &self.context,
        )?;
        transaction.commit().map_err(|_| ServiceError::internal())?;
        Ok(Claim::New(id))
    }

    pub fn finish(
        &mut self,
        identity: &Identity,
        tool: &str,
        account: Option<Uuid>,
        reply: &Reply,
    ) -> Result<()> {
        let id = reply.operation_id.ok_or_else(ServiceError::internal)?;
        let response = serde_json::to_string(reply).map_err(|_| ServiceError::internal())?;
        let state = if reply
            .error
            .as_ref()
            .is_some_and(|error| error.code == "outcome_unknown")
        {
            "outcome_unknown"
        } else if reply.error.is_some() {
            "failed"
        } else {
            "completed"
        };
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ServiceError::internal())?;
        let changed = transaction.execute("UPDATE operations SET state=?1,response=?2 WHERE id=?3 AND subject=?4 AND state='running'",params![state,response,id.to_string(),identity.subject]).map_err(|_| ServiceError::internal())?;
        if changed != 1 {
            return Err(ServiceError::internal());
        }
        let response_hash =
            digest(&serde_json::to_value(reply).map_err(|_| ServiceError::internal())?)?;
        append(
            &transaction,
            identity,
            reply.request_id,
            Some(id),
            tool,
            account,
            state,
            Some(&response_hash),
            &self.context,
        )?;
        transaction.commit().map_err(|_| ServiceError::internal())?;
        Ok(())
    }

    pub fn record(
        &mut self,
        identity: &Identity,
        request: Uuid,
        tool: &str,
        account: Option<Uuid>,
        outcome: &str,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction()
            .map_err(|_| ServiceError::internal())?;
        append(
            &tx,
            identity,
            request,
            None,
            tool,
            account,
            outcome,
            None,
            &self.context,
        )?;
        tx.commit().map_err(|_| ServiceError::internal())
    }

    pub fn safety_state(
        &self,
        scope: &model::account_safety::BrokerAccountScope,
    ) -> Result<model::account_safety::AccountSafetyState> {
        self.read(|snapshot| snapshot.safety_state(scope))
    }

    /// Load policy, reservations and unresolved outcomes from one verified snapshot.
    pub fn account_safety(
        &self,
        scope: &model::account_safety::BrokerAccountScope,
        accounts: &BTreeSet<Uuid>,
    ) -> Result<model::account_safety::AccountSafetyState> {
        self.read(|snapshot| {
            let mut state = snapshot.safety_state(scope)?;
            state.unresolved_operations = snapshot.unresolved_for(accounts)?;
            Ok(state)
        })
    }

    pub fn save_safety(
        &mut self,
        identity: &Identity,
        request: Uuid,
        account: Uuid,
        scope: &model::account_safety::BrokerAccountScope,
        state: &model::account_safety::AccountSafetyState,
    ) -> Result<()> {
        let key = scope.storage_key();
        let value = serde_json::to_value(state).map_err(|_| ServiceError::internal())?;
        let hash = digest(&json!({"scope":key,"state":value}))?;
        let encoded = serde_json::to_string(&value).map_err(|_| ServiceError::internal())?;
        let tx = self
            .connection
            .transaction()
            .map_err(|_| ServiceError::internal())?;
        append(
            &tx,
            identity,
            request,
            None,
            "account_safety",
            Some(account),
            "safety_state",
            Some(&hash),
            &self.context,
        )?;
        let sequence = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO account_safety_history(audit_sequence,scope,state) VALUES(?1,?2,?3)",
            params![sequence, key, encoded],
        )
        .map_err(|_| ServiceError::internal())?;
        tx.execute("INSERT INTO account_safety(scope,state,audit_sequence) VALUES(?1,?2,?3) ON CONFLICT(scope) DO UPDATE SET state=excluded.state,audit_sequence=excluded.audit_sequence", params![key,encoded,sequence]).map_err(|_| ServiceError::internal())?;
        tx.commit().map_err(|_| ServiceError::internal())
    }

    pub fn unresolved_for(&self, accounts: &BTreeSet<Uuid>) -> Result<BTreeSet<Uuid>> {
        self.read(|snapshot| snapshot.unresolved_for(accounts))
    }

    pub fn resolve_operation(
        &mut self,
        identity: &Identity,
        request: Uuid,
        account: Uuid,
        operation: Uuid,
        evidence: &str,
    ) -> Result<()> {
        let evidence =
            serde_json::to_string(&json!({"operation_id":operation,"evidence":evidence}))
                .map_err(|_| ServiceError::internal())?;
        let hash = digest(&serde_json::from_str(&evidence).map_err(|_| ServiceError::internal())?)?;
        let tx = self
            .connection
            .transaction()
            .map_err(|_| ServiceError::internal())?;
        append(
            &tx,
            identity,
            request,
            Some(operation),
            "operation_resolve",
            Some(account),
            "owner_reconciled",
            Some(&hash),
            &self.context,
        )?;
        tx.execute("INSERT INTO operation_resolutions(operation_id,evidence,audit_sequence) VALUES(?1,?2,?3)",params![operation.to_string(),evidence,tx.last_insert_rowid()]).map_err(|_|ServiceError::internal())?;
        tx.commit().map_err(|_| ServiceError::internal())
    }

    pub fn operation(&self, identity: &Identity, key: &str) -> Result<Value> {
        self.read(|snapshot| snapshot.operation(identity, key))
    }

    pub fn list(&self, identity: &Identity, after: u64, limit: u64) -> Result<Value> {
        let after = i64::try_from(after)
            .map_err(|_| ServiceError::invalid("Audit cursor is too large."))?;
        let mut stmt = self
            .connection
            .prepare(
                "SELECT a.sequence,a.event,a.previous_hash,a.event_hash,h.scope,h.state FROM audit a LEFT JOIN account_safety_history h ON h.audit_sequence=a.sequence WHERE a.sequence>?1 ORDER BY a.sequence LIMIT 2000",
            )
            .map_err(|_| ServiceError::internal())?;
        let rows = stmt
            .query_map([after], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(|_| ServiceError::internal())?;
        let mut events = Vec::new();
        let mut cursor = u64::try_from(after).map_err(|_| ServiceError::internal())?;
        for row in rows {
            let (sequence, text, previous_hash, event_hash, scope, state) =
                row.map_err(|_| ServiceError::internal())?;
            let sequence = u64::try_from(sequence).map_err(|_| ServiceError::internal())?;
            cursor = sequence;
            let event: Value = serde_json::from_str(&text).map_err(|_| ServiceError::internal())?;
            let account = event
                .get("account_id")
                .and_then(Value::as_str)
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| ServiceError::internal())?;
            let own =
                event.get("subject").and_then(Value::as_str) == Some(identity.subject.as_str());
            let accounts: BTreeSet<Uuid> = event
                .get("account_ids")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| ServiceError::internal())?
                .unwrap_or_default();
            if own
                && account.is_none_or(|id| identity.permits_account(id))
                && accounts.iter().all(|id| identity.permits_account(*id))
                || identity.permits("trust:admin") && identity.grant.all_accounts
            {
                // Complete historical broker evidence is restricted to the global owner.
                let evidence = if identity.grant.role == crate::auth::Role::Owner
                    && identity.grant.all_accounts
                {
                    state
                        .as_deref()
                        .map(serde_json::from_str::<Value>)
                        .transpose()
                        .map_err(|_| ServiceError::internal())?
                } else {
                    None
                };
                if let Some(state) = &evidence {
                    if event.get("content_hash").and_then(Value::as_str)
                        != Some(digest(&json!({"scope":scope,"state":state}))?.as_str())
                    {
                        return Err(ServiceError::new(
                            "audit_integrity_failed",
                            "Historical safety evidence does not match its audit record.",
                            "Stop trading and restore verified control storage.",
                        ));
                    }
                }
                events.push(json!({"sequence":sequence,"event":event,"previous_hash":previous_hash,"event_hash":event_hash,"safety_evidence":evidence}));
            }
            if u64::try_from(events.len()).map_err(|_| ServiceError::internal())? >= limit {
                break;
            }
        }
        Ok(json!({"events":events,"next_cursor":cursor}))
    }

    pub fn verify(&self) -> Result<Value> {
        verify_chain(&self.connection)
    }

    fn read<T>(&self, query: impl FnOnce(&ReadSnapshot<'_>) -> Result<T>) -> Result<T> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(|_| ServiceError::internal())?;
        let result = query(&ReadSnapshot {
            connection: &tx,
            audit_verified: std::cell::Cell::new(false),
        })?;
        tx.commit().map_err(|_| ServiceError::internal())?;
        Ok(result)
    }
}

// Verification is reusable only inside this SQLite read transaction. Nothing
// survives a request or a commit; external changes are checked on the next read.
struct ReadSnapshot<'a> {
    connection: &'a Connection,
    audit_verified: std::cell::Cell<bool>,
}

impl ReadSnapshot<'_> {
    fn verify_audit(&self) -> Result<()> {
        if !self.audit_verified.get() {
            verify_chain(self.connection)?;
            self.audit_verified.set(true);
        }
        Ok(())
    }

    fn safety_state(
        &self,
        scope: &model::account_safety::BrokerAccountScope,
    ) -> Result<model::account_safety::AccountSafetyState> {
        let key = scope.storage_key();
        let row = self.connection.query_row("SELECT s.state,a.event,s.audit_sequence FROM account_safety s LEFT JOIN audit a ON a.sequence=s.audit_sequence WHERE s.scope=?1", [&key], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,i64>(2)?))).optional().map_err(|_| ServiceError::internal())?;
        let latest: Option<i64> = self
            .connection
            .query_row(
                "SELECT MAX(audit_sequence) FROM account_safety_history WHERE scope=?1",
                [&key],
                |r| r.get(0),
            )
            .map_err(|_| ServiceError::internal())?;
        let corrupt = || {
            ServiceError::new(
                "audit_integrity_failed",
                "Account safety state is missing or does not match its latest audit record.",
                "Stop trading and restore verified control storage.",
            )
        };
        let Some((state, event, sequence)) = row else {
            return if latest.is_some() {
                Err(corrupt())
            } else {
                Ok(Default::default())
            };
        };
        if latest != Some(sequence) {
            return Err(corrupt());
        }
        let event = event.ok_or_else(corrupt)?;
        self.verify_audit()?;
        let value: Value = serde_json::from_str(&state).map_err(|_| ServiceError::internal())?;
        let event: Value = serde_json::from_str(&event).map_err(|_| ServiceError::internal())?;
        if event.get("content_hash").and_then(Value::as_str)
            != Some(digest(&json!({"scope":key,"state":value}))?.as_str())
        {
            return Err(ServiceError::new(
                "audit_integrity_failed",
                "Account safety state does not match its audit record.",
                "Stop trading and restore verified control storage.",
            ));
        }
        serde_json::from_value(value).map_err(|_| ServiceError::internal())
    }

    fn unresolved_for(&self, accounts: &BTreeSet<Uuid>) -> Result<BTreeSet<Uuid>> {
        let mut stmt = self
            .connection
            .prepare("SELECT o.id,o.account_id,o.account_ids,o.tool FROM operations o WHERE o.state='outcome_unknown'")
            .map_err(|_| ServiceError::internal())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|_| ServiceError::internal())?;
        let mut ids = BTreeSet::new();
        for row in rows {
            let (operation, account, affected, tool) = row.map_err(|_| ServiceError::internal())?;
            if !crate::catalog::find(&tool)?.affects_financial_state() {
                continue;
            }
            let mut affected: BTreeSet<Uuid> =
                serde_json::from_str(&affected).map_err(|_| ServiceError::internal())?;
            if let Some(account) = account {
                affected.insert(account.parse().map_err(|_| ServiceError::internal())?);
            }
            if !affected.is_disjoint(accounts) && self.resolution(&operation)?.is_none() {
                ids.insert(operation.parse().map_err(|_| ServiceError::internal())?);
            }
        }
        Ok(ids)
    }

    fn resolution(&self, operation: &str) -> Result<Option<Value>> {
        let row = self.connection.query_row("SELECT r.evidence,a.event FROM operation_resolutions r LEFT JOIN audit a ON a.sequence=r.audit_sequence WHERE r.operation_id=?1", [operation], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?))).optional().map_err(|_|ServiceError::internal())?;
        let Some((evidence, event)) = row else {
            return Ok(None);
        };
        self.verify_audit()?;
        let evidence: Value =
            serde_json::from_str(&evidence).map_err(|_| ServiceError::internal())?;
        let event: Value = serde_json::from_str(event.as_deref().unwrap_or("null"))
            .map_err(|_| ServiceError::internal())?;
        if evidence.get("operation_id").and_then(Value::as_str) != Some(operation)
            || event.get("content_hash").and_then(Value::as_str)
                != Some(digest(&evidence)?.as_str())
            || event.get("tool").and_then(Value::as_str) != Some("operation_resolve")
        {
            return Err(ServiceError::new(
                "audit_integrity_failed",
                "Operation resolution does not match its audit record.",
                "Stop trading and restore verified control storage.",
            ));
        }
        Ok(Some(evidence))
    }

    fn operation(&self, identity: &Identity, key: &str) -> Result<Value> {
        let row = self.connection.query_row("SELECT id,tool,account_id,state,response,account_ids FROM operations WHERE subject=?1 AND request_key=?2",params![identity.subject,key],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,String>(3)?,row.get::<_,Option<String>>(4)?,row.get::<_,String>(5)?))).optional().map_err(|_|ServiceError::internal())?.ok_or_else(||ServiceError::new("not_found","Operation not found.","Use the same identity and original idempotency key."))?;
        let account = row
            .2
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| ServiceError::internal())?;
        let tool = crate::catalog::find(&row.1)?;
        identity.check(tool.scope, account)?;
        let accounts: BTreeSet<Uuid> =
            serde_json::from_str(&row.5).map_err(|_| ServiceError::internal())?;
        for account in &accounts {
            identity.check(tool.scope, Some(*account))?;
        }
        if !tool.account_required && tool.scope == "trust:admin" && !identity.grant.all_accounts {
            return Err(ServiceError::forbidden());
        }
        let response = row
            .4
            .as_deref()
            .map(serde_json::from_str::<Value>)
            .transpose()
            .map_err(|_| ServiceError::internal())?;
        let resolution = self.resolution(&row.0)?;
        Ok(
            json!({"operation_id":row.0,"tool":row.1,"account_id":account,"account_ids":accounts,"state":row.3,"result":response,"resolution":resolution}),
        )
    }
}

fn verify_chain(connection: &Connection) -> Result<Value> {
    let mut stmt = connection
        .prepare("SELECT event,previous_hash,event_hash FROM audit ORDER BY sequence")
        .map_err(|_| ServiceError::internal())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| ServiceError::internal())?;
    let mut previous = "GENESIS".to_owned();
    let mut count = 0u64;
    for row in rows {
        let (event, stored_previous, hash) = row.map_err(|_| ServiceError::internal())?;
        if previous != stored_previous || chain_hash(&previous, &event)? != hash {
            return Err(ServiceError::new(
                "audit_integrity_failed",
                "The service audit chain is inconsistent.",
                "Stop mutations and restore trusted evidence before restarting.",
            ));
        }
        previous = hash;
        count = count.saturating_add(1);
    }
    Ok(json!({"events":count,"head_hash":previous}))
}

fn chain_hash(previous: &str, event: &str) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(previous.as_bytes());
    hash.update(b"\n");
    hash.update(event.as_bytes());
    encode_hash(hash.finalize().as_ref())
}

#[allow(clippy::too_many_arguments)]
fn append(
    connection: &Connection,
    identity: &Identity,
    request: Uuid,
    operation: Option<Uuid>,
    tool: &str,
    account: Option<Uuid>,
    outcome: &str,
    content_hash: Option<&str>,
    context: &AuditContext,
) -> Result<()> {
    let previous = connection
        .query_row(
            "SELECT event_hash FROM audit ORDER BY sequence DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| ServiceError::internal())?
        .unwrap_or_else(|| "GENESIS".into());
    let event=serde_json::to_string(&json!({"version":2,"source":context.source,"queue_wait_ms":context.queue_wait_ms,"duration_ms":context.duration_ms,"error_code":context.error_code,"account_ids":context.account_ids,"at":chrono::Utc::now().to_rfc3339(),"subject":identity.subject,"actor_role":if context.source == "recovery" {None} else {Some(identity.grant.role)},"owner_authenticated_until":identity.owner_authenticated_until,"client_id":identity.client_id,"request_id":request,"operation_id":operation,"tool":tool,"account_id":account,"outcome":outcome,"content_hash":content_hash})).map_err(|_|ServiceError::internal())?;
    let hash = chain_hash(&previous, &event)?;
    connection
        .execute(
            "INSERT INTO audit(event,previous_hash,event_hash) VALUES (?1,?2,?3)",
            params![event, previous, hash],
        )
        .map_err(|_| ServiceError::internal())?;
    Ok(())
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn existing_operation_and_audit_hashes_survive_dependency_upgrades(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // Frozen outputs from sha2 0.10.9 and serde_json 1.0.149.
        let value =
            json!({"tool":"capital_deposit","arguments":{"amount":"10.25","currency":"usd"}});
        assert_eq!(
            digest(&value)?,
            "a3a04d3da5d38b0bcc9670fdd753636eb645bd0f3ce5b67c3fc077f218a176df"
        );
        assert_eq!(
            chain_hash("GENESIS", &serde_json::to_string(&value)?)?,
            "d4fa4e538707847da2451cd3c33b8b8d0766708e70cc42785837249df5227646"
        );
        Ok(())
    }

    #[test]
    fn verification_reuse_is_confined_to_one_consistent_sqlite_snapshot(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("control.db");
        let journal = Journal::open(&path)?;
        let writer = Connection::open(&path)?;
        let event = "{\"version\":2}";
        writer.execute(
            "INSERT INTO audit(event,previous_hash,event_hash) VALUES(?1,'GENESIS',?2)",
            params![event, chain_hash("GENESIS", event)?],
        )?;
        journal.read(|snapshot| {
            snapshot.verify_audit()?;
            // WAL permits another connection to commit while this read transaction
            // retains its original evidence. That commit must not leak into the read.
            writer
                .execute_batch("DROP TRIGGER audit_no_update; UPDATE audit SET event='{}';")
                .map_err(|_| ServiceError::internal())?;
            snapshot.verify_audit()?;
            let visible: String = snapshot
                .connection
                .query_row("SELECT event FROM audit", [], |row| row.get(0))
                .map_err(|_| ServiceError::internal())?;
            assert_eq!(visible, event);
            Ok(())
        })?;
        assert_eq!(
            journal
                .read(|snapshot| snapshot.verify_audit())
                .err()
                .map(|error| error.code),
            Some("audit_integrity_failed".into())
        );
        assert_eq!(
            journal.verify().err().map(|error| error.code),
            Some("audit_integrity_failed".into())
        );
        Ok(())
    }
}
