# Owner handoff from an agent session

Use this reference when the user asks an agent to change limits, configure a mandate or move capital. It remains available when `trust-admin` and owner tools are hidden. Do not attempt hidden tools or read the admin skill just to verify a permission boundary already established by `policy_get`.

1. Read `service_status` and `policy_get` to establish this subject's role and scopes. Select the exact account/environment. Read `account_safety_policy_get` and `agent_mandate_get` for the actual current values; account and agent limits are distinct.
2. Explain the requested change as a before/after comparison with explicit currency and units. A new value can be lower than the current one even if the user says “increase.” Clarify ambiguity between per-order and aggregate limits, or an unintended reduction, before an owner changes anything. Preserve unrelated loss limits, instruments, direction, expiry and account bindings.
3. Give the human the account UUID, broker/environment, agent subject, old/requested values, current blockers and missing decisions. The owner uses a separate authenticated session with recent human MFA and the admin workflow. Do not ask for an owner token, password, passkey material or broker key in this session. The agent's credential remains unchanged.
4. State what has and has not happened. No policy change, deposit or trade occurred from this handoff. Higher limits do not add cash or authorize a new trade. Read back policy and readiness after independently confirmed owner changes; refresh only when authorized.

Trust MCP does not transfer cash into or out of a brokerage. Broker/bank funding takes place in the human's authorized broker/bank workflow. Trust capital deposit, withdrawal and transfer tools record ledger accounting; a separately evidenced external cash-flow record adjusts the equity baseline. Neither substitutes for a broker transfer or forgives trading losses. Do not present a paper/simulator balance adjustment as a real deposit.

Suggested handoff form:

> Account [UUID], [broker/environment], currency [code], agent [subject]. Current [specific limit] is [amount]; requested value is [amount]. This is an [increase/decrease]. Other relevant caps/blockers: [observed facts]. Needed owner decision: [ambiguity or exact change]. No mutation was performed. Complete the change in the separate owner MFA session; handle any broker funding directly, then verify the resulting policy and broker evidence.

If the owner request follows an unknown trade outcome, preserve the halt and original key and use operations recovery first. Raising a limit, signing in again or claiming that funds arrived cannot resolve uncertain exposure.
