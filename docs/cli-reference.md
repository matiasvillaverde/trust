# CLI Reference

Run `trust --help` for the authoritative command list and `trust <command> --help` for command-specific flags. Many commands support interactive prompts when required arguments are omitted.

## Global Patterns

- JSON output: many reporting/list commands accept `--format json`.
- Human output: report commands default to text/human output; some commands also accept `--format human`.
- Protected changes: sensitive mutations may require `--confirm-protected <keyword>`.
- Account arguments: commands generally accept account UUIDs; some flows can resolve account names.

## Database

```bash
trust db export --output backup.json
trust db import --input backup.json --mode strict
```

Use database export/import for full SQLite backup and restore workflows.

## Keys And Onboarding

```bash
trust onboarding init
trust onboarding status --format json
trust keys create
trust keys show
trust keys delete
trust keys protected-set --value "<keyword>"
trust keys protected-show
trust keys protected-delete --confirm-protected "<keyword>"
```

Use onboarding and protected keywords before running agent-driven or automated mutation workflows.

## Accounts And Capital

```bash
trust accounts create
trust accounts search
trust accounts list --hierarchy
trust accounts balance --detailed
trust accounts transfer --from <account-id> --to <account-id> --amount 100 --reason "rebalance"

trust transaction deposit --account <account-id> --amount 1000 --currency USD
trust transaction withdraw --account <account-id> --amount 100 --currency USD
```

Account hierarchy supports primary, earnings, tax reserve, and reinvestment accounts.

## Rules And Levels

```bash
trust rule create
trust rule remove

trust level status --account <account-id>
trust level triggers
trust level history --account <account-id> --days 30
trust level change --account <account-id> --level 2 --reason "manual review"
trust level evaluate --account <account-id> --apply
trust level progress --account <account-id>
trust level rules show --account <account-id>
trust level rules set --account <account-id> --rule <key> --value <value>
```

Rules define base risk limits. Levels adjust allowable position size and can change from performance policy.

## Trading Vehicles

```bash
trust trading-vehicle create
trust trading-vehicle search --symbol AAPL --format json
trust trading-vehicle update-bond-terms --id <vehicle-id>
trust trading-vehicle stats --format json
```

Trading vehicles include broker identity, category, optional ISIN, sector/asset-class metadata, and optional fixed-income terms for bonds.

## Trade Lifecycle

```bash
trust trade create
trust trade size-preview --account <account-id> --entry 100 --stop 95
trust trade hypothesis --account <account-id> --entry 100 --stop 95 --target 115 --quantity 10
trust trade fund
trust trade submit
trust trade sync
trust trade watch
trust trade cancel
trust trade manually-fill
trust trade manually-stop
trust trade manually-target
trust trade manually-close
trust trade modify-stop
trust trade modify-target
trust trade search --account <account-id> --status filled --format json
trust trade list-open --account <account-id> --format json
trust trade reconcile --account <account-id>
```

Trades should move through create, fund, submit, sync, and close paths. Manual commands exist for broker-independent workflows and reconciliation.

## Trade Review

```bash
trust grade show --trade-id <trade-id> --format json
trust grade summary --account <account-id> --days 30
trust trade events add --trade-id <trade-id>
trust trade events list --trade-id <trade-id> --format json
trust trade autopsy --trade-id <trade-id>
```

Use these commands after a trade closes to capture process quality, catalysts, mistakes, and bias tags.

## Sessions

```bash
trust session open --account <account-id>
trust session close --account <account-id>
trust session list --account <account-id> --format json
```

Session plans capture pre-session regime, permitted setups, max new positions, hypothesis, and review notes.

## Reports

```bash
trust report summary --account <account-id> --format json
trust report performance --account <account-id> --days 30
trust report drawdown --account <account-id>
trust report risk --account <account-id>
trust report concentration --account <account-id> --open-only
trust report metrics --account <account-id> --days 30
trust report attribution --account <account-id> --by symbol --from 2026-01-01 --to 2026-01-31
trust report benchmark --account <account-id> --benchmark SPY --from 2026-01-01 --to 2026-01-31
trust report timeline --account <account-id> --granularity week --from 2026-01-01 --to 2026-01-31
trust report bias-summary --account <account-id> --days 7 --format json
```

Reports use core calculators and database reads; JSON report snapshots are contract-tested.

## Advisors And Market Data

```bash
trust advisor configure --account <account-id>
trust advisor check --account <account-id> --symbol AAPL
trust advisor status --account <account-id> --format json
trust advisor history --account <account-id>

trust market-data snapshot --account <account-id> --symbol AAPL --format json
trust market-data bars --account <account-id> --symbol AAPL --timeframe 1d --start 2026-01-01T00:00:00Z --end 2026-02-01T00:00:00Z
trust market-data stream --account <account-id> --symbols AAPL,MSFT --channels bars,quotes --max-events 10
trust market-data quote --account <account-id> --symbol AAPL
trust market-data trade --account <account-id> --symbol AAPL
trust market-data session --account <account-id> --symbol AAPL
```

Advisor commands combine concentration, catalyst, correlation, and regime checks. Market-data commands normalize broker responses into Trust domain payloads.

## Distribution And Policy

```bash
trust distribution configure --account <account-id>
trust distribution execute --account-id <account-id> --amount 100
trust distribution history --account-id <account-id>
trust distribution rules show --account-id <account-id>
trust policy --format json
```

Distribution rules split realized profits into earnings, taxes, and reinvestment according to configured percentages.
