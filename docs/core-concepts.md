# Core Concepts

Trust is designed around mechanical risk control. The important concepts below show how account data, risk rules, trades, reviews, and distributions fit together.

## Accounts

Accounts hold balances and broker configuration. Primary trading accounts can have child accounts for earnings, tax reserve, and reinvestment. Transfers between related accounts are recorded as transaction pairs.

## Trading Vehicles

A trading vehicle represents a tradable instrument such as a stock, ETF, crypto asset, or bond. Vehicles store normalized identity and metadata so concentration reports, advisor checks, and broker adapters can reason about the same security consistently.

## Risk Rules

Risk rules define boundaries such as maximum risk per trade and maximum monthly risk. Funding a trade reads active rules, account capital, existing exposure, and proposed trade geometry before reserving capital.

Risk is based on planned entry and stop distance. For long trades, the stop must be below entry. For short trades, the stop must be above entry.

## Levels

Levels adjust how much size a trader is allowed to take. Level policy can evaluate recent performance and move an account up, down, or into cooldown. Level-adjusted quantity appears in sizing previews and funding validation.

## Trade Lifecycle

The normal lifecycle is:

```text
New -> Funded -> Submitted -> Filled -> ClosedTarget | ClosedStopLoss
```

Funding reserves capital. Submission creates broker orders. Sync reconciles broker status, fills, fees, executions, and close conditions. Manual fill/stop/target/close commands support workflows where the broker is not the source of truth.

## Orders

Every trade has three planned orders:

- entry
- safety stop
- target

Broker implementations map these local orders to broker-specific payloads and broker order IDs.

## Transactions

Transactions are the accounting record. Deposits and withdrawals move account cash. Funding, fills, close payments, fees, distributions, and transfers all create auditable rows. Reports derive realized equity and capital-at-risk from these records plus trade state.

## Advisors

Advisors produce pre-trade warnings and blocks from portfolio concentration, catalyst calendar events, correlation clusters, and market regime. Advisor output should inform trade selection before funding.

## Grading

Closed trades can be graded with weighted criteria. Grade summaries make process quality visible across windows and accounts. Regrading creates a new grade snapshot rather than mutating away the history.

## Mistakes And Bias Tags

Trade autopsy records structured mistakes with Munger tendency tags, commission/omission classification, lollapalooza flags, optional violated rules, counterfactual R, and a lesson. `report bias-summary` aggregates those mistakes over a window and flags repeated tendencies or multiple lollapaloozas.

## Session Plans

Session plans capture the pre-session plan:

- market regime
- permitted setups
- maximum new positions
- hypothesis
- success and failure criteria

Closing a session compares actual trades against permitted setups and records adherence notes and a grade.

## Distribution

Distribution rules split realized profit between earnings, taxes, and reinvestment accounts once profit is available. Distribution execution writes the transfer legs and a history row atomically.

## Protected Mode

Protected mode uses a keyword to authorize critical mutations. Use it when running automated workflows or delegating work to agents so destructive operations require an explicit confirmation token.
