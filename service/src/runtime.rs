//! A single worker owns synchronous, non-Send domain dependencies and durable intent.
use crate::{
    auth::{Grant, Identity},
    catalog::{self, Operation},
    config::Config,
    domain::{self, Domain},
    error::{Result, ServiceError},
    journal::{Claim, Journal, Reply},
};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use uuid::Uuid;

struct Request {
    identity: Identity,
    tool: String,
    arguments: Value,
    request_id: Uuid,
    reply: oneshot::Sender<Reply>,
}
#[derive(Clone)]
pub struct Service {
    sender: mpsc::SyncSender<Request>,
    ready: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
}
struct ReadyGuard(Arc<AtomicBool>);
impl Drop for ReadyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub struct Worker {
    handle: std::thread::JoinHandle<()>,
    stopping: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}

impl Service {
    pub fn start<F>(config: Arc<Config>, factory: F) -> Result<(Self, Worker)>
    where
        F: FnOnce() -> Result<trust_core::TrustFacade> + Send + 'static,
    {
        config.validate()?;
        let (sender, receiver) = mpsc::sync_channel::<Request>(config.queue_capacity);
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let ready = Arc::new(AtomicBool::new(false));
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_ready = ready.clone();
        let worker_stopping = stopping.clone();
        let handle = std::thread::Builder::new()
            .name("trust-financial-worker".into())
            .spawn(move || {
                let _ready_guard = ReadyGuard(worker_ready.clone());
                let initialized = (|| -> Result<_> {
                    let locks = lock_storage(&config)?;
                    let journal = Journal::open(&config.journal)?;
                    let facade = factory()?;
                    Ok((
                        locks,
                        journal,
                        Domain {
                            facade,
                            config: config.clone(),
                        },
                    ))
                })();
                let (locks, mut journal, mut domain) = match initialized {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = startup_tx.send(Err(error));
                        return;
                    }
                };
                worker_ready.store(true, Ordering::Release);
                let _ = startup_tx.send(Ok(()));
                let mut next_reconcile = Instant::now();
                let mut reconcile_cursor = None;
                while !worker_stopping.load(Ordering::Acquire) {
                    match receiver.recv_timeout(Duration::from_millis(250)) {
                        Ok(request) => {
                            if request.reply.is_closed() {
                                continue;
                            }
                            let result = execute(
                                &mut domain,
                                &mut journal,
                                &request.identity,
                                &request.tool,
                                &request.arguments,
                                request.request_id,
                            );
                            let fatal = result
                                .as_ref()
                                .err()
                                .is_some_and(|e| e.code == "audit_unavailable");
                            let reply = result.unwrap_or_else(|e| {
                                Reply::from_result(request.request_id, None, Err(e))
                            });
                            let _ = request.reply.send(reply);
                            if fatal {
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if config.reconcile_interval_seconds > 0
                        && next_reconcile.elapsed()
                            >= Duration::from_secs(config.reconcile_interval_seconds)
                    {
                        if reconcile(&mut domain, &mut journal, &mut reconcile_cursor).is_err() {
                            break;
                        }
                        next_reconcile = Instant::now();
                    }
                }
                worker_ready.store(false, Ordering::Release);
                drop(locks);
            })
            .map_err(|_| ServiceError::internal())?;
        startup_rx.recv().map_err(|_| ServiceError::internal())??;
        Ok((
            Self {
                sender,
                ready: ready.clone(),
                stopping: stopping.clone(),
            },
            Worker {
                handle,
                stopping,
                ready,
            },
        ))
    }
    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire) && !self.stopping.load(Ordering::Acquire)
    }
    pub async fn call(
        &self,
        identity: Identity,
        tool: String,
        arguments: Value,
        request_id: Uuid,
    ) -> Reply {
        let error = |e| Reply::from_result(request_id, None, Err(e));
        if !self.ready() {
            return error(unavailable());
        }
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .try_send(Request {
                identity,
                tool,
                arguments,
                request_id,
                reply: tx,
            })
            .is_err()
        {
            return error(ServiceError::new(
                "busy",
                "The service queue is full or unavailable.",
                "Retry with the same idempotency key after a short delay.",
            ));
        }
        match tokio::time::timeout(Duration::from_secs(45), rx).await {
            Ok(Ok(reply)) => reply,
            _ => error(ServiceError::new(
                "outcome_unknown",
                "No completion response was received; the worker may still be executing.",
                "Use operation_get with the original key. Never repeat this intent with a new key.",
            )),
        }
    }
}
impl Worker {
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
        self.ready.store(false, Ordering::Release);
    }
    pub async fn shutdown(self) -> Result<()> {
        self.stop();
        tokio::task::spawn_blocking(move || self.handle.join())
            .await
            .map_err(|_| ServiceError::internal())?
            .map_err(|_| ServiceError::internal())
    }
}
fn unavailable() -> ServiceError {
    ServiceError::new(
        "unavailable",
        "The financial worker is not ready.",
        "Check readiness and operator logs before retrying the same intent.",
    )
}
fn audit_unavailable() -> ServiceError {
    ServiceError::new(
        "audit_unavailable",
        "The durable operation journal could not confirm a write; the worker has stopped.",
        "The operator must inspect storage and reconcile pending operations before restarting.",
    )
}

fn lock_storage(config: &Config) -> Result<Vec<File>> {
    let mut locks = Vec::new();
    let mut canonical = Vec::new();
    for path in [&config.database, &config.journal] {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| ServiceError::internal())?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path).map_err(|_| ServiceError::internal())?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| {
            ServiceError::new(
                "storage_locked",
                "Another service owns this database or journal.",
                "Run exactly one financial worker per database.",
            )
        })?;
        canonical.push(std::fs::canonicalize(path).map_err(|_| ServiceError::internal())?);
        locks.push(file);
    }
    if canonical.first() == canonical.last() {
        return Err(ServiceError::invalid(
            "Financial and control databases must be separate files.",
        ));
    }
    Ok(locks)
}

fn execute(
    domain: &mut Domain,
    journal: &mut Journal,
    identity: &Identity,
    name: &str,
    args: &Value,
    request_id: Uuid,
) -> Result<Reply> {
    let spec = catalog::find(name)?;
    catalog::validate(&spec, args)?;
    let account = args
        .get("account_id")
        .and_then(Value::as_str)
        .and_then(|v| v.parse().ok());
    let accounts = match domain.authorize(identity, &spec, args) {
        Ok(ids) => ids,
        Err(error) => {
            journal
                .record(identity, request_id, name, account, "denied")
                .map_err(|_| audit_unavailable())?;
            return Err(error);
        }
    };
    let operation_id = if spec.mutation {
        let key = domain::string(args, "idempotency_key")?;
        match journal
            .claim(identity, key, name, account, args, request_id, &accounts)
            .map_err(|error| {
                if error.code == "service_unavailable" {
                    audit_unavailable()
                } else {
                    error
                }
            })? {
            Claim::New(id) => Some(id),
            Claim::Existing(reply) => return Ok(reply),
        }
    } else {
        None
    };
    let result = match spec.operation {
        Operation::OperationGet => journal.operation(identity, domain::string(args, "key")?),
        Operation::AuditList => journal.list(
            identity,
            args.get("after").and_then(Value::as_u64).unwrap_or(0),
            args.get("limit").and_then(Value::as_u64).unwrap_or(100),
        ),
        Operation::AuditVerify => journal.verify(),
        _ => domain.dispatch(identity, spec.operation, args),
    };
    // A domain error can follow an external side effect. Do not claim rollback.
    let result = result.map_err(|error| {
        if spec.mutation && error.code == "domain_rejected" {
            ServiceError::new("outcome_unknown", error.message, &error.next_action)
        } else {
            error
        }
    });
    let result = result.map(|value| {
        if spec.mutation {
            value
        } else {
            domain::paginate(value, args)
        }
    });
    let mut reply = Reply::from_result(request_id, operation_id, result);
    if reply.error.is_none() {
        reply.next_actions = match spec.operation {
            Operation::TradeCreate => vec!["broker_preview".into(), "trade_fund".into()],
            Operation::TradeFund => vec!["trade_submit".into()],
            Operation::TradeSubmit
            | Operation::TradeClose
            | Operation::StopModify
            | Operation::TargetModify => vec!["trade_reconcile".into()],
            _ => Vec::new(),
        };
    }
    // Bound persisted and returned bodies. Never turn a successful mutation into a replay.
    if serde_json::to_vec(&reply)
        .map_err(|_| ServiceError::internal())?
        .len()
        > 1_048_576
    {
        reply.data = None;
        reply.error=Some(ServiceError::new("result_too_large","The result exceeded the response limit.","Use a narrower query or pagination. A mutation may have completed; inspect its resulting records."));
    }
    if spec.mutation {
        journal
            .finish(identity, name, account, &reply)
            .map_err(|_| audit_unavailable())?;
    } else {
        journal
            .record(
                identity,
                request_id,
                name,
                account,
                if reply.error.is_none() {
                    "read"
                } else {
                    "read_failed"
                },
            )
            .map_err(|_| audit_unavailable())?;
    }
    tracing::info!(event="operation",request_id=%request_id,operation_id=?operation_id,tool=spec.name,outcome=if reply.error.is_some(){"error"}else{"ok"},"Trust operation finished");
    Ok(reply)
}

fn reconcile(
    domain: &mut Domain,
    journal: &mut Journal,
    cursor: &mut Option<(Uuid, Uuid)>,
) -> Result<()> {
    let accounts = match domain.facade.search_all_accounts() {
        Ok(accounts) => accounts,
        Err(_) => {
            tracing::warn!(
                event = "reconciliation_failed",
                "Cannot read accounts for reconciliation"
            );
            return Ok(());
        }
    };
    let scopes: std::collections::BTreeSet<_> =
        ["trust:reconcile".to_owned()].into_iter().collect();
    let identity = Identity {
        subject: "system:reconciler".into(),
        client_id: None,
        expires_at: u64::MAX,
        scopes: scopes.clone(),
        grant: Grant {
            accounts: Default::default(),
            all_accounts: true,
            scopes,
            allow_live: domain.config.allow_live,
        },
    };
    let mut candidates = Vec::new();
    for account in accounts {
        if account.environment == model::Environment::Live && !domain.config.allow_live {
            continue;
        }
        match domain.facade.account_trades(account.id) {
            Ok(trades) => candidates.extend(
                trades
                    .into_iter()
                    .filter(|t| domain::active(t.status))
                    .map(|t| (account.id, t.id)),
            ),
            Err(_) => {
                tracing::warn!(event="reconciliation_failed",account_id=%account.id,"Cannot enumerate trades")
            }
        }
    }
    candidates.sort_unstable();
    let next = candidates
        .iter()
        .find(|id| cursor.is_none_or(|last| **id > last))
        .or_else(|| candidates.first())
        .copied();
    if let Some((account, trade)) = next {
        *cursor = Some((account, trade));
        let args = json!({"account_id":account,"trade_id":trade,"idempotency_key":Uuid::new_v4().to_string()});
        match execute(
            domain,
            journal,
            &identity,
            "trade_reconcile",
            &args,
            Uuid::new_v4(),
        ) {
            Err(error) if error.code == "audit_unavailable" => return Err(error),
            Err(_) => {
                tracing::warn!(event="reconciliation_failed",account_id=%account,trade_id=%trade,"Background reconciliation failed")
            }
            Ok(reply) if reply.error.is_some() => {
                tracing::warn!(event="reconciliation_failed",account_id=%account,trade_id=%trade,"Background reconciliation did not confirm an outcome")
            }
            Ok(_) => {}
        }
    }
    Ok(())
}
