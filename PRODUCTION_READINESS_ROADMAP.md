# Trust CLI Production Readiness Roadmap

## Objective
Make `trust-cli` production-ready for three core outcomes:
1. Users can read complete market information.
2. Users can create/manage trades safely and reliably.
3. Users can investigate past performance with decision-grade analytics.

## Current Baseline (Implemented)
- Market data commands: `market-data snapshot|bars|stream|quote|trade|session` with text/json outputs.
- Trade lifecycle commands: `trade create|fund|submit|sync|watch|cancel|modify-*|manually-*|size-preview|search|list-open|reconcile`.
- Performance surface: `report performance|drawdown|risk|concentration|summary|metrics|attribution|benchmark|timeline`, plus `metrics advanced|compare` and `grade show|summary`.

## Status Update (2026-02-27)
- Completed: Phase 1.1 partial market expansion (`quote`, `trade`, `session`).
- Completed: Phase 1.2 automation surface (`trade search`, `trade list-open`, `trade reconcile`).
- Completed: Phase 1.3 report expansion (`report attribution`, `report benchmark`, `report timeline`).
- Remaining in Phase 1: `market-data orderbook`, `market-data corporate-actions`, `market-data news`.

## Gaps To Close
- Market data breadth is still limited to snapshot/bars/stream (no corporate actions/news/orderbook/options/fundamentals).
- Some flows are interactive-first and not ideal for agents/scripts.
- Reliability controls need hard operational guardrails (idempotency, replay, observability, SLOs).
- Contract stability for LLM consumption needs explicit schema governance and compatibility policy.

## Phase 1: Interface Completion (2-3 weeks)
### 1. Market Information Expansion
Add commands:
- `market-data quote --account --symbol`
- `market-data trade --account --symbol`
- `market-data orderbook --account --symbol [--depth]`
- `market-data corporate-actions --symbol [--from --to]`
- `market-data news --symbols --from --to [--limit]`
- `market-data session --symbol`

Acceptance criteria:
- All commands support `--format text|json`.
- JSON includes `schema_version`, `generated_at`, `source`, `scope`, `data`, `freshness`.
- Errors are structured with stable `code` values.

### 2. Trade Management Automation Surface
Add non-interactive alternatives:
- `trade search --account [--status --symbol --from --to --format json]`
- `trade list-open [--account --format json]`
- `trade reconcile [--account|--trade-id]`

Acceptance criteria:
- No user prompt required for automation paths.
- Idempotent behavior for repeated `fund/submit/sync/cancel/reconcile` requests.
- Clear terminal-safe output plus machine-friendly JSON output.

### 3. Performance Investigation Enhancements
Add commands:
- `report attribution --account --by symbol|sector|asset-class --from --to`
- `report benchmark --account --benchmark SPY --from --to`
- `report timeline --account --granularity day|week|month --from --to`

Acceptance criteria:
- Consistent metrics definitions across report commands.
- Cross-report consistency checks included in JSON payload.

## Phase 2: Reliability + Safety (2-4 weeks)
### 1. Data Reliability
- Add market stream reconnect/backoff/heartbeat.
- Add gap detection and optional backfill mode.
- Add staleness guardrails (`max_lag_seconds`) on market-data reads.

### 2. Execution Safety
- Enforce pre-trade validation gates in CLI paths:
  - tradability/session state
  - buying power / reserved capital
  - stop/target geometry
  - max-risk exposure checks
- Add idempotency keys for mutation operations where applicable.

### 3. Observability + Ops
- Structured logs for all command executions.
- Correlation/request ID in logs and JSON output.
- Metrics for command latency, broker failures, data staleness.

Acceptance criteria:
- Simulated provider outage tests pass with graceful degradation.
- Mutation commands are replay-safe under retry.
- On-call diagnostic data available from logs alone.

## Phase 3: LLM-First Contracts (1-2 weeks)
### 1. Stable Agent Schemas
- Create versioned JSON contracts per command family.
- Add `compatibility.md` with schema evolution rules.
- Mark fields as required/optional with null semantics.

### 2. Agent-Focused Wrappers
Add explicit output profiles:
- `--profile human|agent`
- `agent` profile guarantees deterministic field order and reduced prose.

### 3. Validation + Contract Testing
- Snapshot tests for all JSON contracts.
- Golden files for high-value scenarios.
- Backward compatibility tests for minor releases.

Acceptance criteria:
- Agents can parse all major command outputs without prompt-specific parsing logic.
- Schema changes fail CI unless versioning/compatibility rules are followed.

## Testing + CI Requirements
## Coverage Gate
- Enforce 100% line coverage in CI for `trust-cli` (and then workspace-wide by phase).
- Keep `TRUST_DISABLE_KEYCHAIN=1` in test environments.

## Test Quality Standards
- Prefer behavior/integration tests over mock-heavy tests.
- Every mutation command must include:
  - success path
  - validation failure
  - persistence/broker failure
  - idempotent retry behavior
- Every report/market command must include:
  - empty dataset
  - malformed args
  - happy path JSON contract assertions

## CI Pipeline Additions
- `cargo test -p trust-cli -- --test-threads=1`
- `cargo llvm-cov -p trust-cli --all-features --locked --summary-only`
- Contract snapshot verification lane.
- Fail build if coverage < 100% target.

## Prioritized Backlog (Execution Order)
1. Add non-interactive `trade search/list-open/reconcile`.
2. Add market-data quote/trade/session commands.
3. Add market-data news/corporate-actions commands.
4. Add report attribution/timeline commands.
5. Add stream reconnect/gap/backfill reliability controls.
6. Add schema versioning + `--profile agent`.
7. Turn on strict CI coverage and contract gates.

## Definition of Done (Production Ready)
- Complete command interface for market/trade/performance investigation needs.
- Reliable operation under network/provider failures.
- Deterministic, versioned machine-readable contracts for LLM agents.
- CI enforces quality gates: tests, contracts, and 100% coverage policy.
