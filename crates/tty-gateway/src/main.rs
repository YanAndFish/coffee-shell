//! `tty-gateway --config <file> [--check]`: passwordless SSH, one program per session,
//! chosen by the SSH user name. See `tty_gateway::config` for the file.

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use tokio::signal::unix::{SignalKind, signal};
use tty_gateway::{hostkey, init_logging, load, serve};

const USAGE: &str = "\
usage: tty-gateway --config <file> [--check]

  --config <file>  the TOML configuration (else $TTY_GATEWAY_CONFIG)
  --check          read and check the configuration, print a summary, exit
  --help           this text
  --version        the version

environment:
  TTY_GATEWAY_CONFIG    the configuration file, without --config
  TTY_GATEWAY_ADDR      overrides server.listen
  TTY_GATEWAY_HOST_KEY  overrides server.host_key
  TTY_GATEWAY_LOG       log filter (else RUST_LOG; default info,russh=warn)
";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Args {
  Run { config: PathBuf, check: bool },
  Help,
  Version,
}

fn parse_args(
  args: impl IntoIterator<Item = String>,
  env_config: Option<String>,
) -> Result<Args, String> {
  let mut config = None;
  let mut check = false;
  let mut args = args.into_iter();
  while let Some(arg) = args.next() {
    match arg.as_str() {
      "--help" | "-h" => return Ok(Args::Help),
      "--version" | "-V" => return Ok(Args::Version),
      "--check" => check = true,
      "--config" | "-c" => {
        config = Some(args.next().ok_or_else(|| format!("{arg} needs a file"))?);
      }
      _ => match arg.strip_prefix("--config=") {
        Some(value) => config = Some(value.to_owned()),
        None => return Err(format!("unknown argument {arg:?}")),
      },
    }
  }
  let config = config
    .or(env_config.filter(|value| !value.is_empty()))
    .ok_or("no configuration: pass --config <file> or set TTY_GATEWAY_CONFIG")?;
  Ok(Args::Run {
    config: PathBuf::from(config),
    check,
  })
}

fn main() -> ExitCode {
  let args = match parse_args(
    std::env::args().skip(1),
    std::env::var("TTY_GATEWAY_CONFIG").ok(),
  ) {
    Ok(args) => args,
    Err(error) => {
      eprintln!("tty-gateway: {error}\n\n{USAGE}");
      return ExitCode::from(2);
    }
  };
  let (config, check) = match args {
    Args::Help => {
      print!("{USAGE}");
      return ExitCode::SUCCESS;
    }
    Args::Version => {
      println!("tty-gateway {}", env!("CARGO_PKG_VERSION"));
      return ExitCode::SUCCESS;
    }
    Args::Run { config, check } => (config, check),
  };
  let exe = std::env::current_exe().ok();
  let exe_dir = exe.as_deref().and_then(std::path::Path::parent);
  let loaded = load(&config, |name| std::env::var(name).ok(), exe_dir);
  if check {
    return match loaded {
      Ok((snapshot, warnings)) => {
        print!("{}", snapshot.summary());
        for warning in &warnings {
          eprintln!("warning: {warning}");
        }
        println!("{}: ok", config.display());
        ExitCode::SUCCESS
      }
      Err(error) => {
        eprintln!("error: {error}");
        ExitCode::FAILURE
      }
    };
  }
  init_logging();
  let (snapshot, warnings) = match loaded {
    Ok(loaded) => loaded,
    Err(error) => {
      tracing::error!(%error, "bad configuration");
      return ExitCode::FAILURE;
    }
  };
  for warning in &warnings {
    tracing::warn!("{warning}");
  }
  let runtime = match tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
  {
    Ok(runtime) => runtime,
    Err(error) => {
      tracing::error!(%error, "cannot start the runtime");
      return ExitCode::FAILURE;
    }
  };
  runtime.block_on(run(config, snapshot))
}

async fn run(config: PathBuf, snapshot: tty_gateway::Snapshot) -> ExitCode {
  let server = &snapshot.server;
  let (key, generated) = match hostkey::load_or_generate(&server.host_key) {
    Ok(key) => key,
    Err(error) => {
      tracing::error!(path = %server.host_key.display(), %error, "host key");
      return ExitCode::FAILURE;
    }
  };
  let fingerprint = key
    .public_key()
    .fingerprint(russh::keys::HashAlg::Sha256)
    .to_string();
  let listener = match tokio::net::TcpListener::bind(server.listen).await {
    Ok(listener) => listener,
    Err(error) => {
      tracing::error!(addr = %server.listen, %error, "cannot listen");
      return ExitCode::FAILURE;
    }
  };
  let routes: Vec<&str> = snapshot
    .routes
    .iter()
    .map(|route| route.name.as_str())
    .collect();
  tracing::info!(
    addr = %listener.local_addr().unwrap_or(server.listen),
    config = %config.display(),
    host_key = %server.host_key.display(),
    generated,
    %fingerprint,
    routes = %routes.join(","),
    default_route = %server.default_route,
    max_sessions = snapshot.limits.max_sessions,
    max_sessions_per_ip = snapshot.limits.max_sessions_per_ip,
    "listening"
  );
  // SIGHUP reloads come later; for now the first snapshot stays.
  let (_snapshots, receiver) = tokio::sync::watch::channel(Arc::new(snapshot));
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
  match serve(listener, key, receiver, shutdown).await {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      tracing::error!(%error, "server stopped");
      ExitCode::FAILURE
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_owned()).collect()
  }

  #[test]
  fn the_command_line() {
    let run = |config: &str, check| Args::Run {
      config: config.into(),
      check,
    };
    assert_eq!(
      parse_args(args(&["--config", "a.toml"]), None),
      Ok(run("a.toml", false))
    );
    assert_eq!(
      parse_args(args(&["--check", "--config=b.toml"]), None),
      Ok(run("b.toml", true))
    );
    assert_eq!(
      parse_args(args(&[]), Some("env.toml".into())),
      Ok(run("env.toml", false))
    );
    assert_eq!(
      parse_args(args(&["--config", "a.toml"]), Some("env.toml".into())),
      Ok(run("a.toml", false))
    );
    assert_eq!(parse_args(args(&["--help"]), None), Ok(Args::Help));
    assert_eq!(parse_args(args(&["--version"]), None), Ok(Args::Version));
    assert!(parse_args(args(&[]), None).is_err());
    assert!(parse_args(args(&["--config"]), None).is_err());
    assert!(parse_args(args(&["--nope", "--config", "a"]), None).is_err());
  }
}
