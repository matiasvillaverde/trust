use super::*;

pub fn readiness_blockers(
    state: &AccountSafetyState,
    scope: &BrokerAccountScope,
    trades: &[Trade],
    now: DateTime<Utc>,
) -> BTreeSet<String> {
    let mut blockers = BTreeSet::new();
    if !state.unreconciled_statements.is_empty() {
        blockers.insert("statement_reconciliation_unresolved".into());
    }
    if let Some(halt) = &state.halt {
        blockers.extend(halt.reasons.clone());
        blockers.insert("account_halted".into());
    }
    if !state.unresolved_operations.is_empty() {
        blockers.insert("unresolved_operations".into());
    }
    let check = (|| {
        let policy = state
            .policy
            .as_ref()
            .ok_or_else(|| failure("safety_policy_missing"))?;
        validate_policy(policy)?;
        let snapshot = state
            .snapshot
            .as_ref()
            .ok_or_else(|| failure("broker_snapshot_missing"))?;
        validate_snapshot(snapshot, scope, policy, now)?;
        check_equity(state, snapshot)?;
        if gross_commitment(snapshot, trades, None)? > policy.max_gross_notional {
            return Err(failure("account_exposure_exceeded"));
        }
        verify_reconciled(snapshot, trades)?;
        Ok(())
    })();
    if let Err(error) = check {
        blockers.insert(error.code);
    }
    blockers
}
