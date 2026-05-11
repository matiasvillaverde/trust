# Architecture

Trust is organized as a Rust workspace with strict boundaries between domain types, business logic, persistence, brokers, and CLI presentation.

## Workspace Crates

- `trust-model`: domain types and trait contracts. Monetary values use `rust_decimal::Decimal`.
- `core`: business logic and the `TrustFacade` entry point.
- `db-sqlite`: Diesel-backed SQLite implementation of the database factory traits.
- `trust-cli`: Clap CLI, dialogs, command routing, and user-facing views.
- `alpaca-broker`: Alpaca implementation of the `Broker` trait.
- `ibkr-broker`: Interactive Brokers implementation and gateway adapters.
- `broker-sync`: actor-based realtime sync and reconciliation support.
- `advisor`: advisory checks for catalyst, correlation, regime, and position heat.

## Dependency Direction

The intended runtime flow is:

```text
CLI -> core::TrustFacade -> model traits -> db/broker implementations
```

The CLI should not call the database or broker directly for business decisions. It parses arguments, asks dialogs, calls core, and renders results. Core validates and coordinates state transitions. Database and broker crates implement the traits defined in `trust-model`.

## Core Facade

`TrustFacade` is the single public business-logic entry point used by the CLI. It owns:

- account and transaction operations
- trade creation, funding, submission, sync, cancellation, and close workflows
- risk validation before capital commitment
- level evaluation and adjustment
- distribution rules and execution
- grading, catalyst events, mistakes, session plans, and reports
- advisor checks and broker registry routing

This keeps user-facing commands thin and makes tests exercise the same path real users exercise.

## Database Layer

`DatabaseFactory` returns specialized read/write trait objects such as account, trade, order, execution, grade, mistake, and session-plan readers/writers. SQLite implements those traits in `db-sqlite`.

Schema changes are made through migrations:

```bash
make migration NAME=descriptive_name
make build
```

Do not manually edit `db-sqlite/src/schema.rs`; Diesel generates it.

## Trade State Machine

Trades move through explicit states:

```text
Draft/New -> Funded -> Submitted -> Filled -> ClosedTarget | ClosedStopLoss
                         |              |
                         |              -> Canceled for manual close/cancel paths
                         -> Canceled
```

Each trade owns three order records:

- entry order
- safety stop order
- target order

State transitions are persisted with transaction records so capital reservation, fills, fees, and releases are auditable.

## Risk Validation Flow

Funding is the main capital commitment boundary:

```text
CLI command
  -> TrustFacade
  -> validators and calculators
  -> database reads for balances, rules, and open exposure
  -> database writes for funded trade and fund transaction
```

Submission happens after funding. Broker calls do not decide whether a trade is acceptable; core does that first.

## Broker Integration

Brokers implement `model::Broker`. The core broker registry routes operations based on account broker kind. Broker crates are responsible for mapping broker-specific order payloads, statuses, market data, executions, and fees into Trust domain objects.

Broker operations are split by behavior. For Alpaca, submit, sync, close, cancel, and modify operations each have dedicated modules.

## Event-Driven Review

Closed trades feed post-trade processes:

- trade grading
- profit distribution
- level evaluation
- catalyst event review
- mistake and bias autopsy
- session plan review

The key principle is that closing a trade creates data for both accounting and future decision quality, not just a terminal trade status.
