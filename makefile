# Rust Project Makefile

# Set the name of your CLI executable
CLI_NAME = trust-cli

# Set the path to your database migrations folder
MIGRATIONS_DIRECTORY = ./trust-db-sqlite/migrations

# Set the name of your Diesel DB configuration file
DIESEL_CONFIG_FILE = ./trust-db-sqlite/diesel.toml

# Set the path to your Diesel DB URL
CLI_DATABASE_URL = ~/.trust/debug.db

# Set the path to your Diesel CLI executable
DIESEL_CLI = diesel

# Set the path to your Rust compiler executable
RUSTC = rustc

# Set the path to your Cargo executable
CARGO = cargo

# Define the default target
.DEFAULT_GOAL := run

.PHONY: setup
setup:
	$(DIESEL_CLI) setup --config-file $(DIESEL_CONFIG_FILE) --database-url $(CLI_DATABASE_URL)

.PHONY: build
build: setup
	$(CARGO) build

.PHONY: run
run: build
	$(CARGO) run --bin $(CLI_NAME)

.PHONY: test
test: setup
	$(CARGO) test

.PHONY: clean-db
clean-db:
	$(DIESEL_CLI) migration redo --config-file $(DIESEL_CONFIG_FILE) --database-url $(CLI_DATABASE_URL)

.PHONY: delete-db
delete-db:
	rm -fr $(CLI_DATABASE_URL)

.PHONY: migration
migration:
	$(DIESEL_CLI) migration run --config-file $(DIESEL_CONFIG_FILE) --database-url $(CLI_DATABASE_URL)