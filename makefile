# Trust - Rust Project Makefile
# Professional CI/CD and Development Commands

# Configuration
CLI_NAME = cli
MIGRATIONS_DIRECTORY = ./db-sqlite/migrations
DIESEL_CONFIG_FILE = ./db-sqlite/diesel.toml
CLI_DATABASE_URL = ~/.trust/debug.db

# Tool paths
DIESEL_CLI = diesel
RUSTC = rustc
CARGO = cargo

# CI Configuration
CARGO_FLAGS = --locked
TEST_FLAGS = --all-features --workspace
CLIPPY_FLAGS = -- -D warnings
FMT_FLAGS = --all -- --check

# Colors for output
RED = \033[0;31m
GREEN = \033[0;32m
YELLOW = \033[0;33m
BLUE = \033[0;34m
NC = \033[0m # No Color

# Default target
.DEFAULT_GOAL := help

# Help target
.PHONY: help
help:
	@echo "$(BLUE)Trust - Available Commands$(NC)"
	@echo ""
	@echo "$(GREEN)Development Commands:$(NC)"
	@echo "  make build           - Build project in debug mode"
	@echo "  make build-release   - Build project in release mode"
	@echo "  make run            - Build and run the CLI"
	@echo "  make test           - Run all tests"
	@echo "  make test-single    - Run tests single-threaded (for DB tests)"
	@echo ""
	@echo "$(GREEN)Code Quality Commands:$(NC)"
	@echo "  make fmt            - Format code"
	@echo "  make fmt-check      - Check code formatting"
	@echo "  make lint           - Run clippy linter"
	@echo "  make audit          - Check for security vulnerabilities"
	@echo ""
	@echo "$(GREEN)CI Commands:$(NC)"
	@echo "  make ci             - Run full CI pipeline locally"
	@echo "  make ci-fast        - Run quick CI checks (fmt + clippy)"
	@echo "  make ci-test        - Run test suite as in CI"
	@echo "  make ci-build       - Run build checks as in CI"
	@echo ""
	@echo "$(GREEN)Database Commands:$(NC)"
	@echo "  make setup          - Setup database"
	@echo "  make migration      - Run migrations"
	@echo "  make clean-db       - Reset database migrations"
	@echo "  make delete-db      - Delete database file"
	@echo ""
	@echo "$(GREEN)Git Workflow Commands:$(NC)"
	@echo "  make pre-commit     - Run checks before committing"
	@echo "  make pre-push       - Run full CI before pushing"
	@echo ""
	@echo "$(GREEN)Release Commands:$(NC)"
	@echo "  make release-local  - Build all targets locally for testing"
	@echo "  make check-version  - Verify version format and changes"
	@echo ""
	@echo "$(GREEN)Utility Commands:$(NC)"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make install-tools  - Install required development tools"
	@echo "  make act            - Run GitHub Actions locally"

# Database Management
.PHONY: setup
setup:
	@echo "$(BLUE)Setting up database...$(NC)"
	@$(DIESEL_CLI) setup --config-file $(DIESEL_CONFIG_FILE) --database-url $(CLI_DATABASE_URL)

.PHONY: migration
migration:
	@echo "$(BLUE)Running migrations...$(NC)"
	@$(DIESEL_CLI) migration run --config-file $(DIESEL_CONFIG_FILE) --database-url $(CLI_DATABASE_URL)

.PHONY: clean-db
clean-db:
	@echo "$(YELLOW)Resetting database migrations...$(NC)"
	@$(DIESEL_CLI) migration redo --config-file $(DIESEL_CONFIG_FILE) --database-url $(CLI_DATABASE_URL)

.PHONY: delete-db
delete-db:
	@echo "$(RED)Deleting database file...$(NC)"
	@rm -fr $(CLI_DATABASE_URL)

# Build Commands
.PHONY: build
build: setup
	@echo "$(BLUE)Building project (debug)...$(NC)"
	@$(CARGO) build $(CARGO_FLAGS)

.PHONY: build-release
build-release: setup
	@echo "$(BLUE)Building project (release)...$(NC)"
	@$(CARGO) build $(CARGO_FLAGS) --release

.PHONY: run
run: build
	@echo "$(BLUE)Running CLI...$(NC)"
	@$(CARGO) run --bin $(CLI_NAME)

# Testing Commands
.PHONY: test
test: setup
	@echo "$(BLUE)Running tests...$(NC)"
	@$(CARGO) test $(TEST_FLAGS)

.PHONY: test-single
test-single: setup
	@echo "$(BLUE)Running tests (single-threaded)...$(NC)"
	@$(CARGO) test $(TEST_FLAGS) -- --test-threads=1

# Code Quality Commands
.PHONY: fmt
fmt:
	@echo "$(BLUE)Formatting code...$(NC)"
	@$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check:
	@echo "$(BLUE)Checking code formatting...$(NC)"
	@$(CARGO) fmt $(FMT_FLAGS)

.PHONY: lint
lint:
	@echo "$(BLUE)Running clippy...$(NC)"
	@$(CARGO) clippy --workspace --all-targets --all-features $(CLIPPY_FLAGS)

.PHONY: audit
audit:
	@echo "$(BLUE)Checking for security vulnerabilities...$(NC)"
	@$(CARGO) audit

# CI Pipeline Commands
.PHONY: ci
ci: ci-fast ci-build ci-test
	@echo "$(GREEN)✓ Full CI pipeline passed!$(NC)"

.PHONY: ci-fast
ci-fast: fmt-check lint
	@echo "$(GREEN)✓ Quick CI checks passed!$(NC)"

.PHONY: ci-test
ci-test: setup
	@echo "$(BLUE)Running CI test suite...$(NC)"
	@$(CARGO) test $(TEST_FLAGS) $(CARGO_FLAGS)
	@$(CARGO) test --doc $(CARGO_FLAGS)

.PHONY: ci-build
ci-build: setup
	@echo "$(BLUE)Running CI build checks...$(NC)"
	@$(CARGO) check $(CARGO_FLAGS) --all-features --workspace
	@$(CARGO) check $(CARGO_FLAGS) --no-default-features --workspace
	@$(CARGO) build -p model $(CARGO_FLAGS) --release
	@$(CARGO) build -p core $(CARGO_FLAGS) --release
	@$(CARGO) build -p cli $(CARGO_FLAGS) --release
	@$(CARGO) build --all $(CARGO_FLAGS) --release

# Git Workflow Commands
.PHONY: pre-commit
pre-commit: fmt-check lint test-single
	@echo "$(GREEN)✓ Pre-commit checks passed!$(NC)"

.PHONY: pre-push
pre-push: ci
	@echo "$(GREEN)✓ Pre-push checks passed! Safe to push.$(NC)"

# Utility Commands
.PHONY: clean
clean:
	@echo "$(BLUE)Cleaning build artifacts...$(NC)"
	@$(CARGO) clean

.PHONY: install-tools
install-tools:
	@echo "$(BLUE)Installing development tools...$(NC)"
	@echo "Installing cargo-audit..."
	@$(CARGO) install cargo-audit
	@echo "Installing cargo-nextest..."
	@$(CARGO) install cargo-nextest
	@echo ""
	@echo "$(YELLOW)To install 'act' for running GitHub Actions locally:$(NC)"
	@echo "  macOS:    brew install act"
	@echo "  Linux:    curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash"
	@echo "  Windows:  choco install act-cli"

# Release Commands
.PHONY: release-local
release-local:
	@echo "$(BLUE)Building all release targets locally...$(NC)"
	@echo "$(YELLOW)Installing required targets...$(NC)"
	@rustup target add aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu || true
	@echo "$(BLUE)Building for aarch64-apple-darwin...$(NC)"
	@$(CARGO) build --release --target aarch64-apple-darwin --bin cli
	@echo "$(BLUE)Building for x86_64-apple-darwin...$(NC)"
	@$(CARGO) build --release --target x86_64-apple-darwin --bin cli
	@echo "$(BLUE)Building for x86_64-unknown-linux-gnu...$(NC)"
	@$(CARGO) build --release --target x86_64-unknown-linux-gnu --bin cli || echo "$(YELLOW)Warning: Linux target may not be available on this platform$(NC)"
	@echo "$(GREEN)✓ All available targets built successfully!$(NC)"

.PHONY: check-version
check-version:
	@echo "$(BLUE)Checking version format and extracting current version...$(NC)"
	@VERSION=$$(grep -E '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'); \
	if [ -z "$$VERSION" ]; then \
		echo "$(RED)Error: Could not extract version from Cargo.toml$(NC)"; \
		exit 1; \
	fi; \
	echo "$(GREEN)Current version: $$VERSION$(NC)"; \
	if echo "$$VERSION" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+' > /dev/null; then \
		echo "$(GREEN)✓ Version format is valid$(NC)"; \
	else \
		echo "$(RED)Error: Version format is invalid. Expected format: X.Y.Z$(NC)"; \
		exit 1; \
	fi

# Act (GitHub Actions locally)
.PHONY: act
act:
	@echo "$(BLUE)Running GitHub Actions locally with act...$(NC)"
	@if command -v act >/dev/null 2>&1; then \
		act; \
	else \
		echo "$(RED)Error: 'act' is not installed.$(NC)"; \
		echo "Run 'make install-tools' for installation instructions."; \
		exit 1; \
	fi

.PHONY: act-job
act-job:
	@if [ -z "$(JOB)" ]; then \
		echo "$(RED)Error: JOB parameter required.$(NC)"; \
		echo "Usage: make act-job JOB=test"; \
		exit 1; \
	fi
	@echo "$(BLUE)Running job '$(JOB)' with act...$(NC)"
	@if command -v act >/dev/null 2>&1; then \
		act -j $(JOB); \
	else \
		echo "$(RED)Error: 'act' is not installed.$(NC)"; \
		echo "Run 'make install-tools' for installation instructions."; \
		exit 1; \
	fi