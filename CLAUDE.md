# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Trust is an algorithmic trading tool written in Rust that provides risk management features for trading operations. It integrates with the Alpaca API and uses SQLite for data persistence.

## Development Commands

### Build and Run
```bash
make build      # Sets up database and builds the project
make run        # Default target - runs the CLI
make test       # Runs all tests
cargo test -p cli -- --test-threads=1  # Run tests with single thread (for database tests)
```

### Database Management
```bash
make setup      # Initial diesel setup
make migration NAME=migration_name  # Create new migration
make clean-db   # Reset migrations (redo)
make delete-db  # Delete database file
```

### Working with Individual Crates
```bash
cargo build -p model        # Build specific crate
cargo test -p core         # Test specific crate
cargo run -p cli          # Run the CLI directly
```

## CI/CD Workflow

The project includes a comprehensive CI/CD setup that should be run locally before pushing changes.

### Quick CI Commands

```bash
# Essential commands for daily development
make fmt         # Auto-format code
make ci-fast     # Quick checks (fmt + clippy)
make pre-commit  # Run before committing
make pre-push    # Run full CI before pushing
```

### Full CI Pipeline

```bash
# Run the complete CI pipeline locally
make ci

# This runs:
# 1. Format checking (make fmt-check)
# 2. Clippy linting (make lint)
# 3. Build verification (make ci-build)
# 4. Test suite (make ci-test)
```

### Using GitHub Actions Locally

Install and use `act` to run the actual GitHub Actions workflow:

```bash
# Install act
brew install act  # macOS
# Or see other options: make install-tools

# Run GitHub Actions locally
make act          # Run all workflows
make act-job JOB=lint  # Run specific job
```

### CI Best Practices

1. **Always run before pushing**: Use `make pre-push` to ensure your code will pass CI
2. **Format first**: Run `make fmt` to auto-format code before committing
3. **Test database code**: Use `make test-single` for database-related tests
4. **Quick validation**: Use `make ci-fast` for rapid feedback during development

### Common CI Issues and Solutions

1. **Format failures**: Run `make fmt` (not just `make fmt-check`)
2. **Clippy warnings**: Address all warnings from `make lint`
3. **Test failures**: Check if database tests need single-threading
4. **Build issues**: Ensure `make build-release` succeeds

For detailed CI documentation, see [CI.md](./CI.md).

## Architecture

The project follows a clean architecture pattern with these key layers:

### Workspace Structure
- **model**: Core domain models shared across all crates. Contains structs for Account, Trade, Transaction, and broker-agnostic traits.
- **db-sqlite**: SQLite implementation using Diesel ORM. Implements the database traits defined in model.
- **core**: Business logic layer containing validators and calculators. Enforces risk management rules.
- **cli**: Command-line interface using Clap. Orchestrates operations between core logic and database/broker.
- **alpaca-broker**: Alpaca API integration implementing the Broker trait from model.

### Key Architectural Patterns

1. **Trait-Based Abstraction**: The `model` crate defines traits (Database, Broker) that are implemented by concrete modules, allowing easy swapping of implementations.

2. **Calculator Pattern**: Financial calculations are isolated in dedicated calculator modules:
   - `core/src/calculators/account_calculator.rs`: Account metrics and risk calculations
   - `core/src/calculators/trade_calculator.rs`: Trade-specific calculations

3. **Command Pattern in CLI**: Each operation is organized as a command with its own builder:
   - Account commands: create, fund, list, show
   - Trade commands: fund, submit, sync, modify stops/targets, cancel, close

4. **Risk Validation Flow**: 
   ```
   CLI Command → Core Validators → Database Check → Broker API → Database Update
   ```

### Database Schema

Key tables (defined via Diesel migrations):
- `accounts`: Trading accounts with risk parameters
- `trades`: Individual trades with entry/exit prices
- `transactions`: Financial transactions (deposits/withdrawals)
- `broker_accounts`: Broker-specific account information

### Important Implementation Details

1. **Financial Precision**: Uses `rust_decimal::Decimal` for all monetary values to avoid floating-point errors.

2. **Risk Management**: Core validators ensure:
   - Trades don't exceed max risk per trade
   - Monthly risk limits aren't breached
   - Sufficient capital is available

3. **Async Operations**: Broker operations use Tokio for async API calls.

4. **Error Handling**: Comprehensive error types in each crate with proper error propagation.

5. **Testing**: Integration tests require single-threaded execution due to SQLite database access.

## Common Development Tasks

### Adding a New Broker
1. Create a new crate in the workspace
2. Implement the `Broker` trait from `model/src/broker.rs`
3. Add the crate to `Cargo.toml` workspace members
4. Update CLI to support the new broker option

### Adding Database Migrations
```bash
make migration NAME=add_new_field
# Edit the generated migration files in db-sqlite/migrations/
make build  # Applies migrations
```

### Modifying Trade Logic
- Risk validation logic: `core/src/validators/`
- Trade calculations: `core/src/calculators/trade_calculator.rs`
- Database operations: `db-sqlite/src/trade.rs`
- CLI commands: `cli/src/commands/trade/`