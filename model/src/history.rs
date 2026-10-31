//! Bounded history queries. Filtering happens before pagination in the database.
use crate::Status;
use chrono::NaiveDateTime;
use serde::Serialize;

/// A bounded page request, shared by service and database implementations.
#[derive(Debug, Clone, Copy)]
pub struct PageRequest {
    limit: u16,
    offset: i64,
}
impl PageRequest {
    /// Validate a 1..=500 item storage bound. Clients may apply a lower limit.
    pub fn new(limit: u64, offset: u64) -> Result<Self, &'static str> {
        if !(1..=500).contains(&limit) {
            return Err("History limit must be between 1 and 500");
        }
        Ok(Self {
            limit: u16::try_from(limit).map_err(|_| "Invalid history limit")?,
            offset: i64::try_from(offset).map_err(|_| "History offset is too large")?,
        })
    }
    /// SQL LIMIT, validated at construction.
    pub fn limit(self) -> i64 {
        i64::from(self.limit)
    }
    /// SQL OFFSET, validated at construction.
    pub fn offset(self) -> i64 {
        self.offset
    }
    /// Construct a page with a count from the same database snapshot.
    pub fn finish<T>(self, items: Vec<T>, total: i64) -> Result<HistoryPage<T>, &'static str> {
        let total = u64::try_from(total).map_err(|_| "Invalid history count")?;
        let end = u64::try_from(self.offset)
            .map_err(|_| "Invalid history offset")?
            .checked_add(u64::try_from(items.len()).map_err(|_| "Invalid page length")?)
            .ok_or("History page offset overflow")?;
        Ok(HistoryPage {
            items,
            total,
            next_offset: (end < total).then_some(end),
        })
    }
}

/// Stable history response envelope used by HTTP and MCP.
#[derive(Debug, Serialize)]
pub struct HistoryPage<T> {
    /// Only the requested page, ordered deterministically.
    pub items: Vec<T>,
    /// Number of rows matching the filters, before pagination.
    pub total: u64,
    /// Offset for the next page, or none at the end.
    pub next_offset: Option<u64>,
}

/// Trade history predicates. `active_only` means broker-active trades only.
#[derive(Debug, Default)]
pub struct TradeHistoryFilter {
    /// Optional exact lifecycle status.
    pub status: Option<Status>,
    /// Optional case-insensitive, literal instrument symbol (not a wildcard).
    pub symbol: Option<String>,
    /// Restrict to submitted, partially filled, or filled trades.
    pub active_only: bool,
}

/// Inclusive UTC bounds for ledger transactions, across all account currencies.
#[derive(Debug)]
pub struct TransactionHistoryFilter {
    /// Earliest included timestamp.
    pub from: NaiveDateTime,
    /// Latest included timestamp.
    pub to: NaiveDateTime,
}
