use std::{path::PathBuf, sync::Arc};
use trust_service::{
    config::Config,
    error::{Result, ServiceError},
    runtime::Service,
};
#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Provider/core messages may contain third-party response text. Only structured service
    // events are enabled in production; request bodies, tokens, and URLs are never logged.
    tracing_subscriber::fmt()
        .json()
        .with_env_filter("off,trust_service=info,trust::operational=info")
        .with_current_span(false)
        .init();
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(code=%error.code,message=%error.message,"Trust service stopped");
            std::process::ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/trust/config.json"));
    let config = Arc::new(Config::load(&path)?);
    let database = config.database.clone();
    let (service, worker) = Service::start(config.clone(), move || {
        let path = database.to_str().ok_or_else(ServiceError::internal)?;
        Ok(trust_core::TrustFacade::new_with_brokers(
            Box::new(db_sqlite::SqliteDatabase::new(path)),
            vec![
                Box::new(ibkr_broker::IbkrBroker),
                Box::new(alpaca_broker::AlpacaBroker),
            ],
        ))
    })?;
    let app = trust_service::http::router(config.clone(), service.clone())?;
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|_| ServiceError::invalid("Cannot bind the configured service listener."))?;
    let address = listener
        .local_addr()
        .map_err(|_| ServiceError::internal())?;
    tracing::info!(event = "service_ready", listen=%address, "Trust service is ready");
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            service.begin_shutdown();
        })
        .await;
    worker.shutdown().await?;
    server.map_err(|_| ServiceError::internal())
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}};
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
