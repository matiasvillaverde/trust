//! Read-only history facade. Storage owns filtering, ordering and page limits.
use crate::TrustFacade;
use model::{
    history::{HistoryPage, PageRequest, TradeHistoryFilter, TransactionHistoryFilter},
    DistributionHistory, Trade, Transaction,
};
use std::error::Error;
use uuid::Uuid;

impl TrustFacade {
    /// Read a bounded account trade history page.
    pub fn trade_history_page(
        &mut self,
        account_id: Uuid,
        filter: &TradeHistoryFilter,
        page: PageRequest,
    ) -> Result<HistoryPage<Trade>, Box<dyn Error>> {
        self.factory
            .trade_read()
            .history_page(account_id, filter, page)
    }
    /// Read at most `limit` broker-active trades after an exclusive UUID cursor.
    pub fn active_trade_batch(
        &mut self,
        account_id: Uuid,
        after: Option<Uuid>,
        limit: u16,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        self.factory
            .trade_read()
            .active_batch(account_id, after, limit)
    }
    /// Read a bounded account transaction history page.
    pub fn transaction_history_page(
        &mut self,
        account_id: Uuid,
        filter: &TransactionHistoryFilter,
        page: PageRequest,
    ) -> Result<HistoryPage<Transaction>, Box<dyn Error>> {
        self.factory
            .transaction_read()
            .history_page(account_id, filter, page)
    }
    /// Read a bounded account distribution receipt history page.
    pub fn distribution_history_page(
        &mut self,
        account_id: Uuid,
        page: PageRequest,
    ) -> Result<HistoryPage<DistributionHistory>, Box<dyn Error>> {
        self.factory
            .distribution_read()
            .history_page(account_id, page)
    }
}
