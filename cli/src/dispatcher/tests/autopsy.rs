#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use rust_decimal_macros::dec;

#[test]
fn test_trade_autopsy_interactive_persists_mistake_for_closed_graded_trade() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (_account, trade) = seed_closed_target_trade(&mut dispatcher);
    dispatcher
        .trust
        .grade_trade(
            trade.id,
            core::services::grading::GradingWeightsPermille::default(),
        )
        .expect("closed trade should grade");
    let trade_id = trade.id.to_string();
    let matches = trade_autopsy_matches(&["--trade-id", &trade_id]);

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_multi_select(Ok(vec![3, 6]));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("held past stop rule".to_string()));
    crate::dialogs::scripted_push_input(Ok("1.5".to_string()));
    crate::dialogs::scripted_push_input(Ok("Exit when the invalidation level breaks.".to_string()));

    crate::dispatcher::autopsy::trade_autopsy(&mut dispatcher.trust, &matches)
        .expect("trade autopsy should persist");

    let mistakes = dispatcher
        .trust
        .mistakes_for_trade(trade.id)
        .expect("mistakes should read");
    assert_eq!(mistakes.len(), 1);
    let mistake = &mistakes[0];
    assert_eq!(mistake.trade_id, trade.id);
    assert!(mistake
        .bias_tags
        .contains(&model::MungerTendency::PainAvoidingDenial));
    assert!(mistake
        .bias_tags
        .contains(&model::MungerTendency::DeprivalSuperreaction));
    assert!(mistake
        .bias_tags
        .contains(&model::MungerTendency::Lollapalooza));
    assert!(mistake.lollapalooza);
    assert_eq!(mistake.error_type, model::MistakeErrorType::Commission);
    assert_eq!(
        mistake.rule_violated.as_deref(),
        Some("held past stop rule")
    );
    assert_eq!(mistake.counterfactual_r, dec!(1.5));
    assert_eq!(mistake.lesson, "Exit when the invalidation level breaks.");
    crate::dialogs::scripted_reset();
}

#[test]
fn test_trade_autopsy_rejects_open_and_ungraded_trades_before_dialogs() {
    let mut open_dispatcher = test_dispatcher();
    let (_account, open_trade) = seed_new_trade(&mut open_dispatcher);
    let open_trade_id = open_trade.id.to_string();
    let open_matches = trade_autopsy_matches(&["--trade-id", &open_trade_id]);
    let error =
        crate::dispatcher::autopsy::trade_autopsy(&mut open_dispatcher.trust, &open_matches)
            .expect_err("open trade should be rejected");
    assert_eq!(error.code, "trade_not_closed");

    let mut ungraded_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (_account, closed_trade) = seed_closed_target_trade(&mut ungraded_dispatcher);
    let closed_trade_id = closed_trade.id.to_string();
    let closed_matches = trade_autopsy_matches(&["--trade-id", &closed_trade_id]);
    let error =
        crate::dispatcher::autopsy::trade_autopsy(&mut ungraded_dispatcher.trust, &closed_matches)
            .expect_err("ungraded trade should be rejected");
    assert_eq!(error.code, "trade_not_graded");
}
