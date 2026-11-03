//! Account readiness presentation, with owner-only evidence identifiers.
use super::*;

pub(super) fn view(
    domain: &mut Domain,
    identity: &Identity,
    account: &Account,
    scope: &BrokerAccountScope,
    state: &AccountSafetyState,
) -> Result<Value> {
    let all = trades(domain, account)?;
    let mut blockers = risk::readiness_blockers(state, scope, &all, Utc::now());
    let mut portfolio_report = match portfolio::check_existing(state, &all, Utc::now()) {
        Ok(report) => {
            if report.sizing_multiplier <= rust_decimal::Decimal::ZERO {
                blockers.insert("drawdown_blocks_new_risk".into());
            }
            if report.heat_remaining <= rust_decimal::Decimal::ZERO {
                blockers.insert("portfolio_heat_exhausted".into());
            }
            if report.stress_remaining <= rust_decimal::Decimal::ZERO {
                blockers.insert("portfolio_stress_exhausted".into());
            }
            Some(report)
        }
        Err(e) => {
            blockers.insert(e.code);
            None
        }
    };
    if identity.check("trust:trade", Some(account.id)).is_err() {
        blockers.insert("trade_permission_required".into());
    }
    if domain.facade.require_trading_policy(account.id).is_err() {
        blockers.insert("risk_policy_missing".into());
    }
    if domain
        .facade
        .search_rules(account.id)
        .map_err(domain::domain_error)?
        .iter()
        .any(|rule| {
            rule.name
                .risk_decimal()
                .is_ok_and(|amount| amount <= rust_decimal::Decimal::ZERO)
        })
    {
        blockers.insert("risk_policy_disables_trading".into());
    }
    if identity.grant.role == Role::Agent {
        match state.mandates.get(&identity.subject) {
            None => {
                blockers.insert("agent_mandate_missing".into());
            }
            Some(m) if m.expires_at <= Utc::now() => {
                blockers.insert("agent_mandate_expired".into());
            }
            Some(m) if m.risk_budget.is_none() => {
                blockers.insert("agent_risk_budget_missing".into());
            }
            _ => {}
        }
        if let Err(e) = portfolio::check_agent_capacity(state, &all, &identity.subject, Utc::now())
        {
            blockers.insert(e.code);
        }
    }
    let constraints = if blockers.is_empty() {
        match risk::remaining_capacity(
            state,
            &all,
            if identity.grant.role == Role::Agent {
                Some(identity.subject.as_str())
            } else {
                None
            },
        ) {
            Ok(capacity) => Some(capacity),
            Err(error) => {
                blockers.insert(error.code);
                None
            }
        }
    } else {
        None
    };
    let snapshot = state.snapshot.as_ref();
    // Agents see their admission status, not other agents' positions/balances/operations.
    let owner = identity.grant.role == Role::Owner
        && linked_accounts(domain, scope)?
            .iter()
            .all(|id| identity.permits_account(*id));
    if !owner {
        if let Some(report) = &mut portfolio_report {
            report.instrument_notional.clear();
            report.sector_notional.clear();
            report.cluster_notional.clear();
        }
    }
    Ok(json!({
        "account_id":account.id,"broker":account.broker_kind,"environment":account.environment,
        "ready_for_new_risk":blockers.is_empty(),"blockers":blockers,"halted":state.halt.is_some(),
        "halted_since":state.halt.as_ref().map(|h|h.since),"policy_configured":state.policy.is_some(),
        "observed_at":snapshot.map(|s|s.observed_at),"received_at":snapshot.map(|s|s.received_at),
        "snapshot_age_seconds":snapshot.map(|s|Utc::now().signed_duration_since(s.observed_at).num_seconds()),
        "snapshot_complete":snapshot.is_some_and(|s|s.complete&&s.consistent),
        "broker_connected":!state.snapshot_fetch_failed&&snapshot.zip(state.policy.as_ref()).is_some_and(|(s,p)|risk::validate_snapshot(s,scope,p,Utc::now()).is_ok()),
        "unresolved_operation_count":state.unresolved_operations.len(),
        "unreconciled_statement_count":state.unreconciled_statements.len(),
        "unreconciled_statement_ids":if owner {Some(&state.unreconciled_statements)} else {None},
        "unresolved_operation_ids":if owner {Some(&state.unresolved_operations)} else {None},
        "snapshot":if owner {snapshot} else {None},
        "mandate":state.mandates.get(&identity.subject),
        "constraints":constraints,
        "portfolio_risk":portfolio_report,
        "equity_baseline":if owner {state.baseline.as_ref()} else {None},
        "risk_reducing_actions_remain_available":true
    }))
}
