//! `tty-gateway`: passwordless SSH on `COFFEE_SSH_ADDR` (127.0.0.1:2222), one
//! `coffee-tui` per session. Settings: see `tty_gateway::Config`.

use std::process::ExitCode;

use tokio::signal::unix::{SignalKind, signal};
use tty_gateway::{Config, hostkey, init_logging, serve};

#[tokio::main]
async fn main() -> ExitCode {
  init_logging();
  let config = match Config::from_env() {
    Ok(config) => config,
    Err(error) => {
      tracing::error!(%error, "bad configuration");
      return ExitCode::FAILURE;
    }
  };
  if !config.tui_bin.is_file() {
    tracing::warn!(
      bin = %config.tui_bin.display(),
      "COFFEE_TUI_BIN does not exist yet: run `cargo build -p coffee-tui` (sessions fail until it does)"
    );
  }
  let (key, generated) = match hostkey::load_or_generate(&config.host_key) {
    Ok(key) => key,
    Err(error) => {
      tracing::error!(path = %config.host_key.display(), %error, "host key");
      return ExitCode::FAILURE;
    }
  };
  let fingerprint = key
    .public_key()
    .fingerprint(russh::keys::HashAlg::Sha256)
    .to_string();
  let listener = match tokio::net::TcpListener::bind(config.addr).await {
    Ok(listener) => listener,
    Err(error) => {
      tracing::error!(addr = %config.addr, %error, "cannot listen");
      return ExitCode::FAILURE;
    }
  };
  tracing::info!(
    addr = %listener.local_addr().unwrap_or(config.addr),
    host_key = %config.host_key.display(),
    generated,
    %fingerprint,
    tui = %config.tui_bin.display(),
    api = %config.api_url,
    internal_key = config.internal_key.is_some(),
    max_sessions = config.max_sessions,
    max_sessions_per_ip = config.max_sessions_per_ip,
    idle_timeout = config.idle_timeout.map_or(0, |idle| idle.as_secs()),
    "listening"
  );
  let shutdown = async {
    let mut term = signal(SignalKind::terminate()).ok();
    tokio::select! {
      _ = tokio::signal::ctrl_c() => {}
      () = async {
        match term.as_mut() {
          Some(term) => { term.recv().await; }
          None => std::future::pending::<()>().await,
        }
      } => {}
    }
    tracing::info!("shutting down");
  };
  match serve(listener, config, key, shutdown).await {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      tracing::error!(%error, "server stopped");
      ExitCode::FAILURE
    }
  }
}
