# Getting Started

Trust is a local-first trading workflow for designing, validating, funding, submitting, syncing, and reviewing trades. The CLI talks to `core::TrustFacade`; broker and database details stay behind core traits.

## Prerequisites

- Rust toolchain with Cargo.
- SQLite development libraries.
- Diesel CLI with SQLite support:

```bash
cargo install diesel_cli --no-default-features --features sqlite
```

- Broker credentials for the environment you intend to use. Alpaca is supported for normal broker execution. Interactive Brokers support is available for configured IBKR accounts and related workflows.

## Local Setup

```bash
git clone https://github.com/matiasvillaverde/trust.git
cd trust
make build
```

Useful verification commands:

```bash
make ci-fast
cargo test -p trust-cli -- --test-threads=1
cargo test --locked --no-default-features --workspace
```

SQLite defaults to the project database configuration. For isolated local runs, set `TRUST_DB_URL`:

```bash
export TRUST_DB_URL="file:trust-dev.db"
```

## First Account

Create an account, deposit capital, and define a risk rule before creating trades:

```bash
trust accounts create
trust transaction deposit
trust rule create
```

Most commands support interactive prompts. For automation or tests, use command-specific flags shown in `trust <command> <subcommand> --help`.

## Broker Keys

Store broker credentials through the key commands:

```bash
trust keys create
trust keys show
```

Protected mutations can be guarded by a keyword:

```bash
trust onboarding init
trust onboarding status
trust keys protected-set --value "<keyword>"
```

Use paper trading credentials while validating setup. Critical account and policy changes should use protected mode where available.

## First Trading Vehicle

Register a security before creating a trade:

```bash
trust trading-vehicle create
trust trading-vehicle search --symbol AAPL
```

Trust supports stocks, ETFs, crypto, and bonds in the model and database. Broker-specific availability depends on the configured broker.

## First Risk-Managed Trade

The normal lifecycle is:

```bash
trust trade create
trust trade size-preview --account <account-id> --entry <price> --stop <price>
trust trade fund
trust trade submit
trust trade sync
```

Every trade has an entry, safety stop, and target order. Funding validates risk before capital is reserved. Submitting sends the broker order only after the trade is funded.

After exit:

```bash
trust grade show --trade-id <trade-id>
trust trade autopsy --trade-id <trade-id>
trust report bias-summary --account <account-id> --days 7
```

## Daily Workflow

Use session plans and advisor checks to keep decisions mechanical:

```bash
trust session open --account <account-id>
trust trade advisor --account <account-id> --symbol AAPL --entry 100 --stop 95 --target 115 --quantity 10
trust report risk --account <account-id>
trust report concentration --account <account-id>
trust session close --account <account-id>
```

For JSON output, add `--format json` to supported report and list commands.
