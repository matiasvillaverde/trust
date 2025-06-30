# Trust: Risk-Managed Algorithmic Trading System

Trust is a comprehensive algorithmic trading system written in Rust that enforces disciplined risk management and automates the complete trade lifecycle. Built with a focus on capital preservation, Trust ensures that every trade adheres to predefined risk parameters before execution, making it an essential tool for systematic traders who prioritize risk control.

**⚠️ Beta Notice**: This product is in beta. Use it only if you understand the underlying code and accept the risks of beta software.

📚 **Full Documentation**: https://deepwiki.com/matiasvillaverde/trust

## What Trust Does

Trust solves a critical problem in algorithmic trading: **enforcing risk management rules automatically**. Many traders struggle with discipline when it comes to position sizing and risk limits. Trust addresses this by:

1. **Preventing Over-Exposure**: Automatically validates every trade against your risk rules before allowing capital allocation
2. **Managing Trade Lifecycle**: Handles the complete journey from trade creation to settlement with a structured state machine
3. **Broker Abstraction**: Provides a unified interface for trade management across different brokers (currently supporting Alpaca)
4. **Capital Tracking**: Maintains accurate records of capital allocation, ensuring you always know your exposure

## Key Features

### 🛡️ Risk Management
- **Per-Trade Risk Limits**: Enforces maximum risk per trade as a percentage of account balance
- **Monthly Risk Caps**: Prevents excessive monthly drawdowns by limiting total risk exposure
- **Pre-Trade Validation**: All risk checks happen before capital is committed, not after

### 📊 Trade Lifecycle Management
- **Structured Workflow**: Trades progress through defined states (New → Funded → Submitted → Filled → Closed)
- **Three-Order System**: Every trade includes entry, target, and stop-loss orders
- **Real-Time Synchronization**: Continuously syncs with broker to track order status changes
- **Modification Support**: Adjust stops and targets on active trades

### 🔌 Broker Integration
- **Alpaca API Support**: Full integration with Alpaca for automated trading
- **Extensible Design**: Add new brokers by implementing the `Broker` trait
- **Manual Trading Option**: Generate orders for manual submission to any broker

### 💰 Financial Tracking
- **Capital Reservation**: Funds are reserved when trades are funded, preventing over-allocation
- **Transaction History**: Complete audit trail of deposits, withdrawals, and trade settlements
- **Tax Tracking**: Separates taxable and non-taxable capital for proper accounting

## Architecture Overview

Trust follows a clean, modular architecture with clear separation of concerns:

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI Interface                            │
│                    (User Commands & Dialogs)                     │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│                      Core (TrustFacade)                          │
│            (Business Logic & Trade Lifecycle)                    │
├─────────────────────────────────────────────────────────────────┤
│  Validators           │  Calculators         │  Commands         │
│  • Risk Rules         │  • Capital Required  │  • Trade Ops      │
│  • Trade Validation   │  • Risk Assessment   │  • Account Mgmt   │
│  • Funding Checks     │  • Tax Calculations  │  • Transactions   │
└───────┬───────────────┴──────────────────────┴─────────┬────────┘
        │                                                 │
┌───────▼────────┐                              ┌────────▼────────┐
│  Model Layer   │                              │ Broker Interface │
│  (Domain Types)│                              │   (Trait API)    │
└───────┬────────┘                              └────────┬────────┘
        │                                                 │
┌───────▼────────┐                              ┌────────▼────────┐
│ Database Layer │                              │ Broker Impls    │
│   (SQLite)     │                              │   (Alpaca)      │
└────────────────┘                              └─────────────────┘
```

### Workspace Structure

Trust is organized as a Rust workspace with focused crates:

- **`model`**: Core domain models and traits. Defines the contracts that all components must follow.
- **`core`**: Business logic, validators, and calculators. Contains the `TrustFacade` that orchestrates all operations.
- **`db-sqlite`**: SQLite database implementation using Diesel ORM.
- **`alpaca-broker`**: Alpaca API integration implementing the broker trait.
- **`cli`**: Command-line interface providing user interaction.

### Key Design Patterns

#### 1. Facade Pattern (TrustFacade)
The `TrustFacade` serves as the single entry point to the core system, simplifying complex operations:

```rust
// Example: Creating and funding a trade
let trade = trust.create_trade(draft_trade)?;
let funded_trade = trust.fund_trade(trade.id)?;
```

#### 2. Strategy Pattern (Validators & Calculators)
Risk rules and calculations are pluggable strategies:
- **Validators**: Ensure trades meet risk criteria before execution
- **Calculators**: Compute required capital, risk metrics, and tax implications

#### 3. Repository Pattern (Database Layer)
The database layer abstracts data persistence behind trait interfaces, allowing for different storage backends.

#### 4. Adapter Pattern (Broker Integration)
The `Broker` trait defines a standard interface that broker-specific implementations must follow:

```rust
pub trait Broker {
    async fn submit_order(&self, order: Order) -> Result<BrokerOrder>;
    async fn get_order(&self, id: &str) -> Result<BrokerOrder>;
    // ... other broker operations
}
```

### Trade State Machine

Trades progress through a well-defined state machine:

```
   New ──────► Funded ──────► Submitted ──────► Filled ──────► Closed
    │            │                │                │              │
    └────────────┴────────────────┴────────────────┴──────────────┘
                              Cancelled
```

Each transition enforces business rules and maintains data integrity.

### Risk Validation Flow

1. **Trade Creation**: Basic validation of trade parameters
2. **Funding Request**: Triggers comprehensive risk validation
3. **Rule Checking**: Validates against per-trade and monthly risk limits
4. **Capital Reservation**: Locks funds if validation passes
5. **Order Submission**: Sends orders to broker only after funding

## Prerequisites

Make sure you have Rust installed.

## Installation

Clone this repository:

``` bash
git clone https://github.com/matiasvillaverde/trust.git
cd trust
```

## Quick Start

### Development Setup

```bash
# Clone the repository
git clone https://github.com/matiasvillaverde/trust.git
cd trust

# Build the project
make build

# Run tests
make test

# Run the CLI
make run
```

### CI/CD Quick Reference

Before pushing code, ensure it passes all checks:

```bash
# Format code
make fmt

# Run quick CI checks
make ci-fast

# Run full CI pipeline locally
make ci
```

See the [CI Documentation](./CI.md) for detailed CI/CD information.

## Usage

### Getting Started - Complete Trading Workflow

Here's a typical workflow for setting up and executing a trade with Trust:

```bash
# 1. Initial Setup
cargo run --bin cli -- account create          # Create a trading account
cargo run --bin cli -- rule create              # Set risk parameters (e.g., 2% per trade, 6% per month)
cargo run --bin cli -- transaction deposit      # Fund your account

# 2. Configure Trading
cargo run --bin cli -- trading-vehicle create   # Add symbols you want to trade (e.g., AAPL, SPY)
cargo run --bin cli -- key create               # Add Alpaca API credentials

# 3. Trade Execution
cargo run --bin cli -- trade create             # Design a trade with entry, stop, and target
cargo run --bin cli -- trade fund               # Validate risk and reserve capital
cargo run --bin cli -- trade submit             # Send orders to broker

# 4. Trade Management
cargo run --bin cli -- trade sync               # Update trade status from broker
cargo run --bin cli -- trade modify-stop        # Adjust stop loss
cargo run --bin cli -- trade modify-target      # Adjust profit target
cargo run --bin cli -- trade close              # Exit the position
```

### Command Reference

```bash
# Account Management
cargo run --bin cli -- account create           # Create new account
cargo run --bin cli -- account list             # List all accounts
cargo run --bin cli -- account show             # View account details

# Risk Rules
cargo run --bin cli -- rule create              # Define risk parameters
cargo run --bin cli -- rule list                # View active rules

# Capital Management
cargo run --bin cli -- transaction deposit      # Add funds
cargo run --bin cli -- transaction withdraw     # Remove funds
cargo run --bin cli -- transaction list         # Transaction history

# Trade Operations
cargo run --bin cli -- trade create             # Create new trade
cargo run --bin cli -- trade fund               # Allocate capital (validates risk)
cargo run --bin cli -- trade submit             # Send to broker
cargo run --bin cli -- trade sync               # Update from broker
cargo run --bin cli -- trade cancel             # Cancel pending orders
cargo run --bin cli -- trade close              # Close position
cargo run --bin cli -- trade list               # View all trades

# Help & Information
cargo run --bin cli -- help                     # General help
cargo run --bin cli -- [command] help           # Command-specific help
```

### Example: Creating a Risk-Managed Trade

```bash
# Set up 2% max risk per trade, 6% max risk per month
cargo run --bin cli -- rule create --risk-per-trade 2.0 --risk-per-month 6.0

# Create a trade: Buy 100 shares of AAPL at $150, stop at $145, target at $160
cargo run --bin cli -- trade create \
  --symbol AAPL \
  --quantity 100 \
  --entry 150.00 \
  --stop 145.00 \
  --target 160.00

# Fund the trade (Trust will validate: risk = (150-145) × 100 = $500)
cargo run --bin cli -- trade fund --trade-id 1

# If validation passes, submit to broker
cargo run --bin cli -- trade submit --trade-id 1
```

## Extending Trust

### Adding a New Broker

Trust's modular design makes it easy to add support for new brokers:

1. **Create a new crate** in the workspace:
   ```bash
   cargo new --lib brokers/new-broker
   ```

2. **Implement the Broker trait** from `model/src/broker.rs`:
   ```rust
   use model::broker::{Broker, BrokerOrder};
   
   pub struct NewBroker {
       // Your broker-specific fields
   }
   
   #[async_trait]
   impl Broker for NewBroker {
       async fn submit_order(&self, order: Order) -> Result<BrokerOrder> {
           // Implement broker API call
       }
       // ... other required methods
   }
   ```

3. **Add to CLI** in `cli/src/main.rs` to make it available

### Adding Custom Risk Rules

1. **Define the rule** in `model/src/rule.rs`
2. **Implement validation** in `core/src/validators/`
3. **Add calculator** if needed in `core/src/calculators/`

### Database Customization

The database layer uses Diesel ORM with SQLite. To modify:

1. **Create migration**:
   ```bash
   make migration NAME=your_change
   ```

2. **Edit migration** files in `db-sqlite/migrations/`

3. **Apply migration**:
   ```bash
   make build
   ```

## Development

### Available Commands

Run `make help` to see all available commands:

- **Development**: `make build`, `make test`, `make run`
- **Code Quality**: `make fmt`, `make lint`, `make audit`
- **CI Pipeline**: `make ci`, `make pre-commit`, `make pre-push`
- **Database**: `make setup`, `make migration`, `make clean-db`

### Continuous Integration

This project uses GitHub Actions for CI/CD. The pipeline includes:

- Code formatting checks
- Clippy linting
- Comprehensive testing (all features, no features, doc tests)
- Release build verification
- Security audit

To run the same checks locally before pushing:

```bash
# Quick validation
make pre-commit

# Full CI pipeline
make pre-push

# Run GitHub Actions locally with act
make act
```

For more details, see [CI.md](./CI.md).

## Disclaimer

This tool is currently in the beta phase and should be used cautiously. You should only proceed if you understand how the underlying code operates. There might be bugs and unexpected behavior on rare occasions.

## License

MIT License - see the LICENSE file for details.

## Support

If you encounter any problems, please open an issue. We'll try to resolve it as soon as possible.
