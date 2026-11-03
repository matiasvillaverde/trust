//! Fair background polling of open trades and terminal trades with late activity.
use super::*;

#[derive(Default)]
pub(super) struct ReconcileCursor {
    active: Option<(Uuid, Uuid)>,
    terminal: Option<(Uuid, Uuid)>,
    terminal_turn: bool,
}

pub(super) fn reconcile(
    domain: &mut Domain,
    journal: &mut Journal,
    cursor: &mut ReconcileCursor,
    safety_cursor: &mut Option<Uuid>,
    metrics: &Metrics,
) -> Result<()> {
    if let Some(account) = crate::safety::background_account(domain, journal, *safety_cursor)? {
        *safety_cursor = Some(account);
        let scopes = std::collections::BTreeSet::from(["trust:reconcile".to_owned()]);
        let identity = Identity {
            subject: "system:reconciler".into(),
            client_id: None,
            expires_at: u64::MAX,
            owner_authenticated_until: None,
            scopes: scopes.clone(),
            grant: Grant {
                role: crate::auth::Role::Agent,
                accounts: Default::default(),
                all_accounts: true,
                scopes,
                allow_live: domain.config.allow_live,
            },
        };
        let result = execute(
            domain,
            journal,
            &identity,
            "account_refresh",
            &json!({"account_id":account,"idempotency_key":Uuid::new_v4().to_string()}),
            Uuid::new_v4(),
            Observation {
                source: Source::Background,
                wait: Duration::ZERO,
                metrics,
            },
        );
        if let Err(error) = result {
            if matches!(
                error.code.as_str(),
                "audit_unavailable" | "audit_integrity_failed"
            ) {
                return Err(error);
            }
        }
    }
    let accounts = match domain.facade.search_all_accounts() {
        Ok(accounts) => accounts,
        Err(_) => {
            tracing::warn!(
                event = "reconciliation_failed",
                "Cannot read accounts for reconciliation"
            );
            return Ok(());
        }
    };
    let scopes: std::collections::BTreeSet<_> =
        ["trust:reconcile".to_owned()].into_iter().collect();
    let identity = Identity {
        subject: "system:reconciler".into(),
        client_id: None,
        expires_at: u64::MAX,
        owner_authenticated_until: None,
        scopes: scopes.clone(),
        grant: Grant {
            role: crate::auth::Role::Agent,
            accounts: Default::default(),
            all_accounts: true,
            scopes,
            allow_live: domain.config.allow_live,
        },
    };
    let mut active = Vec::new();
    let mut terminal = Vec::new();
    for account in accounts {
        if account.environment == model::Environment::Live && !domain.config.allow_live {
            continue;
        }
        match domain.facade.account_trades(account.id) {
            Ok(trades) => {
                for trade in trades {
                    if domain::active(trade.status) {
                        active.push((account.id, trade.id));
                    } else if domain
                        .facade
                        .terminal_broker_reconciliation_due(&trade, chrono::Utc::now())
                        .map_err(domain::domain_error)?
                    {
                        terminal.push((account.id, trade.id));
                    }
                }
            }
            Err(_) => {
                tracing::warn!(event="reconciliation_failed",account_id=%account.id,"Cannot enumerate trades")
            }
        }
    }
    cursor.terminal_turn = !cursor.terminal_turn;
    let use_terminal = !terminal.is_empty() && (cursor.terminal_turn || active.is_empty());
    let (mut candidates, selected_cursor) = if use_terminal {
        (terminal, &mut cursor.terminal)
    } else {
        (active, &mut cursor.active)
    };
    candidates.sort_unstable();
    let next = candidates
        .iter()
        .find(|id| selected_cursor.is_none_or(|last| **id > last))
        .or_else(|| candidates.first())
        .copied();
    if let Some((account, trade)) = next {
        *selected_cursor = Some((account, trade));
        let args = json!({"account_id":account,"trade_id":trade,"idempotency_key":Uuid::new_v4().to_string()});
        metrics::increment(&metrics.active, 1);
        let result = execute(
            domain,
            journal,
            &identity,
            "trade_reconcile",
            &args,
            Uuid::new_v4(),
            Observation {
                source: Source::Background,
                wait: Duration::ZERO,
                metrics,
            },
        );
        metrics::decrement(&metrics.active);
        match result {
            Err(error) if error.code == "audit_unavailable" => return Err(error),
            Err(_) => {
                tracing::warn!(event="reconciliation_failed",account_id=%account,trade_id=%trade,"Background reconciliation failed")
            }
            Ok(reply) if reply.error.is_some() => {
                tracing::warn!(event="reconciliation_failed",account_id=%account,trade_id=%trade,"Background reconciliation did not confirm an outcome")
            }
            Ok(_) => {}
        }
    }
    Ok(())
}
