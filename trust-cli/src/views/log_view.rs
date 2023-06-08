use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use trust_model::BrokerLog;

#[derive(Tabled)]
pub struct LogView {
    pub log: String,
}

impl LogView {
    fn new(l: &BrokerLog) -> LogView {
        LogView { log: l.log.clone() }
    }

    pub fn display(log: &BrokerLog) {
        LogView::display_logs(vec![log]);
    }

    pub fn display_logs(logs: Vec<&BrokerLog>) {
        let views: Vec<LogView> = logs
            .into_iter()
            .map(|r: &BrokerLog| LogView::new(r))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}
