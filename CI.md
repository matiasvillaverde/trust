# CI/CD Documentation for Trust

This document provides comprehensive information about the Continuous Integration and Continuous Deployment (CI/CD) setup for the Trust project.

## Table of Contents

- [Overview](#overview)
- [Local Development Workflow](#local-development-workflow)
- [CI Pipeline Structure](#ci-pipeline-structure)
- [Running CI Locally](#running-ci-locally)
- [Common Commands](#common-commands)
- [Troubleshooting](#troubleshooting)
- [Best Practices](#best-practices)

## Overview

The Trust project uses GitHub Actions for CI/CD with a focus on:
- **Speed**: Quick feedback on code changes
- **Reliability**: Consistent results between local and remote execution
- **Developer Experience**: Easy-to-use commands for local validation

## Local Development Workflow

### Quick Start

```bash
# Before committing
make pre-commit

# Before pushing
make pre-push

# Run specific checks
make fmt-check  # Check formatting
make lint       # Run clippy
make test       # Run all tests
```

### Recommended Development Flow

1. **Make changes** to your code
2. **Format code** automatically:
   ```bash
   make fmt
   ```
3. **Run tests** locally:
   ```bash
   make test-single  # For database-related tests
   make test         # For all tests
   ```
4. **Check code quality**:
   ```bash
   make ci-fast     # Quick checks (formatting + linting)
   ```
5. **Before committing**:
   ```bash
   make pre-commit  # Runs fmt-check, lint, and tests
   ```
6. **Before pushing**:
   ```bash
   make pre-push    # Runs full CI pipeline locally
   ```

## CI Pipeline Structure

### GitHub Actions Workflow

The CI pipeline (`.github/workflows/rust.yaml`) consists of these jobs:

1. **quick-checks** (runs first, fails fast)
   - Formatting verification
   - Quick compilation check
   
2. **lint** (runs in parallel)
   - Clippy with all targets and features
   - Treats warnings as errors

3. **build-and-test** (comprehensive testing)
   - Tests with all features
   - Tests with no default features
   - Documentation tests
   
4. **release-build** (production readiness)
   - Builds all crates in release mode
   - Ensures optimized builds work

5. **audit** (security check, non-blocking)
   - Checks for known vulnerabilities

### Performance Optimizations

- **Parallel execution**: Jobs run concurrently where possible
- **Smart caching**: Dependencies cached between runs
- **Fail-fast**: Quick checks run first for rapid feedback
- **Matrix builds**: Test variations run in parallel

## Running CI Locally

### Using Make Commands

The Makefile provides commands that mirror the CI pipeline:

```bash
# Run full CI pipeline
make ci

# Run only quick checks
make ci-fast

# Run only tests as in CI
make ci-test

# Run only build checks
make ci-build
```

### Using Act (GitHub Actions Locally)

Act allows you to run the actual GitHub Actions workflow on your machine:

#### Installation

```bash
# macOS
brew install act

# Linux
curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash

# Windows
choco install act-cli

# Or via make
make install-tools  # Shows installation instructions
```

#### Usage

```bash
# Run all workflows
make act

# Run specific job
make act-job JOB=lint
make act-job JOB=build-and-test

# Using act directly
act                    # Run all jobs
act -j quick-checks   # Run specific job
act -l                # List all jobs
```

#### First-time Setup

When running `act` for the first time:
1. Choose the **Medium** image (~500MB) for Rust workflows
2. This provides a good balance between size and functionality

### Direct Command Execution

You can also run CI commands directly:

```bash
# Formatting
cargo fmt --all -- --check

# Linting
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Testing
cargo test --locked --all-features --workspace
cargo test --locked --no-default-features --workspace
cargo test --locked --doc

# Building
cargo build --locked --release --all
```

## Common Commands

### Development Commands

| Command | Description |
|---------|-------------|
| `make build` | Build project in debug mode |
| `make build-release` | Build project in release mode |
| `make run` | Build and run the CLI |
| `make test` | Run all tests |
| `make test-single` | Run tests single-threaded (for DB) |

### Code Quality Commands

| Command | Description |
|---------|-------------|
| `make fmt` | Format code automatically |
| `make fmt-check` | Check code formatting |
| `make lint` | Run clippy linter |
| `make audit` | Security vulnerability check |

### CI Commands

| Command | Description |
|---------|-------------|
| `make ci` | Run full CI pipeline |
| `make ci-fast` | Quick CI checks |
| `make ci-test` | Run test suite as in CI |
| `make ci-build` | Run build checks as in CI |

### Workflow Commands

| Command | Description |
|---------|-------------|
| `make pre-commit` | Pre-commit validation |
| `make pre-push` | Pre-push validation (full CI) |
| `make act` | Run GitHub Actions locally |

## Troubleshooting

### Common Issues

#### 1. Database Test Failures

**Problem**: Tests fail due to database conflicts when run in parallel.

**Solution**:
```bash
make test-single  # Runs tests with --test-threads=1
```

#### 2. Formatting Differences

**Problem**: Local formatting differs from CI.

**Solution**:
```bash
# Ensure you're using the same Rust version
rustup update stable
rustup default stable

# Format code
make fmt
```

#### 3. Clippy Warnings

**Problem**: Clippy finds issues not caught locally.

**Solution**:
```bash
# Run clippy with the same flags as CI
make lint

# Or directly
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

#### 4. Act Docker Issues

**Problem**: Act fails with Docker errors.

**Solution**:
1. Ensure Docker is running
2. Use the medium-sized image
3. Run with specific platform: `act --platform ubuntu-latest=medium`

### CI vs Local Differences

Some differences between CI and local environments:

1. **Environment variables**: CI has `CI=true` set
2. **Permissions**: CI may have different file permissions
3. **Dependencies**: Ensure local tools match CI versions
4. **Caching**: CI uses aggressive caching

To minimize differences:
- Always use `--locked` flag with cargo commands
- Keep tools updated: `rustup update`
- Use the same commands as defined in the Makefile

## Best Practices

### Before Committing

1. **Format your code**: `make fmt`
2. **Run quick checks**: `make ci-fast`
3. **Test your changes**: `make test-single` (for DB) or `make test`
4. **Verify with**: `make pre-commit`

### Before Creating a PR

1. **Run full CI**: `make pre-push`
2. **Check for warnings**: `make lint`
3. **Verify builds**: `make build-release`
4. **Run with act**: `make act` (optional but recommended)

### Writing CI-Friendly Code

1. **Avoid flaky tests**: Use deterministic test data
2. **Handle timeouts**: Set reasonable timeouts for async operations
3. **Clean up resources**: Ensure tests clean up after themselves
4. **Use feature flags**: Test with different feature combinations

### Speed Optimization Tips

1. **Run targeted tests**: `cargo test -p specific_crate`
2. **Use watch mode**: `cargo watch -x test`
3. **Skip slow checks**: Use `make ci-fast` for quick validation
4. **Cache dependencies**: Act caches Docker layers automatically

## Advanced Usage

### Custom CI Runs

```bash
# Run CI with custom cargo flags
CARGO_FLAGS="--verbose" make ci

# Run specific test features
TEST_FLAGS="--features custom" make test

# Debug CI issues
RUST_BACKTRACE=full make ci
```

### Integration with IDEs

Most IDEs can run make commands. Configure your IDE to run:
- `make fmt` on save
- `make ci-fast` before commit
- `make test` with keyboard shortcuts

### Git Hooks (Optional)

To automate checks, create git hooks:

```bash
# .git/hooks/pre-commit
#!/bin/sh
make pre-commit

# .git/hooks/pre-push
#!/bin/sh
make ci-fast
```

## Conclusion

The CI setup for Trust prioritizes developer experience and fast feedback. By following this guide and using the provided commands, you can ensure your code meets quality standards before it reaches the repository.

For more information, see:
- [Makefile](./Makefile) - All available commands
- [GitHub Actions Workflow](./.github/workflows/rust.yaml) - CI pipeline definition
- [CLAUDE.md](./CLAUDE.md) - Project-specific guidelines