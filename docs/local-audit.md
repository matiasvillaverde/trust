# Local audit and reconciliation

Trust records broker-facing trade mutations as structured, SHA-256 hash-chained events. The local journal is designed for agents first: commands are non-interactive, JSON is the default, identifiers and error codes are stable, and exports contain no prompts or decorative output.

## Identity and correlation

Set these variables when an agent or automation invokes Trust:

```bash
export TRUST_AUDIT_ACTOR="portfolio-agent"
export TRUST_AUDIT_REQUEST_ID="job-2026-08-20T12:00:00Z-001"
```

`TRUST_AUDIT_ACTOR` falls back to the local OS user. The request ID falls back to the operation ID. Values are stripped of control characters and bounded to 128 characters. Secrets, credentials, command-line arguments, and raw broker payloads are not included in structured event details.

## Verify, inspect, and export

```bash
trust trade audit --trade-id TRADE_UUID
trust trade audit --trade-id TRADE_UUID --format text
trust trade audit --trade-id TRADE_UUID --output audit-TRADE_UUID.json
```

JSON output contains a schema version, verification result, chain-head hash, and events in verified chain order. Verification fails on altered hashes, duplicates, forks, cycles, or disconnected events. Export uses create-new semantics and refuses to overwrite an existing file.

## Reconcile broker state

```bash
trust trade reconcile --trade-id TRADE_UUID --format json
trust trade reconcile --account ACCOUNT --format json
```

Reconciliation uses the existing broker-aware facade path and persists reconciled financial state atomically. The audit event stores a digest of the broker response instead of the potentially sensitive raw payload.

## Local security boundary

The journal is tamper-evident, not tamper-proof. Anyone able to replace the database and every exported checkpoint can rewrite local history. Stronger deployment should periodically copy verified exports and chain-head hashes to independently controlled immutable storage. This local implementation intentionally does not emulate WORM storage, invent a daemon, or introduce a second logging stack.

Current structured coverage focuses on broker-facing trade operations: submit, cancel, modify, close, and successful reconciliation. Transactions, distributions, level changes, executions, sessions, and trade events already have dedicated immutable domain records, but are not yet members of this single hash chain.
