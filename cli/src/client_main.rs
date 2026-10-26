//! Thin terminal client. Financial policy, storage, and broker credentials belong to the service.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::{
    io::{self, Read},
    path::PathBuf,
};
#[derive(Parser)]
#[command(
    name = "trust",
    about = "Client for the authenticated Trust risk service",
    version = env!("CARGO_PKG_VERSION")
)]
struct Cli {
    /// Service origin. Defaults to TRUST_SERVICE_URL; HTTPS required except loopback.
    #[arg(long)]
    url: Option<String>,
    /// Access token file. Defaults to TRUST_ACCESS_TOKEN_FILE; tokens never appear in arguments.
    #[arg(long)]
    token_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Discover every tool, its input schema, and required scope.
    Tools,
    /// Read service capabilities and the recommended workflow.
    Status,
    /// Read a bundled workflow skill.
    Skill { name: String },
    /// Call any service tool. Read a JSON object from --input or stdin.
    Call {
        tool: String,
        #[arg(long)]
        input: Option<PathBuf>,
    },
}
fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(value) => {
            let failed = value.get("error").is_some_and(|e| !e.is_null());
            println!("{value}");
            if failed {
                std::process::ExitCode::FAILURE
            } else {
                std::process::ExitCode::SUCCESS
            }
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn run(cli: Cli) -> Result<Value, &'static str> {
    let origin = cli
        .url
        .or_else(|| std::env::var("TRUST_SERVICE_URL").ok())
        .ok_or("Set TRUST_SERVICE_URL or --url to the service origin.")?;
    let base = reqwest::Url::parse(&origin).map_err(|_| "Invalid service URL.")?;
    let local = base.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if (base.scheme() != "https" && !(base.scheme() == "http" && local))
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
        || base.path() != "/"
    {
        return Err("Use an HTTPS service origin without credentials, paths, or query strings. Loopback HTTP is allowed for development.");
    }
    let file=cli.token_file.or_else(||std::env::var_os("TRUST_ACCESS_TOKEN_FILE").map(PathBuf::from)).ok_or("Set TRUST_ACCESS_TOKEN_FILE or --token-file to an operator-provided access token file.")?;
    let token = read_limited(
        std::fs::File::open(file).map_err(|_| "Cannot read access token file.")?,
        16384,
    )?;
    let token = std::str::from_utf8(&token)
        .map_err(|_| "Invalid token file.")?
        .trim();
    if token.is_empty() || token.contains(char::is_whitespace) {
        return Err("Invalid token file.");
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(55))
        .build()
        .map_err(|_| "Cannot initialize HTTPS client.")?;
    let (path, body) = match cli.command {
        Command::Tools => ("/v1/tools".into(), None),
        Command::Status => ("/v1/tools/service_status".into(), Some(json!({}))),
        Command::Skill { name } => ("/v1/tools/skill_read".into(), Some(json!({"name":name}))),
        Command::Call { tool, input } => {
            if tool.is_empty() || !tool.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                return Err("Use an exact tool name from trust tools.");
            }
            let bytes = match input {
                Some(path) => read_limited(
                    std::fs::File::open(path).map_err(|_| "Cannot read tool input.")?,
                    131072,
                )?,
                None => read_limited(io::stdin().lock(), 131072)?,
            };
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|_| "Tool input must be valid JSON.")?;
            if !value.is_object() {
                return Err("Tool input must be a JSON object.");
            }
            (format!("/v1/tools/{tool}"), Some(value))
        }
    };
    let target = base.join(&path).map_err(|_| "Invalid service endpoint.")?;
    let request = if let Some(body) = body {
        client.post(target).json(&body)
    } else {
        client.get(target)
    };
    let response=request.bearer_auth(token).send().map_err(|_|"No service response. For a mutation, inspect operation_get with the original idempotency key; do not repeat it with a new key.")?;
    let success = response.status().is_success();
    let bytes = read_limited(response, 1_048_576)?;
    let value:Value=serde_json::from_slice(&bytes).map_err(|_|"The service returned an invalid response. Check its health and authentication configuration.")?;
    if !success && value.get("error").is_none() {
        return Err("The service rejected the request. Check authentication, permissions, and service availability.");
    }
    Ok(value)
}
fn read_limited(reader: impl Read, max: u64) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    reader
        .take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read input or response.")?;
    if u64::try_from(bytes.len()).map_err(|_| "Input too large.")? > max {
        return Err("Input or response exceeded the size limit.");
    }
    Ok(bytes)
}
