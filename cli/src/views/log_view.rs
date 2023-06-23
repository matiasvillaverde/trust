use model::BrokerLog;

pub struct LogView;

impl LogView {
    pub fn display(log: &BrokerLog) {
        println!();
        println!("Log: {}", log.id);
        println!("{}", log.log);
        println!();
    }
}
