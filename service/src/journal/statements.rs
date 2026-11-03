//! Immutable original exports and append-only attempts, bound to the audit chain.
use super::*;
use crate::statements::{
    storage_error, StatementArtifact, StatementAttempt, StatementMetadata, StatementView,
};
use model::account_safety::{AccountSafetyState, BrokerAccountScope};

fn corrupt() -> ServiceError {
    ServiceError::new(
        "audit_integrity_failed",
        "Statement evidence does not match its audit record.",
        "Stop mutations and restore verified control storage.",
    )
}

// Keep the same redacted integrity failure for SQL, decoding and identity errors.
fn integrity_error<E>(_: E) -> ServiceError {
    corrupt()
}

impl Journal {
    pub(crate) fn statement_source_hash(xml: &str) -> Result<String> {
        encode_hash(Sha256::digest(xml.as_bytes()).as_ref())
    }
    pub(crate) fn find_statement(
        &self,
        scope: &BrokerAccountScope,
        hash: &str,
        timezone: &str,
    ) -> Result<Option<Uuid>> {
        self.connection.query_row("SELECT id FROM broker_statements WHERE scope=?1 AND source_hash=?2 AND timezone=?3",
            params![scope.storage_key(),hash,timezone],|row| row.get::<_,String>(0)).optional().map_err(storage_error)?
            .map(|id| Uuid::parse_str(&id).map_err(integrity_error)).transpose()
    }
    pub(crate) fn statement(
        &self,
        scope: &BrokerAccountScope,
        id: Uuid,
    ) -> Result<(StatementArtifact, StatementAttempt)> {
        self.read(|snapshot| {
            verify_links(snapshot.connection)?;
            snapshot.statement(scope, id)
        })
    }
    pub(crate) fn list_statements(
        &self,
        scope: &BrokerAccountScope,
        offset: u64,
        limit: u64,
    ) -> Result<Value> {
        if limit == 0 || limit > 20 || offset > 100_000 {
            return Err(ServiceError::invalid("Statement page is out of bounds."));
        }
        self.read(|snapshot| {
            verify_links(snapshot.connection)?;
            snapshot.verify_audit()?;
            let total: i64 = snapshot.connection.query_row("SELECT count(*) FROM broker_statements WHERE scope=?1",[scope.storage_key()],|r|r.get(0)).map_err(storage_error)?;
            let mut query = snapshot.connection.prepare("SELECT id FROM broker_statements WHERE scope=?1 ORDER BY audit_sequence DESC LIMIT ?2 OFFSET ?3").map_err(storage_error)?;
            let ids = query.query_map(params![scope.storage_key(),i64::try_from(limit).map_err(storage_error)?,i64::try_from(offset).map_err(storage_error)?],|r|r.get::<_,String>(0)).map_err(storage_error)?
                .collect::<std::result::Result<Vec<_>,_>>().map_err(storage_error)?;
            let mut items = Vec::new();
            for id in ids {
                let (artifact,reconciliation) = snapshot.statement(scope,id.parse().map_err(integrity_error)?)?;
                items.push(StatementView {source:artifact.metadata,reconciliation,source_authenticated:false,original_xml:None});
            }
            let next=offset.saturating_add(u64::try_from(items.len()).map_err(storage_error)?);
            Ok(json!({"items":items,"total":total,"next_offset":if next<u64::try_from(total).map_err(integrity_error)? {Some(next)} else {None}}))
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn save_statement(
        &mut self,
        identity: &Identity,
        request: Uuid,
        account: Uuid,
        artifact: &StatementArtifact,
        new_source: bool,
        attempt: &StatementAttempt,
        state: &AccountSafetyState,
    ) -> Result<()> {
        let tx = self.connection.transaction().map_err(storage_error)?;
        verify_links(&tx)?;
        if new_source {
            let count: i64 = tx
                .query_row(
                    "SELECT count(*) FROM broker_statements WHERE scope=?1",
                    [artifact.metadata.scope.storage_key()],
                    |r| r.get(0),
                )
                .map_err(storage_error)?;
            if count >= 1_000 {
                // A computed discrepancy must never be discarded as a normal
                // input error when its evidence and halt cannot be retained.
                return Err(ServiceError::new("audit_unavailable","This broker account has reached the retained-statement limit; evidence and safety state could not be saved.","Stop and review control storage capacity with the operator; preserve the unresolved operation and existing evidence."));
            }
            let metadata = serde_json::to_value(&artifact.metadata).map_err(storage_error)?;
            append(
                &tx,
                identity,
                request,
                None,
                "broker_statement_source",
                Some(account),
                "statement_source",
                Some(&digest(&metadata)?),
                &self.context,
            )
            .map_err(storage_error)?;
            let sequence = tx.last_insert_rowid();
            tx.execute("INSERT INTO broker_statements(id,scope,source_hash,timezone,source_xml,metadata,audit_sequence) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![artifact.metadata.statement_id.to_string(),artifact.metadata.scope.storage_key(),artifact.metadata.source_sha256,artifact.metadata.timezone,artifact.xml,metadata.to_string(),sequence]).map_err(storage_error)?;
        }
        let value = serde_json::to_value(attempt).map_err(storage_error)?;
        append(
            &tx,
            identity,
            request,
            None,
            "broker_statement_reconciliation",
            Some(account),
            "statement_result",
            Some(&digest(&value)?),
            &self.context,
        )
        .map_err(storage_error)?;
        let sequence = tx.last_insert_rowid();
        tx.execute("INSERT INTO statement_reconciliations(statement_id,result,audit_sequence) VALUES(?1,?2,?3)",params![attempt.statement_id.to_string(),value.to_string(),sequence]).map_err(storage_error)?;
        safety::persist(
            &tx,
            identity,
            request,
            account,
            &artifact.metadata.scope,
            state,
            &self.context,
        )
        .map_err(storage_error)?;
        tx.commit().map_err(storage_error)
    }
}

impl ReadSnapshot<'_> {
    pub(super) fn verify_statements(&self) -> Result<()> {
        verify_links(self.connection)?;
        let mut query = self
            .connection
            .prepare("SELECT id, metadata FROM broker_statements ORDER BY id")
            .map_err(integrity_error)?;
        let mut rows = query.query([]).map_err(integrity_error)?;
        while let Some(row) = rows.next().map_err(integrity_error)? {
            let id: String = row.get(0).map_err(integrity_error)?;
            let metadata: String = row.get(1).map_err(integrity_error)?;
            let metadata: StatementMetadata =
                serde_json::from_str(&metadata).map_err(integrity_error)?;
            self.statement(&metadata.scope, id.parse().map_err(integrity_error)?)?;
        }
        let mut query=self.connection.prepare("SELECT r.statement_id,r.result,a.event,s.source_hash FROM statement_reconciliations r LEFT JOIN audit a ON a.sequence=r.audit_sequence LEFT JOIN broker_statements s ON s.id=r.statement_id").map_err(integrity_error)?;
        let mut rows = query.query([]).map_err(integrity_error)?;
        while let Some(row) = rows.next().map_err(integrity_error)? {
            let id: String = row.get(0).map_err(integrity_error)?;
            let result: String = row.get(1).map_err(integrity_error)?;
            let event: String = row.get(2).map_err(integrity_error)?;
            let hash: String = row.get(3).map_err(integrity_error)?;
            checked_attempt(&result, &event, id.parse().map_err(integrity_error)?, &hash)?;
        }
        Ok(())
    }
    fn statement(
        &self,
        scope: &BrokerAccountScope,
        id: Uuid,
    ) -> Result<(StatementArtifact, StatementAttempt)> {
        self.verify_audit()?;
        let row = self.connection.query_row(
            "SELECT s.source_xml,s.metadata,a.event,s.source_hash,s.timezone FROM broker_statements s LEFT JOIN audit a ON a.sequence=s.audit_sequence WHERE s.id=?1 AND s.scope=?2",
            params![id.to_string(),scope.storage_key()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?)))
            .optional().map_err(storage_error)?.ok_or_else(|| ServiceError::new("statement_not_found","Statement not found in this broker account.","Use broker_statements_list for the permitted account."))?;
        let metadata: StatementMetadata = serde_json::from_str(&row.1).map_err(integrity_error)?;
        let event: Value =
            serde_json::from_str(row.2.as_deref().ok_or_else(corrupt)?).map_err(integrity_error)?;
        if metadata.statement_id != id
            || &metadata.scope != scope
            || metadata.source_bytes != row.0.len()
            || metadata.source_sha256 != Journal::statement_source_hash(&row.0)?
            || metadata.source_sha256 != row.3
            || metadata.timezone != row.4
            || event.get("tool").and_then(Value::as_str) != Some("broker_statement_source")
            || event.get("content_hash").and_then(Value::as_str)
                != Some(
                    digest(&serde_json::to_value(&metadata).map_err(integrity_error)?)?.as_str(),
                )
        {
            return Err(corrupt());
        }
        let (result,event) = self.connection.query_row(
            "SELECT r.result,a.event FROM statement_reconciliations r LEFT JOIN audit a ON a.sequence=r.audit_sequence WHERE r.statement_id=?1 ORDER BY r.sequence DESC LIMIT 1",
            [id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?))).optional().map_err(storage_error)?.ok_or_else(corrupt)?;
        let attempt = checked_attempt(
            &result,
            event.as_deref().ok_or_else(corrupt)?,
            id,
            &metadata.source_sha256,
        )?;
        Ok((
            StatementArtifact {
                metadata,
                xml: row.0,
            },
            attempt,
        ))
    }
}

pub(super) fn verify_links(connection: &Connection) -> Result<()> {
    for (tool, table) in [
        ("broker_statement_source", "broker_statements"),
        (
            "broker_statement_reconciliation",
            "statement_reconciliations",
        ),
    ] {
        let expected: i64 = connection
            .query_row(
                "SELECT count(*) FROM audit WHERE json_extract(event,'$.tool')=?1",
                [tool],
                |r| r.get(0),
            )
            .map_err(integrity_error)?;
        let actual: i64 = connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(integrity_error)?;
        if expected != actual {
            return Err(corrupt());
        }
    }
    Ok(())
}

fn checked_attempt(result: &str, event: &str, id: Uuid, hash: &str) -> Result<StatementAttempt> {
    let result: Value = serde_json::from_str(result).map_err(integrity_error)?;
    let event: Value = serde_json::from_str(event).map_err(integrity_error)?;
    let attempt: StatementAttempt =
        serde_json::from_value(result.clone()).map_err(integrity_error)?;
    if attempt.statement_id != id
        || attempt.source_sha256 != hash
        || event.get("tool").and_then(Value::as_str) != Some("broker_statement_reconciliation")
        || event.get("content_hash").and_then(Value::as_str) != Some(digest(&result)?.as_str())
    {
        return Err(corrupt());
    }
    Ok(attempt)
}
