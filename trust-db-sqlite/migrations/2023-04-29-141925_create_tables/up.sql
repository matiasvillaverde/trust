CREATE TABLE accounts (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	name			TEXT NOT NULL UNIQUE,
	description		TEXT NOT NULL
);

CREATE TABLE accounts_overviews (
	id 				TEXT NOT NULL PRIMARY KEY,
	created_at			DATETIME NOT NULL,
	updated_at			DATETIME NOT NULL,
	deleted_at			DATETIME,
	account_id 			TEXT NOT NULL REFERENCES accounts(id),
	total_balance_id	TEXT NOT NULL REFERENCES price (id),
	total_in_trade_id	TEXT NOT NULL REFERENCES price (id),
	total_available_id	TEXT NOT NULL REFERENCES price (id),
	taxed_id	TEXT NOT NULL REFERENCES price (id),
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
	currency 		TEXT CHECK(currency IN ('EUR', 'USD', 'BTC')) NOT NULL,
	category 		TEXT CHECK(category IN ('deposit', 'withdrawal', 'payment_from_trade', 'fund_trade', 'open_trade', 'close_target', "close_safety_stop", "close_safety_stop_slippage", "fee_open", "fee_close", "payment_earnings", "withdrawal_earnings", "payment_tax", "withdrawal_tax")) NOT NULL,
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

CREATE TABLE "orders" (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at			DATETIME NOT NULL,
	updated_at			DATETIME NOT NULL,
	deleted_at			DATETIME,
	price_id			TEXT NOT NULL REFERENCES prices (id),
	quantity			INTEGER NOT NULL,
	trading_vehicle_id	TEXT NOT NULL REFERENCES trading_vehicles (id),
	action 				TEXT CHECK(action IN ('sell', 'buy', 'short')) NOT NULL,
	category 			TEXT CHECK(category IN ('market', 'limit', 'stop')) NOT NULL,
	opened_at			DATETIME,
	closed_at			DATETIME
);

CREATE TABLE "trades" (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at			DATETIME NOT NULL,
	updated_at			DATETIME NOT NULL,
	deleted_at			DATETIME,
	category 			TEXT CHECK(category IN ('long', 'short')) NOT NULL,
	currency 				TEXT CHECK(currency IN ('EUR', 'USD', 'BTC')) NOT NULL,
	trading_vehicle_id	TEXT NOT NULL REFERENCES trading_vehicles (id),
	safety_stop_id 		TEXT NOT NULL REFERENCES orders (id),
	entry_id 			TEXT NOT NULL REFERENCES orders (id),
	target_id 			TEXT NOT NULL REFERENCES orders (id),
	account_id 			TEXT NOT NULL REFERENCES accounts (id),
	approved_at			DATETIME,
	rejected_at			DATETIME,
	opened_at			DATETIME,
	failed_at			DATETIME,
	closed_at			DATETIME,
	rejected_by_rule_id	TEXT REFERENCES rules (id),
	overview_id 			TEXT NOT NULL REFERENCES trades_overviews (id)
);

CREATE TABLE "trades_overviews" (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at				DATETIME NOT NULL,
	updated_at				DATETIME NOT NULL,
	deleted_at				DATETIME,
	funding_id			TEXT NOT NULL REFERENCES prices (id),
	capital_in_market_id		TEXT NOT NULL REFERENCES prices (id),
	capital_out_market_id		TEXT NOT NULL REFERENCES prices (id),
	taxed_id		TEXT NOT NULL REFERENCES prices (id),
	total_performance_id	TEXT NOT NULL REFERENCES prices (id)
);