use super::*;
use model::{DraftTrade, OrderCategory, TradeCategory};
impl Domain {
    pub(super) fn trades(&mut self, op: Operation, args: &Value) -> Result<Value> {
        use Operation::*;
        let account = self.account(args)?;
        match op {
            Trades => {
                let filter = model::history::TradeHistoryFilter {
                    status: args
                        .get("status")
                        .and_then(Value::as_str)
                        .map(|status| status.to_ascii_lowercase().parse::<model::Status>())
                        .transpose()
                        .map_err(|_| ServiceError::invalid("Invalid trade status."))?,
                    symbol: args
                        .get("symbol")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    active_only: boolean(args, "open_only"),
                };
                output(
                    self.facade
                        .trade_history_page(account.id, &filter, history_page(args)?),
                )
            }
            TradeCreate => {
                let instrument = self
                    .facade
                    .search_trading_vehicles()
                    .map_err(domain_error)?
                    .into_iter()
                    .find(|v| optional_id(args, "instrument_id").ok().flatten() == Some(v.id))
                    .ok_or_else(not_found)?;
                let draft = DraftTrade {
                    account,
                    trading_vehicle: instrument,
                    quantity: decimal(args, "quantity")?,
                    currency: currency(args)?,
                    category: if string(args, "side")? == "long" {
                        TradeCategory::Long
                    } else {
                        TradeCategory::Short
                    },
                    thesis: optional_string(args, "thesis"),
                    sector: optional_string(args, "sector"),
                    asset_class: optional_string(args, "asset_class"),
                    context: optional_string(args, "context"),
                };
                let category =
                    if args.get("stop_type").and_then(Value::as_str) == Some("stop_limit") {
                        OrderCategory::StopLimit
                    } else {
                        OrderCategory::Stop
                    };
                output(self.facade.create_trade_with_safety_order_category(
                    draft,
                    decimal(args, "stop")?,
                    decimal(args, "entry")?,
                    decimal(args, "target")?,
                    category,
                ))
            }
            SizePreview => output(self.facade.calculate_level_adjusted_quantity(
                account.id,
                decimal(args, "entry")?,
                decimal(args, "stop")?,
                &currency(args)?,
            )),
            Hypothesis => output(self.facade.calculate_trade_hypothesis(
                account.id,
                decimal(args, "entry")?,
                decimal(args, "stop")?,
                decimal(args, "target")?,
                decimal(args, "quantity")?,
                &currency(args)?,
            )),
            Reconcile => {
                let after = optional_id(args, "after_trade_id")?;
                let limit = u16::try_from(uint(args, "batch_size", 1))
                    .ok()
                    .filter(|limit| (1..=500).contains(limit))
                    .ok_or_else(|| ServiceError::invalid("Invalid batch size."))?;
                let mut trades = self
                    .facade
                    .active_trade_batch(account.id, after, limit.saturating_add(1))
                    .map_err(domain_error)?;
                let limit = usize::from(limit);
                let more = trades.len() > limit;
                trades.truncate(limit);
                let next = if more {
                    trades.last().map(|t| t.id)
                } else {
                    None
                };
                let mut results = Vec::new();
                for trade in trades {
                    results.push(match self.facade.sync_trade(&trade, &account) {
                        Ok((status, orders, _)) => {
                            json!({"trade_id":trade.id,"status":status,"orders":orders})
                        }
                        Err(error) => json!({"trade_id":trade.id,"error":domain_error(error)}),
                    });
                }
                Ok(
                    json!({"trades":results,"complete":!more&&results.iter().all(|result|result.get("error").is_none()),"next_after_trade_id":next}),
                )
            }
            _ => {
                let trade = self.trade(args)?;
                match op {
                    TradeGet => value(trade),
                    TradeFund => {
                        self.facade
                            .require_trading_policy(account.id)
                            .map_err(domain_error)?;
                        let (updated, transaction, balance, trade_balance) =
                            self.facade.fund_trade(&trade).map_err(domain_error)?;
                        Ok(
                            json!({"trade":updated,"transaction":transaction,"account_balance":balance,"trade_balance":trade_balance}),
                        )
                    }
                    TradeSubmit => {
                        self.facade
                            .require_trading_policy(account.id)
                            .map_err(domain_error)?;
                        let (updated, _) =
                            self.facade.submit_trade(&trade).map_err(domain_error)?;
                        Ok(json!({"trade":updated,"next_action":"trade_reconcile"}))
                    }
                    TradeCancel => {
                        match trade.status {model::Status::Funded=>{self.facade.cancel_funded_trade(&trade).map_err(domain_error)?;},model::Status::Submitted|model::Status::PartiallyFilled=>{self.facade.cancel_submitted_trade(&trade).map_err(domain_error)?;},_=>return Err(ServiceError::invalid("Only funded or submitted trades can be canceled; inspect trade_get."))};
                        output(self.facade.read_trade(trade.id))
                    }
                    TradeClose => {
                        self.facade.close_trade(&trade).map_err(domain_error)?;
                        Ok(
                            json!({"trade_id":trade.id,"exit_requested":true,"next_action":"trade_reconcile"}),
                        )
                    }
                    TradeSync => {
                        let (status, orders, _) = self
                            .facade
                            .sync_trade(&trade, &account)
                            .map_err(domain_error)?;
                        Ok(json!({"trade_id":trade.id,"status":status,"orders":orders}))
                    }
                    StopModify => output(self.facade.modify_stop(
                        &trade,
                        &account,
                        decimal(args, "price")?,
                    )),
                    TargetModify => output(self.facade.modify_target(
                        &trade,
                        &account,
                        decimal(args, "price")?,
                    )),
                    ManualFill | ManualStop | ManualTarget => {
                        let mut trade = trade;
                        let price = decimal(args, "price")?;
                        if price <= Decimal::ZERO {
                            return Err(ServiceError::invalid("price must be positive."));
                        }
                        match op {
                            ManualFill => trade.entry.average_filled_price = Some(price),
                            ManualStop => trade.safety_stop.average_filled_price = Some(price),
                            ManualTarget => trade.target.average_filled_price = Some(price),
                            _ => return Err(ServiceError::internal()),
                        }
                        let fee = decimal(args, "fee")?;
                        if fee < Decimal::ZERO {
                            return Err(ServiceError::invalid("fee must be nonnegative."));
                        }
                        match op {
                            ManualFill => {
                                self.facade.fill_trade(&trade, fee).map_err(domain_error)?;
                            }
                            ManualStop => {
                                self.facade.stop_trade(&trade, fee).map_err(domain_error)?;
                            }
                            _ => {
                                self.facade
                                    .target_acquired(&trade, fee)
                                    .map_err(domain_error)?;
                            }
                        }
                        output(self.facade.read_trade(trade.id))
                    }
                    BrokerPreview => output(self.facade.preview_broker_order(&account, &trade)),
                    Executions => output(self.facade.executions_for_trade(trade.id)),
                    TradeAudit => Ok(
                        json!({"events":self.facade.trade_audit_events(trade.id).map_err(domain_error)?,"verification":self.facade.trade_audit_verification(trade.id).map_err(domain_error)?}),
                    ),
                    _ => Err(ServiceError::internal()),
                }
            }
        }
    }
}
