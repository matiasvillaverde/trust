use trust_model::BrokerLog;

pub struct LogView;

impl LogView {
    pub fn display(log: &BrokerLog) {
        println!("{}", log.log);
    }
}
