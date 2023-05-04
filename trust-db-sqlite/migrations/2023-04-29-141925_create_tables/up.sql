CREATE TABLE accounts (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	name			TEXT NOT NULL UNIQUE,
	description		TEXT NOT NULL
);

CREATE TABLE account_overviews (
	id 				TEXT NOT NULL PRIMARY KEY,
	created_at			DATETIME NOT NULL,
	updated_at			DATETIME NOT NULL,
	deleted_at			DATETIME,
	account_id 			TEXT NOT NULL REFERENCES accounts(id),
	total_balance_id	TEXT NOT NULL REFERENCES price (id),
	total_in_trade_id	TEXT NOT NULL REFERENCES price (id),
	total_available_id	TEXT NOT NULL REFERENCES price (id),
	total_taxable_id	TEXT NOT NULL REFERENCES price (id),
	currency	 		TEXT CHECK(currency IN ('EUR', 'USD', 'BTC')) NOT NULL
);

CREATE TABLE rules (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	name 			TEXT CHECK(name IN ('risk_per_trade', 'risk_per_month')) NOT NULL,
	risk			INTEGER NOT NULL,
	description		TEXT NOT NULL,
	priority		INTEGER NOT NULL,
	level 			TEXT CHECK(level IN ('advice', 'warning', 'error')) NOT NULL,
	account_id 		TEXT NOT NULL REFERENCES accounts(id),
	active			BOOLEAN NOT NULL
);

CREATE TABLE prices (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	currency 		TEXT CHECK(currency IN ('EUR', 'USD', 'BTC')) NOT NULL,
	amount			TEXT NOT NULL
);

CREATE TABLE transactions (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	category 		TEXT CHECK(category IN ('deposit', 'withdrawal', 'output', 'input', 'input_tax', 'output_tax')) NOT NULL,
	price_id		TEXT NOT NULL REFERENCES price (id),
	account_id 		TEXT NOT NULL REFERENCES accounts(id),
	trade_id		TEXT REFERENCES trades (uuid)
);

CREATE TABLE "trading_vehicles" (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	symbol			TEXT NOT NULL,
	isin			TEXT NOT NULL UNIQUE,
	category 		TEXT CHECK(category IN ('crypto', 'fiat', 'stock')) NOT NULL,
	broker 			TEXT NOT NULL
);
