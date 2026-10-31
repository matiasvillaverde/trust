# Trust - Rust Project Makefile
# Professional CI/CD and Development Commands

# Configuration
CLI_NAME = trust
MIGRATIONS_DIRECTORY = ./db-sqlite/migrations
DIESEL_CONFIG_FILE = ./db-sqlite/diesel.toml
CLI_DATABASE_URL = ~/.trust/debug.db
CARGO_HOME ?= $(HOME)/.cargo

# Ensure cargo-installed tooling is available in all make targets.
export PATH := $(CARGO_HOME)/bin:$(PATH)

# Tool paths
DIESEL_CLI = diesel
RUSTC = rustc
CARGO = cargo
CARGO_AUDIT_VERSION = 0.22.2
CARGO_DENY_VERSION = 0.20.2
CARGO_MACHETE_VERSION = 0.9.2
CARGO_LLVM_COV_VERSION = 0.9.1

# CI Configuration
CARGO_FLAGS = --locked
TEST_FLAGS = --all-features --workspace
CLIPPY_FLAGS = -- -D warnings
FMT_FLAGS = --all -- --check
COVERAGE_MIN_LINES ?= 94
COVERAGE_MIN_FUNCTIONS ?= 90
COVERAGE_MIN_REGIONS ?= 93
RUSTC_LLVM_VERSION := $(shell $(RUSTC) -vV 2>/dev/null | awk -F': ' '/LLVM version/ {print $$2}')
HOMEBREW_LLVM_BIN := $(firstword $(wildcard /opt/homebrew/Cellar/llvm/$(RUSTC_LLVM_VERSION)/bin /usr/local/Cellar/llvm/$(RUSTC_LLVM_VERSION)/bin))
ifneq ($(HOMEBREW_LLVM_BIN),)
ifneq ($(wildcard $(HOMEBREW_LLVM_BIN)/llvm-cov),)
LLVM_COV ?= $(HOMEBREW_LLVM_BIN)/llvm-cov
LLVM_PROFDATA ?= $(HOMEBREW_LLVM_BIN)/llvm-profdata
export LLVM_COV
export LLVM_PROFDATA
endif
endif
IBKR_LIVE_PREFLIGHT_VARS = TRUST_IBKR_ACCOUNT_ID
IBKR_LIVE_E2E_VARS = $(IBKR_LIVE_PREFLIGHT_VARS) TRUST_IBKR_STOCK_SYMBOL TRUST_IBKR_ETF_SYMBOL TRUST_IBKR_BOND_SYMBOL TRUST_IBKR_STOCK_ENTRY_PRICE TRUST_IBKR_STOCK_STOP_PRICE TRUST_IBKR_STOCK_TARGET_PRICE TRUST_IBKR_ETF_ENTRY_PRICE TRUST_IBKR_ETF_STOP_PRICE TRUST_IBKR_ETF_TARGET_PRICE TRUST_IBKR_BOND_ENTRY_PRICE TRUST_IBKR_BOND_STOP_PRICE TRUST_IBKR_BOND_TARGET_PRICE

# Colors for output
RED = \033[0;31m
GREEN = \033[0;32m
YELLOW = \033[0;33m
BLUE = \033[0;34m
NC = \033[0m # No Color

# Default target
.DEFAULT_GOAL := help

.PHONY: native-sqlcipher
native-sqlcipher:
	python3 scripts/build_sqlcipher.py

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
	@echo "  make trust-proof    - Run focused multi-asset risk and IBKR proof suite"
	@echo "  make ibkr-live-env-check - Check required IBKR live E2E env vars"
	@echo "  make ibkr-live-input-check - Validate IBKR live symbols and bracket prices"
	@echo "  make ibkr-live-preflight - Check authenticated IBKR gateway readiness"
	@echo "  make ibkr-live-e2e  - Run authenticated IBKR Client Portal smoke test"
	@echo "  make ibkr-paper-order-e2e - Submit/cancel one explicitly authorized paper order"
	@echo ""
	@echo "$(GREEN)Code Quality Commands:$(NC)"
	@echo "  make fmt            - Format code"
	@echo "  make fmt-check      - Check code formatting"
	@echo "  make lint           - Run clippy linter"
	@echo "  make lint-strict    - Run enhanced clippy with complexity analysis"
	@echo "  make security-check - Run comprehensive security and dependency checks"
	@echo "  make quality-gate   - Run all quality checks (strict)"
	@echo "  make audit          - Check for security vulnerabilities"
	@echo ""
	@echo "$(GREEN)CI Commands:$(NC)"
	@echo "  make ci             - Run full CI pipeline locally"
	@echo "  make ci-fast        - Run quick CI checks (fmt + clippy)"
	@echo "  make ci-test        - Run test suite as in CI"
	@echo "  make ci-perf        - Run performance regression gate"
	@echo "  make ci-build       - Run build checks as in CI"
	@echo "  make ci-snapshots   - Verify CLI report JSON snapshots"
	@echo "  make ci-coverage    - Enforce workspace coverage baseline"
	@echo "  make ci-coverage-strict - Enforce 100% coverage across workspace"
	@echo "  make snapshots-update - Update CLI report JSON snapshots"
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
	@echo "  make perf-gate      - Run sync-lifecycle performance gate"
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
build:
	@echo "$(BLUE)Building project (debug)...$(NC)"
	@$(CARGO) build $(CARGO_FLAGS)

.PHONY: build-release
build-release:
	@echo "$(BLUE)Building project (release)...$(NC)"
	@$(CARGO) build $(CARGO_FLAGS) --release

.PHONY: run
run: build
	@echo "$(BLUE)Running CLI...$(NC)"
	@$(CARGO) run --bin $(CLI_NAME)

# Testing Commands
.PHONY: test
test:
	@echo "$(BLUE)Running tests...$(NC)"
	@$(CARGO) test $(TEST_FLAGS) $(CARGO_FLAGS)

.PHONY: test-single
test-single:
	@echo "$(BLUE)Running tests (single-threaded)...$(NC)"
	@$(CARGO) test $(TEST_FLAGS) $(CARGO_FLAGS) -- --test-threads=1

.PHONY: reliability
reliability:
	@python3 -m unittest discover -s scripts -p test_reliability.py
	@python3 scripts/reliability.py

.PHONY: assurance-status assurance-unit assurance-protocol ibkr-paper-preflight
assurance-status:
	@python3 scripts/assurance.py status

assurance-unit:
	@CARGO=$(CARGO) python3 scripts/assurance.py run --level 1

assurance-protocol:
	@CARGO=$(CARGO) python3 scripts/assurance.py run --level 2

# Explicit real-gateway access; no order submission is selected by this target.
ibkr-paper-preflight:
	@CARGO=$(CARGO) python3 scripts/assurance.py run --level 3 --connect-paper

.PHONY: trust-proof
trust-proof:
	@echo "$(BLUE)Running focused Trust multi-asset proof suite...$(NC)"
	@$(CARGO) test -p core --test risk_invariants_test $(CARGO_FLAGS) -- --test-threads=1
	@$(CARGO) test -p core calculators_fixed_income $(CARGO_FLAGS)
	@$(CARGO) test -p core fractional_qty_policy $(CARGO_FLAGS)
	@$(CARGO) test -p db-sqlite worker_trading_vehicle $(CARGO_FLAGS) -- --test-threads=1
	@$(CARGO) test -p db-sqlite migration_fk_safety_tests $(CARGO_FLAGS) -- --test-threads=1
	@$(CARGO) test -p trust-cli --test integration_test_risk_guards $(CARGO_FLAGS) -- --test-threads=1
	@$(CARGO) test -p trust-cli --test architecture_guard $(CARGO_FLAGS)
	@$(CARGO) test -p trust-cli trading_vehicle $(CARGO_FLAGS) -- --test-threads=1
	@$(CARGO) test -p trust-cli --features legacy-cli --test integration_test_dispatcher_commands $(CARGO_FLAGS) test_bond_terms_update_and_metrics_cli_round_trip -- --test-threads=1
	@$(CARGO) test -p trust-cli --features legacy-cli --test integration_test_dispatcher_commands $(CARGO_FLAGS) test_trading_vehicle_stats_cli_reports_multi_asset_bond_inventory -- --test-threads=1
	@$(CARGO) test -p trust-cli --features legacy-cli --test integration_test_dispatcher_commands $(CARGO_FLAGS) test_trading_vehicle_search_cli_filters_missing_bond_terms_json -- --test-threads=1
	@$(CARGO) test -p ibkr-broker --test http_integration $(CARGO_FLAGS) -- --test-threads=1
	@$(CARGO) test -p ibkr-broker --test live_gateway_smoke_test $(CARGO_FLAGS)

.PHONY: ibkr-live-env-preflight
ibkr-live-env-preflight:
	@missing=""; \
	for var in $(IBKR_LIVE_PREFLIGHT_VARS); do \
		value=$$(printenv "$$var" || true); \
		if [ -z "$$value" ]; then missing="$$missing $$var"; fi; \
	done; \
	if [ -n "$$missing" ]; then \
		echo "$(RED)Missing IBKR live environment variables:$$missing$(NC)"; \
		echo "$(YELLOW)See docs/interactive-brokers-e2e.md for setup.$(NC)"; \
		exit 2; \
	fi

.PHONY: ibkr-live-env-check
ibkr-live-env-check:
	@missing=""; \
	for var in $(IBKR_LIVE_E2E_VARS); do \
		value=$$(printenv "$$var" || true); \
		if [ -z "$$value" ]; then missing="$$missing $$var"; fi; \
	done; \
	if [ -n "$$missing" ]; then \
		echo "$(RED)Missing IBKR live environment variables:$$missing$(NC)"; \
		echo "$(YELLOW)See docs/interactive-brokers-e2e.md for setup.$(NC)"; \
		exit 2; \
	fi

.PHONY: ibkr-live-input-check
ibkr-live-input-check: ibkr-live-env-check
	@echo "$(BLUE)Validating IBKR live E2E symbols and bracket prices...$(NC)"
	@TRUST_IBKR_LIVE_E2E=1 $(CARGO) test -p ibkr-broker --test live_gateway_smoke_test live_e2e_inputs_are_present_and_valid_when_enabled -- --nocapture --test-threads=1

.PHONY: ibkr-live-e2e
ibkr-live-e2e: ibkr-live-input-check ibkr-live-preflight
	@echo "$(BLUE)Running authenticated IBKR live contract and what-if smoke tests...$(NC)"
	@TRUST_IBKR_LIVE_E2E=1 $(CARGO) test -p ibkr-broker --test live_gateway_smoke_test live_gateway_resolves_stock_etf_and_bond_contracts -- --ignored --nocapture --test-threads=1
	@TRUST_IBKR_LIVE_E2E=1 $(CARGO) test -p ibkr-broker --test live_gateway_smoke_test live_gateway_previews_stock_etf_and_bond_brackets_with_whatif -- --ignored --nocapture --test-threads=1

.PHONY: ibkr-paper-order-e2e
ibkr-paper-order-e2e: ibkr-live-input-check ibkr-live-preflight
	@if [ "$$TRUST_IBKR_ALLOW_PAPER_MUTATIONS" != "I_UNDERSTAND_PAPER_ORDERS_ARE_REAL_SIMULATIONS" ]; then \
		echo "$(RED)Paper mutation acknowledgement is missing or incorrect.$(NC)"; \
		echo "$(YELLOW)See docs/interactive-brokers-e2e.md; this target places an order.$(NC)"; \
		exit 2; \
	fi
	@echo "$(BLUE)Submitting and cancelling one authenticated IBKR paper order...$(NC)"
	@TRUST_IBKR_LIVE_E2E=1 $(CARGO) test -p ibkr-broker --test live_gateway_smoke_test live_paper_account_submits_cancels_and_verifies_audit_chain -- --ignored --nocapture --test-threads=1

.PHONY: ibkr-live-preflight
ibkr-live-preflight: ibkr-live-env-preflight
	@echo "$(BLUE)Checking authenticated IBKR gateway readiness...$(NC)"
	@TRUST_IBKR_LIVE_E2E=1 $(CARGO) test -p ibkr-broker --test live_gateway_smoke_test live_gateway_preflight_authenticates_and_selects_account -- --ignored --nocapture --test-threads=1

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
	@$(CARGO) audit --deny warnings

# Enhanced Quality Commands for Financial Application Standards
.PHONY: lint-strict
lint-strict:
	@echo "$(BLUE)Running strict clippy with complexity analysis...$(NC)"
	@$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

.PHONY: security-check
security-check:
	@echo "$(BLUE)Running comprehensive security and dependency checks...$(NC)"
	@echo "$(YELLOW)Checking dependency security and licenses...$(NC)"
	@$(CARGO) deny --all-features check
	@echo "$(YELLOW)Checking for security vulnerabilities...$(NC)"
	@$(CARGO) audit --deny warnings
	@echo "$(YELLOW)Checking for unused dependencies...$(NC)"
	@$(CARGO) machete --with-metadata --skip-target-dir

.PHONY: quality-gate
quality-gate: fmt-check lint-strict security-check
	@echo "$(GREEN)✓ All quality gates passed! Ready for financial application deployment.$(NC)"

# CI Pipeline Commands
.PHONY: ci
ci: ci-fast ci-build ci-test ci-snapshots ci-coverage ci-perf security-check
	@echo "$(GREEN)✓ Full CI pipeline passed!$(NC)"

.PHONY: ci-fast
ci-fast: fmt-check lint
	@echo "$(GREEN)✓ Quick CI checks passed!$(NC)"

.PHONY: ci-test
ci-test:
	@echo "$(BLUE)Running CI test suite...$(NC)"
	@$(CARGO) test $(TEST_FLAGS) $(CARGO_FLAGS)
	@$(CARGO) test --doc $(CARGO_FLAGS)

.PHONY: ci-snapshots
ci-snapshots:
	@echo "$(BLUE)Verifying CLI report JSON snapshots...$(NC)"
	@$(CARGO) test -p trust-cli --features legacy-cli --test integration_test_report_json_snapshots $(CARGO_FLAGS)

.PHONY: ci-coverage
ci-coverage:
	@echo "$(BLUE)Enforcing workspace coverage baseline...$(NC)"
	@cargo llvm-cov --workspace --all-features --locked \
		--ignore-filename-regex '(^|/)test_support\.rs$$' \
		--fail-under-lines $(COVERAGE_MIN_LINES) \
		--fail-under-functions $(COVERAGE_MIN_FUNCTIONS) \
		--fail-under-regions $(COVERAGE_MIN_REGIONS)

.PHONY: ci-coverage-strict
ci-coverage-strict:
	@echo "$(BLUE)Enforcing 100% test coverage (workspace)...$(NC)"
	@cargo llvm-cov --workspace --all-features --locked \
		--ignore-filename-regex '(^|/)test_support\.rs$$' \
		--fail-under-lines 100 \
		--fail-under-functions 100 \
		--fail-under-regions 100

.PHONY: snapshots-update
snapshots-update:
	@echo "$(YELLOW)Updating CLI report JSON snapshots...$(NC)"
	@UPDATE_SNAPSHOTS=1 $(CARGO) test -p trust-cli --features legacy-cli --test integration_test_report_json_snapshots $(CARGO_FLAGS)

.PHONY: ci-perf
ci-perf:
	@echo "$(BLUE)Running CI performance gate (production release profile)...$(NC)"
	@TRUST_PERF_GATE_ITERATIONS=$${TRUST_PERF_GATE_ITERATIONS:-3} \
	TRUST_PERF_GATE_WARMUP_ITERATIONS=$${TRUST_PERF_GATE_WARMUP_ITERATIONS:-1} \
	TRUST_PERF_GATE_MAX_CASE_200_MS=$${TRUST_PERF_GATE_MAX_CASE_200_MS:-26000} \
	TRUST_PERF_GATE_MAX_CASE_211_MS=$${TRUST_PERF_GATE_MAX_CASE_211_MS:-6000} \
	$(CARGO) test --release -p trust-cli --test integration_test_use_cases $(CARGO_FLAGS) -- test_case_212_perf_gate_sync_lifecycle_regression_guard --ignored --nocapture
	@$(MAKE) service-perf

.PHONY: service-perf
service-perf:
	@echo "$(BLUE)Running real-process HTTP/MCP latency gate (offline, release profile)...$(NC)"
	$(CARGO) test --release -p trust-service --test e2e $(CARGO_FLAGS) -- real_process_risk_preview_latency_budget --ignored --exact --nocapture
	$(CARGO) test --release -p trust-service --test journal_reliability $(CARGO_FLAGS) -- large_history_resolution_verification_budget --ignored --exact --nocapture

.PHONY: ci-build
ci-build:
	@echo "$(BLUE)Running CI build checks...$(NC)"
	@$(CARGO) check $(CARGO_FLAGS) --all-features --workspace
	@$(CARGO) check $(CARGO_FLAGS) --no-default-features --workspace
	@$(CARGO) build -p trust-model $(CARGO_FLAGS) --release
	@$(CARGO) build -p core $(CARGO_FLAGS) --release
	@$(CARGO) build -p trust-cli $(CARGO_FLAGS) --release
	@$(CARGO) build --all $(CARGO_FLAGS) --release

# Git Workflow Commands
.PHONY: pre-commit
pre-commit: fmt-check lint-strict test-single
	@echo "$(GREEN)✓ Pre-commit checks passed!$(NC)"

.PHONY: pre-push
pre-push: quality-gate ci-build ci-test ci-snapshots ci-coverage ci-perf
	@echo "$(GREEN)✓ Pre-push checks passed! Safe to push.$(NC)"

# Utility Commands
.PHONY: clean
clean:
	@echo "$(BLUE)Cleaning build artifacts...$(NC)"
	@$(CARGO) clean

.PHONY: perf-gate
perf-gate:
	@echo "$(BLUE)Running performance regression gate (production release profile)...$(NC)"
	@TRUST_PERF_GATE_ITERATIONS=$${TRUST_PERF_GATE_ITERATIONS:-3} \
	TRUST_PERF_GATE_WARMUP_ITERATIONS=$${TRUST_PERF_GATE_WARMUP_ITERATIONS:-1} \
	TRUST_PERF_GATE_MAX_CASE_200_MS=$${TRUST_PERF_GATE_MAX_CASE_200_MS:-26000} \
	TRUST_PERF_GATE_MAX_CASE_211_MS=$${TRUST_PERF_GATE_MAX_CASE_211_MS:-6000} \
	$(CARGO) test --release -p trust-cli --test integration_test_use_cases $(CARGO_FLAGS) -- test_case_212_perf_gate_sync_lifecycle_regression_guard --ignored --nocapture

.PHONY: install-tools
install-tools:
	@echo "$(BLUE)Installing development tools...$(NC)"
	@echo "Installing cargo-audit..."
	@$(CARGO) install cargo-audit --version $(CARGO_AUDIT_VERSION) --locked
	@echo "Installing cargo-nextest..."
	@$(CARGO) install cargo-nextest --locked
	@echo "Installing cargo-llvm-cov..."
	@$(CARGO) install cargo-llvm-cov --version $(CARGO_LLVM_COV_VERSION) --locked
	@echo "Installing cargo-deny..."
	@$(CARGO) install cargo-deny --version $(CARGO_DENY_VERSION) --locked
	@echo "Installing cargo-machete..."
	@$(CARGO) install cargo-machete --version $(CARGO_MACHETE_VERSION) --locked
	@echo ""
	@echo "$(YELLOW)To install 'act' for running GitHub Actions locally:$(NC)"
	@echo "  macOS:    brew install act"
	@echo "  Linux:    curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash"
	@echo "  Windows:  choco install act-cli"
	@echo ""
	@echo "$(YELLOW)To install 'pre-commit' for enhanced git hooks:$(NC)"
	@echo "  pip install pre-commit"
	@echo "  pre-commit install      # Install git hooks"
	@echo "  pre-commit install --hook-type pre-push  # Install pre-push hooks"

# Release Commands
.PHONY: release-local
release-local:
	@echo "$(BLUE)Building all release targets locally...$(NC)"
	@echo "$(YELLOW)Installing required targets...$(NC)"
	@rustup target add aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu || true
	@echo "$(BLUE)Building for aarch64-apple-darwin...$(NC)"
	@$(CARGO) build --release --target aarch64-apple-darwin --bin trust
	@echo "$(BLUE)Building for x86_64-apple-darwin...$(NC)"
	@$(CARGO) build --release --target x86_64-apple-darwin --bin trust
	@echo "$(BLUE)Building for x86_64-unknown-linux-gnu...$(NC)"
	@$(CARGO) build --release --target x86_64-unknown-linux-gnu --bin trust || echo "$(YELLOW)Warning: Linux target may not be available on this platform$(NC)"
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

# Real HTTP/MCP connections to a child service and a loopback IBKR simulator.
.PHONY: e2e
e2e:
	@$(CARGO) test $(CARGO_FLAGS) -p trust-service --test e2e -- --test-threads=1

.PHONY: oidc-acceptance
oidc-acceptance:
	@$(CARGO) build --locked -p trust-service
	@docker build --pull -t trust-keycloak:26.8.1 deploy/keycloak
	@docker build --pull -t trust-postgres:18.6 deploy/postgres
	@npm ci --prefix service/tests/oidc
	@service/tests/oidc/node_modules/.bin/playwright install chromium
	@uv sync --locked --project service/tests/oidc
	@python3 service/tests/oidc/run.py

.PHONY: assurance-mutations
assurance-mutations:
	@CARGO=$(CARGO) python3 scripts/mutation_gate.py

.PHONY: skills-check
skills-check: ## Validate skill contracts, MCP delivery and offline packaging
	@$(CARGO) test --locked -p trust-service --test skills_contract -- --test-threads=1
	@$(CARGO) test --locked -p trust-service --test service_contract mcp_skill_references_discover_read_and_preserve_owner_visibility -- --exact --test-threads=1
	@python3 -m unittest discover -s scripts -p test_package_skills.py
	@python3 -m unittest discover -s scripts -p test_review_agent_eval.py
	@python3 -m unittest discover -s scripts -p test_evaluate_skill_routing.py
