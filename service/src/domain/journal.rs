use super::*;
use model::{Mistake, SessionPlan, SessionPlanClose};
use trust_core::services::grading::GradingWeightsPermille;
impl Domain {
    pub(super) fn journal(&mut self, op: Operation, args: &Value) -> Result<Value> {
        use Operation::*;
        let account_id = id(args, "account_id")?;
        let now = Utc::now().naive_utc();
        match op {
            SessionList => {
                let (from, to) = period(args)?;
                output(self.facade.session_plans_for_account(
                    account_id,
                    from.naive_utc(),
                    to.naive_utc(),
                ))
            }
            SessionOpen => output(
                self.facade.create_session_plan(SessionPlan {
                    id: Uuid::new_v4(),
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                    account_id,
                    opened_at: now,
                    closed_at: None,
                    regime: parse(args, "regime")?,
                    permitted_setups: strings(args, "setups")?,
                    max_positions: i32::try_from(uint(args, "max_positions", 0))
                        .map_err(|_| ServiceError::invalid("Invalid max_positions."))?,
                    hypothesis: string(args, "hypothesis")?.into(),
                    success_criteria: string(args, "success_criteria")?.into(),
                    failure_criteria: string(args, "failure_criteria")?.into(),
                    session_grade: None,
                    adherence_notes: None,
                }),
            ),
            SessionClose => {
                let session = self
                    .facade
                    .open_session_for_account(account_id)
                    .map_err(domain_error)?
                    .ok_or_else(not_found)?;
                output(self.facade.close_session_plan(SessionPlanClose {
                    session_plan_id: session.id,
                    closed_at: now,
                    session_grade: Some(string(args, "grade")?.into()),
                    adherence_notes: Some(string(args, "notes")?.into()),
                }))
            }
            GradeSummary => output(
                self.facade
                    .trade_grades_for_account_days(account_id, days(args)?),
            ),
            _ => {
                let trade = self.trade(args)?;
                match op {
                    Events => output(self.facade.trade_events_for_trade(trade.id)),
                    EventCreate => output(self.facade.create_trade_event(
                        trade.id,
                        parse(args, "event_type")?,
                        parse(args, "event_date")?,
                        parse(args, "severity")?,
                        optional_string(args, "notes"),
                    )),
                    Mistakes => output(self.facade.mistakes_for_trade(trade.id)),
                    MistakeCreate => {
                        let bias_tags = strings(args, "tendencies")?
                            .iter()
                            .map(|v| {
                                v.parse()
                                    .map_err(|_| ServiceError::invalid("Unknown Munger tendency."))
                            })
                            .collect::<Result<Vec<_>>>()?;
                        output(self.facade.create_mistake(Mistake {
                            id: Uuid::new_v4(),
                            created_at: now,
                            updated_at: now,
                            deleted_at: None,
                            trade_id: trade.id,
                            bias_tags,
                            lollapalooza: boolean(args, "lollapalooza"),
                            error_type: parse(args, "error_type")?,
                            rule_violated: optional_string(args, "violated_rule"),
                            counterfactual_r: dec_or(args, "counterfactual_r", Decimal::ZERO)?,
                            lesson: string(args, "lesson")?.into(),
                        }))
                    }
                    GradeGet => output(self.facade.latest_trade_grade(trade.id)),
                    GradeCompute => output(
                        self.facade
                            .compute_trade_grade(trade.id, GradingWeightsPermille::default()),
                    ),
                    GradeRecord => output(
                        self.facade
                            .grade_trade(trade.id, GradingWeightsPermille::default()),
                    ),
                    _ => Err(ServiceError::internal()),
                }
            }
        }
    }
}
