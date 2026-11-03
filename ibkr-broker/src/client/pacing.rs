//! Gateway-wide pacing survives the short-lived clients used by broker operations.
//! Limits: https://www.interactivebrokers.com/docs/web-api/v1/pacing-limitations
use reqwest::blocking::{RequestBuilder, Response};
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const GLOBAL_INTERVAL: Duration = Duration::from_millis(100);
const LIST_INTERVAL: Duration = Duration::from_secs(5);
const HISTORY_INTERVAL: Duration = Duration::from_millis(1200);
const PENALTY_INTERVAL: Duration = Duration::from_secs(15 * 60);

#[derive(Default, Debug)]
struct Schedule {
    global: Option<Instant>,
    lists: HashMap<&'static str, Instant>,
    blocked_until: Option<Instant>,
}

impl Schedule {
    fn wait(&self, path: &str, now: Instant) -> Duration {
        let endpoint = limited_endpoint(path).and_then(|(key, _)| self.lists.get(key));
        [self.global.as_ref(), endpoint, self.blocked_until.as_ref()]
            .into_iter()
            .flatten()
            .map(|deadline| deadline.saturating_duration_since(now))
            .max()
            .unwrap_or_default()
    }

    fn completed(&mut self, path: &str, now: Instant) -> Result<(), Box<dyn Error>> {
        self.global = Some(
            now.checked_add(GLOBAL_INTERVAL)
                .ok_or("Pacing clock overflow")?,
        );
        if let Some((key, interval)) = limited_endpoint(path) {
            self.lists.insert(
                key,
                now.checked_add(interval).ok_or("Pacing clock overflow")?,
            );
        }
        Ok(())
    }
}

fn limited_endpoint(path: &str) -> Option<(&'static str, Duration)> {
    let key = match path {
        "/iserver/account/orders" => "orders",
        "/iserver/account/trades" => "trades",
        "/portfolio/accounts" => "portfolio_accounts",
        "/portfolio/subaccounts" => "portfolio_subaccounts",
        "/iserver/account/pnl/partitioned" => "pnl",
        "/iserver/marketdata/history" => return Some(("history", HISTORY_INTERVAL)),
        _ => return None,
    };
    Some((key, LIST_INTERVAL))
}

type SharedSchedule = Arc<Mutex<Schedule>>;
static GATEWAYS: OnceLock<Mutex<HashMap<String, SharedSchedule>>> = OnceLock::new();

#[derive(Debug)]
pub(super) struct Pacer(SharedSchedule);

impl Pacer {
    pub(super) fn for_gateway(url: &str) -> Result<Self, Box<dyn Error>> {
        let url = reqwest::Url::parse(url)?;
        let origin = url.origin().ascii_serialization();
        let mut gateways = GATEWAYS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| "IBKR pacing registry unavailable")?;
        Ok(Self(gateways.entry(origin).or_default().clone()))
    }

    pub(super) fn send(
        &self,
        path: &str,
        request: RequestBuilder,
    ) -> Result<Response, Box<dyn Error>> {
        let mut schedule = self.0.lock().map_err(|_| "IBKR pacing state unavailable")?;
        let now = Instant::now();
        // Do not hold the sole financial worker for a broker penalty window.
        // No request is sent and no mutating request is automatically replayed.
        if schedule
            .blocked_until
            .is_some_and(|deadline| deadline > now)
        {
            return Err(
                "IBKR pacing cooldown is active; reconcile after the broker cooldown".into(),
            );
        }
        std::thread::sleep(schedule.wait(path, now));
        // Keep the gateway lock through dispatch: delayed threads cannot bunch requests
        // together after reserving slots. Conservatively pace from response completion.
        let result = request.send();
        schedule.completed(path, Instant::now())?;
        if result
            .as_ref()
            .is_ok_and(|response| response.status().as_u16() == 429)
        {
            let retry_after = result
                .as_ref()
                .ok()
                .and_then(|r| r.headers().get(reqwest::header::RETRY_AFTER))
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or(PENALTY_INTERVAL)
                .max(PENALTY_INTERVAL);
            schedule.blocked_until = Some(
                Instant::now()
                    .checked_add(retry_after)
                    .ok_or("IBKR pacing cooldown overflow")?,
            );
        }
        result.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_and_global_deadlines_are_independent() {
        let mut schedule = Schedule::default();
        let now = Instant::now();
        schedule.completed("/iserver/account/orders", now).unwrap();
        assert_eq!(schedule.wait("/iserver/account/orders", now), LIST_INTERVAL);
        assert_eq!(
            schedule.wait("/iserver/account/trades", now),
            GLOBAL_INTERVAL
        );
        assert_eq!(
            schedule.wait("/iserver/accounts", now + GLOBAL_INTERVAL),
            Duration::ZERO
        );
        schedule
            .completed("/iserver/marketdata/history", now + GLOBAL_INTERVAL)
            .unwrap();
        assert_eq!(
            schedule.wait("/iserver/marketdata/history", now + GLOBAL_INTERVAL),
            HISTORY_INTERVAL
        );
        assert_eq!(
            schedule.wait("/iserver/account/orders", now + LIST_INTERVAL),
            Duration::ZERO
        );
    }
}
