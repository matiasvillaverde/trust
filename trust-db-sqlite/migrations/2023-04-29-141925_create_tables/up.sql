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