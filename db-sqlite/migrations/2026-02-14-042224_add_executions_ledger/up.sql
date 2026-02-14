CREATE TABLE executions (
  id TEXT NOT NULL PRIMARY KEY,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL,
  deleted_at DATETIME,

  broker TEXT NOT NULL,
  source TEXT CHECK(source IN ('trade_updates', 'account_activities')) NOT NULL,

  account_id TEXT NOT NULL REFERENCES accounts(id),
  trade_id TEXT REFERENCES trades(id),
  order_id TEXT REFERENCES orders(id),

  broker_execution_id TEXT NOT NULL,
  broker_order_id TEXT,

  symbol TEXT NOT NULL,
  side TEXT CHECK(side IN ('buy', 'sell', 'sell_short')) NOT NULL,

  qty TEXT NOT NULL,
  price TEXT NOT NULL,
  executed_at DATETIME NOT NULL,

  raw_json TEXT
);

CREATE UNIQUE INDEX idx_executions_broker_account_execution
ON executions(broker, account_id, broker_execution_id);

CREATE INDEX idx_executions_trade_time_active
ON executions(trade_id, executed_at)
WHERE deleted_at IS NULL;

CREATE INDEX idx_executions_order_time_active
ON executions(order_id, executed_at)
WHERE deleted_at IS NULL;
