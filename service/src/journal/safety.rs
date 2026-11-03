use super::*;

impl Journal {
    pub fn save_safety(
        &mut self,
        identity: &Identity,
        request: Uuid,
        account: Uuid,
        scope: &model::account_safety::BrokerAccountScope,
        state: &model::account_safety::AccountSafetyState,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction()
            .map_err(|_| ServiceError::internal())?;
        persist(&tx, identity, request, account, scope, state, &self.context)?;
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
}

#[allow(clippy::too_many_arguments)]
pub(super) fn persist(
    connection: &Connection,
    identity: &Identity,
    request: Uuid,
    account: Uuid,
    scope: &model::account_safety::BrokerAccountScope,
    state: &model::account_safety::AccountSafetyState,
    context: &AuditContext,
) -> Result<()> {
    let key = scope.storage_key();
    let value = serde_json::to_value(state).map_err(|_| ServiceError::internal())?;
    let hash = digest(&json!({"scope":key,"state":value}))?;
    let encoded = serde_json::to_string(&value).map_err(|_| ServiceError::internal())?;
    append(
        connection,
        identity,
        request,
        None,
        "account_safety",
        Some(account),
        "safety_state",
        Some(&hash),
        context,
    )?;
    let sequence = connection.last_insert_rowid();
    connection
        .execute(
            "INSERT INTO account_safety_history(audit_sequence,scope,state) VALUES(?1,?2,?3)",
            params![sequence, key, encoded],
        )
        .map_err(|_| ServiceError::internal())?;
    connection.execute("INSERT INTO account_safety(scope,state,audit_sequence) VALUES(?1,?2,?3) ON CONFLICT(scope) DO UPDATE SET state=excluded.state,audit_sequence=excluded.audit_sequence",params![key,encoded,sequence]).map_err(|_| ServiceError::internal())?;
    Ok(())
}
