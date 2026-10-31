//! A single worker owns synchronous, non-Send domain dependencies and durable intent.
use crate::{
    auth::{Grant, Identity},
    catalog::{self, Operation},
    config::Config,
    domain::{self, Domain},
    error::{Result, ServiceError},
    journal::{AuditContext, Claim, Journal, Reply},
    metrics::{self, Metrics, Stage},
};
use serde_json::{json, Value};
use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum Source {
    Internal,
    Http,
    Mcp,
    Background,
}
impl Source {
    fn name(self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Http => "http",
            Self::Mcp => "mcp",
            Self::Background => "background",
        }
    }
}
const QUEUED: u8 = 0;
const STARTED: u8 = 1;
const CANCELLED: u8 = 2;

struct Request {
    identity: Identity,
    tool: String,
    arguments: Value,
    request_id: Uuid,
    reply: oneshot::Sender<Reply>,
    enqueued: Instant,
    state: Arc<AtomicU8>,
    source: Source,
}
#[derive(Clone)]
pub struct Service {
    sender: mpsc::SyncSender<Request>,
    pub metrics: Arc<Metrics>,
    queue_capacity: usize,
    request_timeout: Duration,
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
        config.verify_secret_files()?;
        let metrics = Arc::new(Metrics::default());
        let worker_metrics = metrics.clone();
        let queue_capacity = config.queue_capacity;
        let request_timeout = Duration::from_millis(config.request_timeout_ms);
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
                    let journal = if let Some(encryption) = &config.storage_encryption {
                        let key = trust_security::DatabaseKey::load(&encryption.journal_key_file).map_err(|_| crate::config::storage_error())?;
                        Journal::open_encrypted(&config.journal, &key)?
                    } else {
                        Journal::open(&config.journal)?
                    };
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
                let mut safety_cursor = None;
                while !worker_stopping.load(Ordering::Acquire) {
                    match receiver.recv_timeout(Duration::from_millis(250)) {
                        Ok(request) => {
                            metrics::decrement(&worker_metrics.queued);
                            let wait = request.enqueued.elapsed();
                            worker_metrics.queue_wait.observe(wait);
                            if request.reply.is_closed()
                                || wait >= Duration::from_millis(config.queue_timeout_ms)
                                || request.state.compare_exchange(QUEUED, STARTED, Ordering::AcqRel, Ordering::Acquire).is_err() {
                                request.state.store(CANCELLED, Ordering::Release);
                                metrics::increment(&worker_metrics.expired, 1);
                                tracing::info!(event="queued_request_cancelled", request_id=%request.request_id, queue_wait_ms=metrics::milliseconds(wait), "Queued request was not dispatched");
                                let _ = request.reply.send(Reply::from_result(request.request_id, None, Err(expired())));
                                continue;
                            }
                            metrics::increment(&worker_metrics.active, 1);
                            let result = execute(
                                &mut domain,
                                &mut journal,
                                &request.identity,
                                &request.tool,
                                &request.arguments,
                                request.request_id,
                                Observation { source: request.source, wait, metrics: &worker_metrics },
                            );
                            metrics::decrement(&worker_metrics.active);
                            let fatal = result
                                .as_ref()
                                .err()
                                .is_some_and(|e| matches!(e.code.as_str(), "audit_unavailable" | "audit_integrity_failed"));
                            let reply = result.unwrap_or_else(|e| {
                                Reply::from_result(request.request_id, None, Err(e))
                            });
                            if fatal { worker_ready.store(false, Ordering::Release); }
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
                        if reconcile(&mut domain, &mut journal, &mut reconcile_cursor, &mut safety_cursor, &worker_metrics).is_err() {
                            break;
                        }
                        next_reconcile = Instant::now();
                    }
                }
                worker_ready.store(false, Ordering::Release);
                for request in receiver.try_iter() {
                    metrics::decrement(&worker_metrics.queued);
                    request.state.store(CANCELLED, Ordering::Release);
                    let _ = request.reply.send(Reply::from_result(request.request_id, None, Err(unavailable())));
                }
                tracing::info!(event="worker_stopped", "Financial worker stopped");
                drop(locks);
            })
            .map_err(|_| ServiceError::internal())?;
        startup_rx.recv().map_err(|_| ServiceError::internal())??;
        Ok((
            Self {
                sender,
                metrics,
                queue_capacity,
                request_timeout,
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
    pub fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        self.ready.store(false, Ordering::Release);
        tracing::info!(
            event = "shutdown_started",
            "Stopping admission; finishing the active operation"
        );
    }
    pub fn prometheus(&self) -> String {
        self.metrics.render(self.ready(), self.queue_capacity)
    }
    pub async fn call(
        &self,
        identity: Identity,
        tool: String,
        arguments: Value,
        request_id: Uuid,
    ) -> Reply {
        self.call_from(identity, tool, arguments, request_id, Source::Internal)
            .await
    }
    pub async fn call_from(
        &self,
        identity: Identity,
        tool: String,
        arguments: Value,
        request_id: Uuid,
        source: Source,
    ) -> Reply {
        let error = |e| Reply::from_result(request_id, None, Err(e));
        if !self.ready() {
            return error(unavailable());
        }
        let (tx, rx) = oneshot::channel();
        let state = Arc::new(AtomicU8::new(QUEUED));
        metrics::increment(&self.metrics.queued, 1);
        if self
            .sender
            .try_send(Request {
                identity,
                tool,
                arguments,
                request_id,
                reply: tx,
                enqueued: Instant::now(),
                state: state.clone(),
                source,
            })
            .is_err()
        {
            metrics::decrement(&self.metrics.queued);
            metrics::increment(&self.metrics.busy, 1);
            tracing::warn!(event="queue_rejected", request_id=%request_id, "Financial worker queue is full or unavailable");
            return error(ServiceError::before_dispatch_retry(
                "busy",
                "The service queue is full or unavailable.",
                "Retry with the same idempotency key after a short delay.",
            ));
        }
        match tokio::time::timeout(self.request_timeout, rx).await {
            Ok(Ok(reply)) => reply,
            _ if state
                .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok() =>
            {
                error(expired())
            }
            _ if state.load(Ordering::Acquire) == CANCELLED => error(unavailable()),
            _ => {
                metrics::increment(&self.metrics.unknown, 1);
                tracing::warn!(event="completion_unknown", request_id=%request_id, "Caller stopped waiting for an active operation");
                error(ServiceError::new("outcome_unknown", "No completion response was received; the worker may still be executing.", "Use operation_get with the original key. Never repeat this intent with a new key."))
            }
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
fn expired() -> ServiceError {
    ServiceError::before_dispatch_retry(
        "request_expired",
        "The request expired before dispatch; no operation was started.",
        "Retry with the same idempotency key after the queue clears.",
    )
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
        let file = trust_security::private_file(path, false)
            .map_err(|_| crate::config::storage_error())?;
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

struct Observation<'a> {
    source: Source,
    wait: Duration,
    metrics: &'a Metrics,
}
fn execute(
    domain: &mut Domain,
    journal: &mut Journal,
    identity: &Identity,
    name: &str,
    args: &Value,
    request_id: Uuid,
    observation: Observation<'_>,
) -> Result<Reply> {
    let start = Instant::now();
    journal.context = AuditContext {
        source: observation.source.name(),
        queue_wait_ms: metrics::milliseconds(observation.wait),
        ..AuditContext::default()
    };
    let spec = catalog::find(name);
    let safe_name = spec.as_ref().map_or("unknown", |spec| spec.name);
    let span = tracing::info_span!("operation", request_id=%request_id, tool=safe_name);
    let _entered = span.enter();
    let result = execute_inner(
        domain,
        journal,
        identity,
        name,
        args,
        request_id,
        start,
        observation.metrics,
    );
    let error = match &result {
        Ok(reply) => reply.error.as_ref(),
        Err(error) => Some(error),
    };
    metrics::increment(&observation.metrics.completed, 1);
    if error.is_some() {
        metrics::increment(&observation.metrics.failed, 1);
    }
    if error.is_some_and(|e| {
        matches!(
            e.code.as_str(),
            "audit_unavailable" | "audit_integrity_failed"
        )
    }) {
        metrics::increment(&observation.metrics.audit_failures, 1);
        tracing::error!(event="audit_failed", code=error.map_or("", |e| e.code.as_str()), request_id=%request_id, "Durable audit failed; financial worker will stop");
    }
    if journal.context.replayed {
        metrics::increment(&observation.metrics.replayed, 1);
    }
    let duration = start.elapsed();
    observation.metrics.execution.observe(duration);
    observation
        .metrics
        .operation(safe_name, duration, error.is_some());
    tracing::info!(event="operation", request_id=%request_id, operation_id=?result.as_ref().ok().and_then(|reply| reply.operation_id), tool=safe_name, source=observation.source.name(), queue_wait_ms=metrics::milliseconds(observation.wait), queue_wait_us=metrics::microseconds(observation.wait), duration_ms=metrics::milliseconds(duration), duration_us=metrics::microseconds(duration), replayed=journal.context.replayed, error_code=error.map_or("", |e|e.code.as_str()), outcome=if error.is_some(){"error"}else{"ok"}, "Trust operation finished");
    result
}

#[allow(clippy::too_many_arguments)]
fn execute_inner(
    domain: &mut Domain,
    journal: &mut Journal,
    identity: &Identity,
    name: &str,
    args: &Value,
    request_id: Uuid,
    start: Instant,
    metrics: &Metrics,
) -> Result<Reply> {
    let spec = match catalog::find(name) {
        Ok(spec) => spec,
        Err(error) => {
            journal.context.error_code = Some(error.code.clone());
            metrics
                .measure(Stage::JournalFinish, || {
                    journal.record(identity, request_id, "unknown", None, "invalid")
                })
                .map_err(|_| audit_unavailable())?;
            return Err(error);
        }
    };
    if let Err(error) = metrics.measure(Stage::Validation, || catalog::validate(spec, args)) {
        journal.context.error_code = Some(error.code.clone());
        metrics
            .measure(Stage::JournalFinish, || {
                journal.record(identity, request_id, name, None, "invalid")
            })
            .map_err(|_| audit_unavailable())?;
        return Err(error);
    }
    let account = args
        .get("account_id")
        .and_then(Value::as_str)
        .and_then(|v| v.parse().ok());
    let accounts = match metrics.measure(Stage::Authorization, || {
        domain.authorize(identity, spec, args)
    }) {
        Ok(ids) => ids,
        Err(error) => {
            journal.context.error_code = Some(error.code.clone());
            metrics
                .measure(Stage::JournalFinish, || {
                    journal.record(identity, request_id, name, account, "denied")
                })
                .map_err(|_| audit_unavailable())?;
            return Err(error);
        }
    };
    journal.context.account_ids = accounts.clone();
    let operation_id = if spec.mutation {
        let key = domain::string(args, "idempotency_key")?;
        match metrics.measure(Stage::JournalClaim, || {
            journal.claim(identity, key, name, account, args, request_id, &accounts)
        }) {
            Ok(Claim::New(id)) => Some(id),
            Ok(Claim::Existing(reply)) => return Ok(reply),
            Err(error) if error.code == "service_unavailable" => return Err(audit_unavailable()),
            Err(error) => {
                journal.context.error_code = Some(error.code.clone());
                metrics
                    .measure(Stage::JournalFinish, || {
                        journal.record(identity, request_id, name, account, "denied")
                    })
                    .map_err(|_| audit_unavailable())?;
                return Err(error);
            }
        }
    } else {
        None
    };
    let result = metrics.measure(Stage::Domain, || {
        crate::safety::before(domain, journal, identity, spec.operation, args, request_id)?;
        identity.check(spec.scope, account)?;
        if crate::safety::handles(spec.operation) {
            return crate::safety::dispatch(
                domain,
                journal,
                identity,
                spec.operation,
                args,
                request_id,
            );
        }
        match spec.operation {
            Operation::OperationGet => journal.operation(identity, domain::string(args, "key")?),
            Operation::AuditList => journal.list(
                identity,
                args.get("after").and_then(Value::as_u64).unwrap_or(0),
                args.get("limit").and_then(Value::as_u64).unwrap_or(100),
            ),
            Operation::AuditVerify => journal.verify(),
            _ => domain.dispatch(identity, spec.operation, args),
        }
    });
    let result = match result {
        Err(error) if error.code == "audit_integrity_failed" => return Err(error),
        other => other,
    };
    // A domain error can follow an external side effect. Do not claim rollback.
    let result = result.map_err(|error| {
        if spec.mutation && error.code == "domain_rejected" {
            ServiceError::new("outcome_unknown", error.message, &error.next_action)
        } else {
            error
        }
    });
    crate::safety::after(
        domain,
        journal,
        identity,
        spec,
        args,
        request_id,
        operation_id,
        &result,
    )?;
    let result = result.map(|value| {
        if spec.mutation {
            value
        } else {
            domain::paginate(value, args)
        }
    });
    let mut reply = Reply::from_result(request_id, operation_id, result);
    reply.actions =
        crate::guidance::actions(domain, journal, identity, spec.operation, args, &reply);
    reply.next_actions = reply
        .actions
        .iter()
        .filter(|action| action.available)
        .map(|action| action.tool.clone())
        .collect();
    // Bound persisted and returned bodies. Never turn a successful mutation into a replay.
    if serde_json::to_vec(&reply)
        .map_err(|_| ServiceError::internal())?
        .len()
        > 1_048_576
    {
        reply.data = None;
        reply.error=Some(ServiceError::new("result_too_large","The result exceeded the response limit.","Use a narrower query or pagination. A mutation may have completed; inspect its resulting records."));
    }
    journal.context.duration_ms = metrics::milliseconds(start.elapsed());
    journal.context.error_code = reply.error.as_ref().map(|error| error.code.clone());
    if spec.mutation {
        metrics
            .measure(Stage::JournalFinish, || {
                journal.finish(identity, name, account, &reply)
            })
            .map_err(|_| audit_unavailable())?;
    } else {
        metrics
            .measure(Stage::JournalFinish, || {
                journal.record(
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
            })
            .map_err(|_| audit_unavailable())?;
    }
    Ok(reply)
}

fn reconcile(
    domain: &mut Domain,
    journal: &mut Journal,
    cursor: &mut Option<(Uuid, Uuid)>,
    safety_cursor: &mut Option<Uuid>,
    metrics: &Metrics,
) -> Result<()> {
    if let Some(account) = crate::safety::background_account(domain, journal, *safety_cursor)? {
        *safety_cursor = Some(account);
        let scopes = std::collections::BTreeSet::from(["trust:reconcile".to_owned()]);
        let identity = Identity {
            subject: "system:reconciler".into(),
            client_id: None,
            expires_at: u64::MAX,
            owner_authenticated_until: None,
            scopes: scopes.clone(),
            grant: Grant {
                role: crate::auth::Role::Agent,
                accounts: Default::default(),
                all_accounts: true,
                scopes,
                allow_live: domain.config.allow_live,
            },
        };
        let result = execute(
            domain,
            journal,
            &identity,
            "account_refresh",
            &json!({"account_id":account,"idempotency_key":Uuid::new_v4().to_string()}),
            Uuid::new_v4(),
            Observation {
                source: Source::Background,
                wait: Duration::ZERO,
                metrics,
            },
        );
        if let Err(error) = result {
            if matches!(
                error.code.as_str(),
                "audit_unavailable" | "audit_integrity_failed"
            ) {
                return Err(error);
            }
        }
    }
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
        owner_authenticated_until: None,
        scopes: scopes.clone(),
        grant: Grant {
            role: crate::auth::Role::Agent,
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
        metrics::increment(&metrics.active, 1);
        let result = execute(
            domain,
            journal,
            &identity,
            "trade_reconcile",
            &args,
            Uuid::new_v4(),
            Observation {
                source: Source::Background,
                wait: Duration::ZERO,
                metrics,
            },
        );
        metrics::decrement(&metrics.active);
        match result {
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
