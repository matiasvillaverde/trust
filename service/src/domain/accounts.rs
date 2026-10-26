use super::*;
use model::database::TradingVehicleUpsert;
use model::{FixedIncomeTerms, RuleName, TransactionCategory};
impl Domain {
    pub(super) fn accounts(
        &mut self,
        identity: &Identity,
        op: Operation,
        args: &Value,
    ) -> Result<Value> {
        use Operation::*;
        match op {
            AccountsList => value(
                self.facade
                    .search_all_accounts()
                    .map_err(domain_error)?
                    .into_iter()
                    .filter(|a| identity.permits_account(a.id))
                    .collect::<Vec<_>>(),
            ),
            AccountCreate => output(
                self.facade.create_account_with_profile(
                    string(args, "name")?,
                    string(args, "description")?,
                    parse(args, "environment")?,
                    dec_or(args, "taxes_percentage", Decimal::ZERO)?,
                    dec_or(args, "earnings_percentage", Decimal::ZERO)?,
                    match args
                        .get("account_type")
                        .and_then(Value::as_str)
                        .unwrap_or("primary")
                    {
                        "earnings" => model::AccountType::Earnings,
                        "tax_reserve" => model::AccountType::TaxReserve,
                        "reinvestment" => model::AccountType::Reinvestment,
                        "insurance" => model::AccountType::Insurance,
                        _ => model::AccountType::Primary,
                    },
                    optional_id(args, "parent_account_id")?,
                    parse(args, "broker")?,
                    args.get("broker_account_id").and_then(Value::as_str),
                ),
            ),
            Instruments => {
                let mut items = self
                    .facade
                    .search_trading_vehicles()
                    .map_err(domain_error)?;
                items.retain(|v| {
                    args.get("symbol")
                        .and_then(Value::as_str)
                        .is_none_or(|s| v.symbol.eq_ignore_ascii_case(s))
                        && args
                            .get("category")
                            .and_then(Value::as_str)
                            .is_none_or(|s| v.category.to_string() == s)
                        && args
                            .get("broker")
                            .and_then(Value::as_str)
                            .is_none_or(|s| v.broker.eq_ignore_ascii_case(s))
                });
                value(items)
            }
            InstrumentCreate => output(self.facade.create_trading_vehicle(
                string(args, "symbol")?,
                args.get("isin").and_then(Value::as_str),
                &parse(args, "category")?,
                string(args, "broker")?,
            )),
            InstrumentUpdate => {
                let instrument_id = id(args, "instrument_id")?;
                let existing = self
                    .facade
                    .search_trading_vehicles()
                    .map_err(domain_error)?
                    .into_iter()
                    .find(|v| v.id == instrument_id)
                    .ok_or_else(not_found)?;
                let mut terms = existing.fixed_income.unwrap_or(FixedIncomeTerms {
                    face_value: None,
                    annual_coupon_rate_pct: None,
                    maturity_date: None,
                    coupon_frequency_per_year: None,
                });
                if args.get("face_value").is_some() {
                    terms.face_value = Some(decimal(args, "face_value")?);
                }
                if args.get("annual_coupon_rate_pct").is_some() {
                    terms.annual_coupon_rate_pct = Some(decimal(args, "annual_coupon_rate_pct")?);
                }
                if args.get("maturity_date").is_some() {
                    terms.maturity_date = Some(parse(args, "maturity_date")?);
                }
                if args.get("coupon_frequency_per_year").is_some() {
                    terms.coupon_frequency_per_year = Some(
                        u16::try_from(uint(args, "coupon_frequency_per_year", 0))
                            .map_err(|_| ServiceError::invalid("Invalid coupon frequency."))?,
                    );
                }
                let input = TradingVehicleUpsert {
                    symbol: existing.symbol,
                    isin: existing.isin,
                    category: existing.category,
                    broker: existing.broker,
                    broker_asset_id: optional_string(args, "broker_asset_id")
                        .or(existing.broker_asset_id),
                    exchange: optional_string(args, "exchange").or(existing.exchange),
                    broker_asset_class: optional_string(args, "broker_asset_class")
                        .or(existing.broker_asset_class),
                    broker_asset_status: optional_string(args, "broker_asset_status")
                        .or(existing.broker_asset_status),
                    tradable: args
                        .get("tradable")
                        .and_then(Value::as_bool)
                        .or(existing.tradable),
                    marginable: args
                        .get("marginable")
                        .and_then(Value::as_bool)
                        .or(existing.marginable),
                    shortable: args
                        .get("shortable")
                        .and_then(Value::as_bool)
                        .or(existing.shortable),
                    easy_to_borrow: args
                        .get("easy_to_borrow")
                        .and_then(Value::as_bool)
                        .or(existing.easy_to_borrow),
                    fractionable: args
                        .get("fractionable")
                        .and_then(Value::as_bool)
                        .or(existing.fractionable),
                    fixed_income: if terms.is_empty() { None } else { Some(terms) },
                };
                output(self.facade.upsert_trading_vehicle(input))
            }
            InstrumentResolve => {
                let account = self.account(args)?;
                output(self.facade.resolve_instrument(
                    &account,
                    string(args, "symbol")?,
                    parse(args, "category")?,
                ))
            }
            InstrumentStats => {
                let items = self
                    .facade
                    .search_trading_vehicles()
                    .map_err(domain_error)?;
                Ok(
                    json!({"total":items.len(),"bonds_missing_terms":items.iter().filter(|v|v.category==model::TradingVehicleCategory::Bond&&v.fixed_income.is_none()).count()}),
                )
            }
            ConnectionStatus => {
                let accounts = self.facade.search_all_accounts().map_err(domain_error)?;
                value(accounts.into_iter().filter(|a|identity.permits_account(a.id)).map(|a|json!({"account_id":a.id,"broker":a.broker_kind,"environment":a.environment,"broker_account_bound":a.broker_account_id.is_some(),"connectivity":"not_probed","next_action":"market_snapshot"})).collect::<Vec<_>>())
            }
            BackupExport | BackupImport => self.backup(op, args),
            _ => {
                let account = self.account(args)?;
                match op {
                    AccountGet => Ok(
                        json!({"account":account,"balances":self.facade.search_all_balances(account.id).map_err(domain_error)?}),
                    ),
                    AccountDelete => output(
                        self.facade
                            .delete_account(account.id, boolean(args, "force")),
                    ),
                    Balances => output(self.facade.search_all_balances(account.id)),
                    Transfer => output(self.facade.transfer_between_accounts(
                        account.id,
                        id(args, "to_account_id")?,
                        decimal(args, "amount")?,
                        currency(args)?,
                        string(args, "reason")?,
                    )),
                    Transactions => {
                        let (from, to) = period(args)?;
                        value(
                            self.facade
                                .get_account_transactions(account.id)
                                .map_err(domain_error)?
                                .into_iter()
                                .filter(|t| {
                                    t.created_at >= from.naive_utc()
                                        && t.created_at <= to.naive_utc()
                                })
                                .collect::<Vec<_>>(),
                        )
                    }
                    Deposit | Withdraw => {
                        let amount = decimal(args, "amount")?;
                        if amount <= Decimal::ZERO {
                            return Err(ServiceError::invalid("amount must be positive."));
                        }
                        output(self.facade.create_transaction(
                            &account,
                            &if op == Deposit {
                                TransactionCategory::Deposit
                            } else {
                                TransactionCategory::Withdrawal
                            },
                            amount,
                            &currency(args)?,
                        ))
                    }
                    Rules => output(self.facade.search_all_rules(account.id)),
                    RuleCreate => {
                        let percentage = decimal(args, "percentage")?;
                        if percentage < Decimal::ZERO || percentage > Decimal::from(100) {
                            return Err(ServiceError::invalid(
                                "percentage must be between 0 and 100.",
                            ));
                        }
                        // The legacy domain contract stores rule percentages as f32. No arithmetic is done in floating point.
                        let boundary = percentage.to_string().parse::<f32>().map_err(|_| {
                            ServiceError::invalid("Invalid legacy risk percentage.")
                        })?;
                        let rule = if string(args, "kind")? == "per_trade" {
                            RuleName::RiskPerTrade(boundary)
                        } else {
                            RuleName::RiskPerMonth(boundary)
                        };
                        if rule.risk_decimal().ok() != Some(percentage) {
                            return Err(ServiceError::invalid("percentage exceeds the precision supported by the risk-rule contract."));
                        }
                        output(self.facade.create_rule(
                            &account,
                            &rule,
                            string(args, "description")?,
                            &parse(args, "priority")?,
                        ))
                    }
                    RuleRemove => {
                        let rule_id = id(args, "rule_id")?;
                        let rule = self
                            .facade
                            .search_all_rules(account.id)
                            .map_err(domain_error)?
                            .into_iter()
                            .find(|r| r.id == rule_id)
                            .ok_or_else(not_found)?;
                        output(self.facade.deactivate_rule(&rule))
                    }
                    _ => Err(ServiceError::internal()),
                }
            }
        }
    }
    fn backup(&mut self, op: Operation, args: &Value) -> Result<Value> {
        // Never accept a client path. Backups are operator-readable opaque artifacts.
        let directory = self
            .config
            .database
            .parent()
            .ok_or_else(ServiceError::internal)?
            .join("backups");
        std::fs::create_dir_all(&directory).map_err(|_| ServiceError::internal())?;
        let database_path = self
            .config
            .database
            .to_str()
            .ok_or_else(ServiceError::internal)?;
        if op == Operation::BackupExport {
            let backup_id = Uuid::new_v4();
            let path = directory.join(format!("{backup_id}.json"));
            db_sqlite::SqliteDatabase::new(database_path)
                .export_backup_to_path(&path)
                .map_err(domain_error)?;
            Ok(
                json!({"backup_id":backup_id,"scope":"financial_data","control_journal":"Back up the control journal separately with the service stopped."}),
            )
        } else {
            // Replacement can resurrect previously submitted orders and roll back accounting.
            // Only strict import into an empty financial DB is offered online.
            if !self
                .facade
                .search_all_accounts()
                .map_err(domain_error)?
                .is_empty()
            {
                return Err(ServiceError::invalid("Online restore requires an empty financial database; replacement is an offline operator workflow."));
            }
            let path = directory.join(format!("{}.json", id(args, "backup_id")?));
            output(
                db_sqlite::SqliteDatabase::new(database_path)
                    .import_backup_from_path(&path, db_sqlite::ImportOptions::default()),
            )
        }
    }
}
