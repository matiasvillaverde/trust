//! Owner workflow for independent reports. No broker or financial ledger writes.
use crate::{
    auth::Identity,
    catalog::Operation,
    domain::{self, Domain},
    error::{Result, ServiceError},
    journal::Journal,
};
use chrono::{DateTime, NaiveDate, Utc};
use model::{
    account_safety::BrokerAccountScope,
    statement::{BrokerStatement, ReconciliationStatus, StatementReconciliation},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementMetadata {
    pub statement_id: Uuid,
    pub scope: BrokerAccountScope,
    pub source_sha256: String,
    pub source_bytes: usize,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub timezone: String,
    pub imported_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementAttempt {
    pub attempt_id: Uuid,
    pub statement_id: Uuid,
    pub source_sha256: String,
    pub result: StatementReconciliation,
    pub supersedes_statement_id: Option<Uuid>,
}

#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatementView {
    pub source: StatementMetadata,
    pub reconciliation: StatementAttempt,
    pub source_authenticated: bool,
    pub original_xml: Option<String>,
}

pub(crate) struct StatementArtifact {
    pub metadata: StatementMetadata,
    pub xml: String,
}

pub fn handles(op: Operation) -> bool {
    matches!(
        op,
        Operation::StatementImport
            | Operation::StatementGet
            | Operation::StatementList
            | Operation::StatementReconcile
    )
}
fn invalid(code: &str) -> ServiceError {
    ServiceError::new(code,"The report cannot be used for the requested reconciliation.","Check the documented Flex query fields, complete period, exact account and report timezone. Import original exports only.")
}
pub(crate) fn storage_error<E>(_: E) -> ServiceError {
    ServiceError::new(
        "audit_unavailable",
        "Statement evidence and safety state could not be saved atomically.",
        "Stop and inspect control storage and the original operation before retrying.",
    )
}
fn parse(xml: &str, scope: &BrokerAccountScope, timezone: &str) -> Result<BrokerStatement> {
    ibkr_broker::statements::parse_activity_xml(xml, &scope.account_id, timezone, Utc::now())
        .map_err(|e| invalid(e.0))
}
fn affected(domain: &mut Domain, scope: &BrokerAccountScope) -> Result<BTreeSet<Uuid>> {
    Ok(domain
        .facade
        .search_all_accounts()
        .map_err(domain::domain_error)?
        .iter()
        .filter(|a| BrokerAccountScope::from_account(a).ok().as_ref() == Some(scope))
        .map(|a| a.id)
        .collect())
}

pub fn dispatch(
    domain: &mut Domain,
    journal: &mut Journal,
    identity: &Identity,
    op: Operation,
    args: &Value,
    request: Uuid,
) -> Result<Value> {
    let account_id = domain::id(args, "account_id")?;
    let account = domain
        .facade
        .account_by_id(account_id)
        .map_err(domain::domain_error)?;
    let scope = BrokerAccountScope::from_account(&account)
        .map_err(|_| invalid("broker_account_unbound"))?;
    if scope.broker != "ibkr" {
        return Err(invalid("statement_broker_unsupported"));
    }
    let accounts = affected(domain, &scope)?;
    for account in &accounts {
        identity.check("trust:policy", Some(*account))?;
    }
    if op == Operation::StatementList {
        return journal.list_statements(
            &scope,
            args.get("offset").and_then(Value::as_u64).unwrap_or(0),
            args.get("limit").and_then(Value::as_u64).unwrap_or(20),
        );
    }
    if op == Operation::StatementGet {
        let id = domain::id(args, "statement_id")?;
        let (artifact, attempt) = journal.statement(&scope, id)?;
        return serde_json::to_value(StatementView {
            source: artifact.metadata,
            reconciliation: attempt,
            source_authenticated: false,
            original_xml: if args.get("include_source").and_then(Value::as_bool) == Some(true) {
                Some(artifact.xml)
            } else {
                None
            },
        })
        .map_err(|_| ServiceError::internal());
    }
    let (artifact, statement, new_source) = source(journal, &scope, op, args)?;
    let mut state = journal.account_safety(&scope, &accounts)?;
    let supersedes = args
        .get("supersedes_statement_id")
        .and_then(Value::as_str)
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| invalid("invalid_statement_identity"))?;
    if let Some(previous) = supersedes {
        let (old, _) = journal.statement(&scope, previous)?;
        if previous == artifact.metadata.statement_id
            || !state.unreconciled_statements.contains(&previous)
            || old.metadata.from_date != artifact.metadata.from_date
            || old.metadata.to_date != artifact.metadata.to_date
            || old.metadata.timezone != artifact.metadata.timezone
        {
            return Err(invalid("statement_supersession_mismatch"));
        }
    }
    let result = domain
        .facade
        .reconcile_broker_statement(&scope, &statement)
        .unwrap_or_else(|_| {
            StatementReconciliation::inconclusive("trust_ledger_evidence_unavailable")
        });
    let matched = result.status == ReconciliationStatus::Matched;
    if matched {
        state
            .unreconciled_statements
            .remove(&artifact.metadata.statement_id);
        if let Some(previous) = supersedes {
            state.unreconciled_statements.remove(&previous);
        }
    } else {
        if state.unreconciled_statements.len() >= 256
            && !state
                .unreconciled_statements
                .contains(&artifact.metadata.statement_id)
        {
            return Err(invalid("statement_issue_limit"));
        }
        state
            .unreconciled_statements
            .insert(artifact.metadata.statement_id);
        state.halt("statement_reconciliation_unresolved", Utc::now());
    }
    let attempt = StatementAttempt {
        attempt_id: Uuid::new_v4(),
        statement_id: artifact.metadata.statement_id,
        source_sha256: artifact.metadata.source_sha256.clone(),
        result,
        supersedes_statement_id: if matched { supersedes } else { None },
    };
    journal.save_statement(
        identity, request, account_id, &artifact, new_source, &attempt, &state,
    )?;
    Ok(json!(StatementView {
        source: artifact.metadata,
        reconciliation: attempt,
        source_authenticated: false,
        original_xml: None
    }))
}

fn source(
    journal: &Journal,
    scope: &BrokerAccountScope,
    op: Operation,
    args: &Value,
) -> Result<(StatementArtifact, BrokerStatement, bool)> {
    if op == Operation::StatementReconcile {
        let (artifact, _) = journal.statement(scope, domain::id(args, "statement_id")?)?;
        let statement = parse(&artifact.xml, scope, &artifact.metadata.timezone)?;
        return Ok((artifact, statement, false));
    }
    let xml = domain::string(args, "xml")?;
    let timezone = domain::string(args, "timezone")?;
    let statement = parse(xml, scope, timezone)?;
    let hash = Journal::statement_source_hash(xml)?;
    if let Some(id) = journal.find_statement(scope, &hash, timezone)? {
        let (artifact, _) = journal.statement(scope, id)?;
        return Ok((artifact, statement, false));
    }
    Ok((
        StatementArtifact {
            metadata: StatementMetadata {
                statement_id: Uuid::new_v4(),
                scope: scope.clone(),
                source_sha256: hash,
                source_bytes: xml.len(),
                from_date: statement.from_date,
                to_date: statement.to_date,
                timezone: timezone.into(),
                imported_at: Utc::now(),
            },
            xml: xml.into(),
        },
        statement,
        true,
    ))
}
