use crate::{
    auth::{Authenticator, Identity, SCOPES},
    catalog,
    config::Config,
    error::ServiceError,
    journal::Reply,
    mcp::Mcp,
    metrics,
    runtime::{Service, Source},
};
use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;
#[derive(Clone)]
struct App {
    service: Service,
    auth: Authenticator,
    config: Arc<Config>,
    limits: Arc<Mutex<Limits>>,
    inflight: Arc<tokio::sync::Semaphore>,
}
struct Limits {
    since: Instant,
    global: u32,
    subjects: BTreeMap<String, u32>,
}
impl Limits {
    fn allow(&mut self, subject: Option<&str>) -> bool {
        if self.since.elapsed() >= Duration::from_secs(60) {
            self.since = Instant::now();
            self.global = 0;
            self.subjects.clear();
        }
        if let Some(subject) = subject {
            let count = self.subjects.entry(subject.into()).or_default();
            *count = count.saturating_add(1);
            *count <= 120
        } else {
            self.global = self.global.saturating_add(1);
            self.global <= 600
        }
    }
}
pub fn router(config: Arc<Config>, service: Service) -> crate::error::Result<Router> {
    let auth = Authenticator::new(config.clone())?;
    let app = App {
        service: service.clone(),
        auth,
        config: config.clone(),
        limits: Arc::new(Mutex::new(Limits {
            since: Instant::now(),
            global: 0,
            subjects: BTreeMap::new(),
        })),
        inflight: Arc::new(tokio::sync::Semaphore::new(32)),
    };
    let mut transport = StreamableHttpServerConfig::default();
    transport.legacy_session_mode = false;
    transport.json_response = true;
    transport.max_request_body_bytes = 131072;
    let public = url::Url::parse(&config.public_url).map_err(|_| ServiceError::internal())?;
    transport.allowed_hosts = vec![public.authority().to_owned()];
    transport.allowed_origins = config.allowed_origins.iter().cloned().collect();
    transport = transport.enforce_origin_validation();
    let mcp = StreamableHttpService::new(
        move || {
            Ok(Mcp {
                service: service.clone(),
            })
        },
        Arc::new(LocalSessionManager::default()),
        transport,
    );
    Ok(Router::new()
        .route(
            "/healthz",
            get(|| async { Json(json!({"status":"alive"})) }),
        )
        .route("/readyz", get(ready))
        .route("/metrics", get(prometheus))
        .route("/.well-known/oauth-protected-resource/mcp", get(metadata))
        .route("/.well-known/oauth-protected-resource", get(metadata))
        .route("/v1/tools", get(list_tools))
        .route("/v1/tools/{tool}", post(call))
        .nest_service("/mcp", mcp)
        .layer(tower_http::limit::RequestBodyLimitLayer::new(131072))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app))
}
async fn prometheus(State(app): State<App>, Extension(identity): Extension<Identity>) -> Response {
    if identity.check("trust:admin", None).is_err() || !identity.grant.all_accounts {
        return StatusCode::FORBIDDEN.into_response();
    }
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        app.service.prometheus(),
    )
        .into_response()
}
async fn ready(State(app): State<App>) -> Response {
    if app.service.ready() {
        (StatusCode::OK, Json(json!({"status":"ready"}))).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status":"unavailable"})),
        )
            .into_response()
    }
}
async fn metadata(State(app): State<App>) -> Json<Value> {
    Json(
        json!({"resource":app.config.resource(),"authorization_servers":[app.config.issuer],"scopes_supported":SCOPES,"bearer_methods_supported":["header"],"resource_name":"Trust risk management service"}),
    )
}
async fn list_tools(Extension(identity): Extension<Identity>) -> Json<Value> {
    Json(
        json!({"schema_version":1,"tools":catalog::tools().into_iter().filter(|tool|crate::mcp::visible(&identity,tool)).collect::<Vec<_>>()}),
    )
}
async fn call(
    State(app): State<App>,
    Extension(identity): Extension<Identity>,
    Extension(request_id): Extension<Uuid>,
    Path(tool): Path<String>,
    Json(arguments): Json<Value>,
) -> Json<Reply> {
    Json(
        app.service
            .call_from(identity, tool, arguments, request_id, Source::Http)
            .await,
    )
}
fn failure(status: StatusCode, error: ServiceError, request: Uuid) -> Response {
    (status, Json(Reply::from_result(request, None, Err(error)))).into_response()
}

async fn guard(State(app): State<App>, mut request: Request, next: Next) -> Response {
    let id = Uuid::new_v4();
    let start = Instant::now();
    let origin = request.headers().get(header::ORIGIN).cloned();
    let control = matches!(request.uri().path(), "/healthz" | "/readyz" | "/metrics");
    let mut response = check_request(&app, &mut request, id).await;
    let _permit = if response.is_none() && !control {
        match app.inflight.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                response = Some(failure(
                    StatusCode::TOO_MANY_REQUESTS,
                    ServiceError::new(
                        "busy",
                        "Request concurrency limit reached.",
                        "Retry after a short delay with the same idempotency key.",
                    ),
                    id,
                ));
                None
            }
        }
    } else {
        None
    };
    let mut response = if let Some(response) = response {
        response
    } else {
        request.extensions_mut().insert(id);
        match tokio::time::timeout(Duration::from_secs(50), next.run(request)).await {
            Ok(response) => response,
            Err(_) => failure(
                StatusCode::GATEWAY_TIMEOUT,
                ServiceError::new(
                    "outcome_unknown",
                    "The request timed out.",
                    "Inspect operation_get with the original key before retrying any mutation.",
                ),
                id,
            ),
        }
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(&id.to_string()) {
        response.headers_mut().insert("x-request-id", value);
    }
    if let Some(origin) = origin.filter(|o| {
        o.to_str()
            .is_ok_and(|s| app.config.allowed_origins.contains(s))
    }) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
        response.headers_mut().insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("WWW-Authenticate, X-Request-Id, MCP-Protocol-Version"),
        );
    }
    metrics::increment(&app.service.metrics.http_requests, 1);
    if response.status().is_client_error() || response.status().is_server_error() {
        metrics::increment(&app.service.metrics.http_rejected, 1);
    }
    app.service.metrics.http_duration.observe(start.elapsed());
    tracing::info!(event="http_request",request_id=%id,status=response.status().as_u16(),elapsed_ms=start.elapsed().as_millis(),"Trust HTTP request completed");
    response
}
async fn check_request(app: &App, request: &mut Request, id: Uuid) -> Option<Response> {
    let deny = |status, error| Some(failure(status, error, id));
    let health = matches!(request.uri().path(), "/healthz" | "/readyz");
    if !health && !app.limits.lock().is_ok_and(|mut limits| limits.allow(None)) {
        return deny(
            StatusCode::TOO_MANY_REQUESTS,
            ServiceError::new(
                "rate_limited",
                "Request rate exceeded.",
                "Wait one minute before retrying.",
            ),
        );
    }
    let path = request.uri().path();
    if request.uri().query().is_some() {
        return deny(
            StatusCode::BAD_REQUEST,
            ServiceError::invalid("Query strings are not accepted; use JSON tool arguments."),
        );
    }
    if request.headers().get_all(header::ORIGIN).iter().count() > 1
        || request.headers().get(header::ORIGIN).is_some_and(|o| {
            !o.to_str()
                .is_ok_and(|o| app.config.allowed_origins.contains(o))
        })
    {
        return deny(StatusCode::FORBIDDEN, ServiceError::forbidden());
    }
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok());
    let expected = url::Url::parse(&app.config.public_url).ok();
    if !health
        && (request.headers().get_all(header::HOST).iter().count() != 1
            || expected
                .as_ref()
                .is_none_or(|url| host != Some(url.authority())))
    {
        return deny(StatusCode::FORBIDDEN, ServiceError::forbidden());
    }
    if request.method() == axum::http::Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, MCP-Protocol-Version, MCP-Method",
            ),
        );
        return Some(response);
    }
    if health || path.starts_with("/.well-known/oauth-protected-resource") {
        return None;
    }
    let token = if request
        .headers()
        .get_all(header::AUTHORIZATION)
        .iter()
        .count()
        == 1
    {
        request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
    } else {
        None
    };
    let identity = token
        .ok_or_else(crate::auth::unauthorized)
        .and_then(|t| app.auth.authenticate(t));
    match identity {
        Ok(identity) => {
            if !app
                .limits
                .lock()
                .is_ok_and(|mut limits| limits.allow(Some(&identity.subject)))
            {
                return deny(
                    StatusCode::TOO_MANY_REQUESTS,
                    ServiceError::new(
                        "rate_limited",
                        "Agent request rate exceeded.",
                        "Wait one minute before retrying.",
                    ),
                );
            }
            request.extensions_mut().insert(identity);
            None
        }
        Err(error) => {
            let mut response = failure(StatusCode::UNAUTHORIZED, error, id);
            if let Ok(header)=HeaderValue::from_str(&format!("Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", error=\"invalid_token\"",app.config.public_url)){response.headers_mut().insert(header::WWW_AUTHENTICATE,header);}
            Some(response)
        }
    }
}
