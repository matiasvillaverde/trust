CREATE TABLE accounts (
	id 			TEXT NOT NULL PRIMARY KEY,
	created_at		DATETIME NOT NULL,
	updated_at		DATETIME NOT NULL,
	deleted_at		DATETIME,
	name			TEXT NOT NULL UNIQUE,
	description		TEXT NOT NULL
);
