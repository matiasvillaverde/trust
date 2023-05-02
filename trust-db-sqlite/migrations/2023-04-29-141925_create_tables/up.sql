CREATE TABLE accounts (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	name			TEXT NOT NULL UNIQUE,
	description		TEXT NOT NULL
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
