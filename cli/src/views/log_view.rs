use model::BrokerLog;

pub struct LogView;

impl LogView {
    pub fn display(log: &BrokerLog) {
        println!();
        println!("{}", Self::header(log));
        println!("{}", Self::body(log));
        println!();
    }

    fn header(log: &BrokerLog) -> String {
        format!("Log: {}", log.id)
    }

    fn body(log: &BrokerLog) -> &str {
        &log.log
    }
}

#[cfg(test)]
mod tests {
    use super::LogView;
    use chrono::Utc;
    use model::BrokerLog;
    use uuid::Uuid;

    fn sample_log() -> BrokerLog {
        BrokerLog {
            id: Uuid::new_v4(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
            trade_id: Uuid::new_v4(),
            log: "broker event".to_string(),
        }
    }

    #[test]
    fn helper_methods_return_expected_strings() {
        let log = sample_log();
        assert!(LogView::header(&log).starts_with("Log: "));
        assert_eq!(LogView::body(&log), "broker event");
    }

    #[test]
    fn display_runs_for_smoke_coverage() {
        LogView::display(&sample_log());
    }
}
