CREATE TABLE accounts (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	name			TEXT NOT NULL UNIQUE,
	description		TEXT NOT NULL,
	environment		TEXT NOT NULL,
	taxes_percentage 			TEXT NOT NULL,
	earnings_percentage 		TEXT NOT NULL
);

CREATE TABLE accounts_balances (
	id 				TEXT NOT NULL PRIMARY KEY,
	created_at			DATETIME NOT NULL,
	updated_at			DATETIME NOT NULL,
	deleted_at			DATETIME,
	account_id 			TEXT NOT NULL REFERENCES accounts(id),
	total_balance	TEXT NOT NULL,
	total_in_trade	TEXT NOT NULL,
	total_available	TEXT NOT NULL,
	taxed			TEXT NOT NULL,
	currency	 		TEXT CHECK(currency IN ('EUR', 'USD', 'BTC')) NOT NULL,
	total_earnings	TEXT NOT NULL
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

CREATE TABLE transactions (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	currency 		TEXT CHECK(currency IN ('EUR', 'USD', 'BTC')) NOT NULL,
	category 		TEXT CHECK(category IN ('deposit', 'withdrawal', 'payment_from_trade', 'fund_trade', 'open_trade', 'close_target', "close_safety_stop", "close_safety_stop_slippage", "fee_open", "fee_close", "payment_earnings", "withdrawal_earnings", "payment_tax", "withdrawal_tax")) NOT NULL,
	amount			TEXT NOT NULL,
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
	broker_order_id			TEXT,
	created_at				DATETIME NOT NULL,
	updated_at				DATETIME NOT NULL,
	deleted_at				DATETIME,
	unit_price				TEXT NOT NULL,
	currency	 			TEXT CHECK(currency IN ('USD', 'EUR', 'BTC')) NOT NULL,
	quantity				INTEGER NOT NULL,
	category 				TEXT CHECK(category IN ('market', 'limit', 'stop')) NOT NULL,
	trading_vehicle_id		TEXT NOT NULL REFERENCES trading_vehicles (id),
	action 					TEXT CHECK(action IN ('sell', 'buy', 'short')) NOT NULL,
	status 					TEXT CHECK(status IN ('new', 'replaced', 'partially_filled', 'filled', 'done_for_day', 'canceled', 'expired', 'accepted', 'pending_new', 'accepted_for_bidding', 'pending_cancel', 'pending_replace', 'stopped', 'rejected', 'suspended', 'calculated', 'held', 'unknown')) NOT NULL,
	time_in_force 			TEXT CHECK(time_in_force IN ('until_canceled', 'day', 'until_market_open', 'until_market_close')) NOT NULL,
	trailing_percentage		TEXT,
	trailing_price			TEXT,
	filled_quantity			INTEGER,
	average_filled_price	TEXT,
	extended_hours			BOOLEAN NOT NULL,
	submitted_at			DATETIME,
	filled_at				DATETIME,
	expired_at				DATETIME,
	cancelled_at			DATETIME,
	closed_at				DATETIME
);

CREATE TABLE "trades" (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at			DATETIME NOT NULL,
	updated_at			DATETIME NOT NULL,
	deleted_at			DATETIME,
	category 			TEXT CHECK(category IN ('long', 'short')) NOT NULL,
	status 				TEXT CHECK(status IN ('new', 'funded', 'submitted' , 'partially_filled', 'filled', 'canceled', 'expired', 'rejected', 'closed_stop_loss', 'closed_target')) NOT NULL,
	currency 			TEXT CHECK(currency IN ('USD', 'EUR', 'BTC')) NOT NULL,
	trading_vehicle_id	TEXT NOT NULL REFERENCES trading_vehicles (id),
	safety_stop_id 		TEXT NOT NULL REFERENCES orders (id),
	entry_id 			TEXT NOT NULL REFERENCES orders (id),
	target_id 			TEXT NOT NULL REFERENCES orders (id),
	account_id 			TEXT NOT NULL REFERENCES accounts (id),
	balance_id 		TEXT NOT NULL REFERENCES trades_balances (id)
);

CREATE TABLE "trades_balances" (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at				DATETIME NOT NULL,
	updated_at				DATETIME NOT NULL,
	deleted_at				DATETIME,
	currency 				TEXT CHECK(currency IN ('USD', 'EUR', 'BTC')) NOT NULL,
	funding				TEXT NOT NULL,
	capital_in_market	TEXT NOT NULL,
	capital_out_market	TEXT NOT NULL,
	taxed				TEXT NOT NULL,
	total_performance	TEXT NOT NULL
);

CREATE TABLE "logs" (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at	DATETIME NOT NULL,
	updated_at	DATETIME NOT NULL,
	deleted_at	DATETIME,
	log			TEXT NOT NULL,
	trade_id	TEXT NOT NULL REFERENCES trades (id)
);