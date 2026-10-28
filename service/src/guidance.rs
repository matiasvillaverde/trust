//! Suggestions are derived from current permissions and persisted state, never authority.
use crate::{
    auth::Identity,
    catalog::{self, Operation},
    domain::Domain,
    journal::{Journal, Reply},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NextAction {
    pub tool: String,
    pub arguments: BTreeMap<String, String>,
    pub required_arguments: Vec<String>,
    pub available: bool,
    pub blockers: Vec<String>,
}

pub fn actions(
    domain: &mut Domain,
    journal: &Journal,
    identity: &Identity,
    op: Operation,
    args: &Value,
    reply: &Reply,
) -> Vec<NextAction> {
    let account = args
        .get("account_id")
        .and_then(Value::as_str)
        .and_then(|id| id.parse::<Uuid>().ok())
        .or_else(|| {
            if op == Operation::OperationGet {
                reply
                    .data
                    .as_ref()?
                    .get("account_id")?
                    .as_str()?
                    .parse()
                    .ok()
            } else {
                None
            }
        });
    let trade_id = args
        .get("trade_id")
        .and_then(Value::as_str)
        .and_then(|id| id.parse().ok())
        .or_else(|| {
            if op == Operation::TradeCreate {
                reply.data.as_ref()?.get("id")?.as_str()?.parse().ok()
            } else {
                None
            }
        });
    let mut candidates = vec![];
    if reply.error.is_some() {
        if args.get("idempotency_key").is_some() {
            candidates.push(("operation_get", vec![]));
        }
        if account.is_some() {
            candidates.push(("account_readiness", vec![]));
        }
        if trade_id.is_some() {
            candidates.push(("trade_get", vec![]));
            candidates.push(("trade_reconcile", vec![]));
        }
    } else if let Some(account) = account {
        let record = domain.facade.account_by_id(account).ok();
        let trade = trade_id.and_then(|id| domain.facade.read_trade(id).ok());
        candidates.push(("account_readiness", vec![]));
        candidates.push(("risk_check", vec![]));
        if op == Operation::RiskCheck
            && reply.data.as_ref().is_some_and(|d| {
                d.get("requested_admissible").and_then(Value::as_bool) == Some(true)
            })
        {
            candidates.push(("trade_prepare", vec![]));
        }
        if let Some(trade) = trade.filter(|t| t.account_id == account) {
            use model::Status;
            match trade.status {
                Status::New | Status::Funded => {
                    let blockers =
                        crate::safety::intent_blockers(domain, journal, identity, account, &trade);
                    if record.is_some_and(|a| a.broker_kind == model::BrokerKind::Ibkr) {
                        candidates.push(("broker_preview", vec![]));
                    }
                    candidates.push((
                        if trade.status == Status::New {
                            "trade_fund"
                        } else {
                            "trade_submit"
                        },
                        blockers,
                    ));
                    if trade.status == Status::Funded {
                        candidates.push(("trade_cancel", vec![]));
                    }
                }
                Status::Submitted | Status::PartiallyFilled | Status::Filled => {
                    candidates.push(("trade_reconcile", vec![]));
                    if trade.status == Status::Filled {
                        candidates.push(("trade_close", vec![]));
                        candidates.push(("trade_modify_stop", vec![]));
                    } else {
                        candidates.push(("trade_cancel", vec![]));
                    }
                }
                _ => {
                    candidates.push(("executions_list", vec![]));
                    candidates.push(("trade_audit", vec![]));
                }
            }
        } else {
            candidates.push(("account_refresh", vec![]));
            if op == Operation::OperationGet
                && reply
                    .data
                    .as_ref()
                    .and_then(|v| v.get("state"))
                    .and_then(Value::as_str)
                    == Some("outcome_unknown")
            {
                candidates.push(("account_reconcile", vec![]));
            }
            if reply
                .data
                .as_ref()
                .and_then(|v| v.get("halted"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                candidates.push(("account_reconcile", vec![]));
                candidates.push(("account_resume", vec!["verified_recovery_required".into()]));
            }
        }
    } else {
        candidates.push(("accounts_list", vec![]));
        candidates.push(("policy_get", vec![]));
    }
    candidates
        .into_iter()
        .filter_map(|(name, blockers)| {
            let spec = catalog::find(name).ok()?;
            if identity
                .check(
                    spec.scope,
                    if spec.account_required { account } else { None },
                )
                .is_err()
            {
                return None;
            }
            if spec.account_required && account.is_none() {
                return None;
            }
            let mut arguments = BTreeMap::new();
            if spec.account_required {
                arguments.insert("account_id".into(), account?.to_string());
            }
            if spec.input_schema.pointer("/properties/trade_id").is_some() {
                arguments.insert("trade_id".into(), trade_id?.to_string());
            }
            if matches!(name, "risk_check" | "trade_prepare") {
                for key in [
                    "instrument_id",
                    "side",
                    "quantity",
                    "entry",
                    "stop",
                    "target",
                    "currency",
                ] {
                    if let Some(value) = args.get(key).and_then(Value::as_str) {
                        arguments.insert(key.into(), value.into());
                    }
                }
            }
            if name == "operation_get" {
                arguments.insert(
                    "key".into(),
                    args.get("idempotency_key")?.as_str()?.to_owned(),
                );
            }
            let required_arguments = spec
                .input_schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|key| !arguments.contains_key(*key))
                .map(str::to_owned)
                .collect();
            Some(NextAction {
                tool: name.into(),
                arguments,
                required_arguments,
                available: blockers.is_empty(),
                blockers,
            })
        })
        .collect()
}
