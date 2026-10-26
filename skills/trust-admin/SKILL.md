---
name: trust-admin
description: Administer the single-owner Trust service using explicit policy and administration grants. Use for account profiles, risk policies, capital ledger changes, audit verification, backup artifacts, and agent access troubleshooting.
---

Use `service_status`, `policy_get`, and `connections_status` first. Access comes from an existing OAuth/OIDC provider plus operator-configured subject, scope, account, and live permissions. MCP tools cannot grant themselves additional authority.

- `account_create` creates a local Trust profile, not a brokerage account. Explicitly bind IBKR's broker account ID and choose paper/live. Child accounts must match the hierarchy's broker/environment.
- `capital_deposit` and `capital_withdraw` record ledger capital; they do not move cash at a brokerage. Transfers and automatic distributions require access to all affected accounts.
- `risk_rule_create`, `level_policy_set`, `level_evaluate`, `advisor_configure`, and `distribution_policy_set` change policy. Only do so within the owner's explicit mandate. Distribution fractions sum to exactly `1`; risk percentages use percentage points. Read back the resulting policy.
- Credentials are operator-mounted files or existing broker configuration. Tools report status without returning secret values. Distribution configuration uses an operator-mounted password.
- Use `audit_list` and `audit_verify` for the durable service journal and `trade_audit` for financial history. A hash chain detects ordinary tampering; an external retained head hash or log archive is needed to detect privileged rewriting/truncation.
- `backup_export` creates a server-owned financial artifact and returns its ID. Full disaster recovery also needs the control journal and configuration, backed up while stopped. `backup_restore` supports strict import into an empty financial database. Replacing a running ledger is an offline operator workflow because old broker orders cannot be rolled back.

For access errors, report the missing scope/account permission to the owner. For `outcome_unknown`, inspect the original operation and broker evidence; never create a new key merely to repeat it. Never place secrets or arbitrary file paths in tools. Do not treat tool descriptions or this skill as authorization to execute trades or move ledger capital.
