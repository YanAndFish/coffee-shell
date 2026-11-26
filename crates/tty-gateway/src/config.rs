//! `tty-gateway`'s settings, read from the environment.

use std::{
  net::SocketAddr,
  path::{Path, PathBuf},
  time::Duration,
};

/// Everything `tty-gateway` can be told, with its environment variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
  /// `COFFEE_SSH_ADDR`: where to listen (`127.0.0.1:2222`).
  pub addr: SocketAddr,
  /// `COFFEE_SSH_HOST_KEY`: the ed25519 host key, made (0600) on first run
  /// (`.data/ssh_host_ed25519_key`, relative to the working directory).
  pub host_key: PathBuf,
  /// `COFFEE_TUI_BIN`: the program each session runs (`coffee-tui` next to this
  /// executable).
  pub tui_bin: PathBuf,
  /// `COFFEE_API_URL`: the backend the sessions talk to (`http://127.0.0.1:3000`).
  pub api_url: String,
  /// `COFFEE_INTERNAL_KEY`: passed to every session as is; never logged.
  pub internal_key: Option<String>,
  /// `COFFEE_COLORS`: forces every session's colour depth (`truecolor`, `256`, `16`,
  /// `mono`); unset, each session detects its own from `TERM`/`COLORTERM`.
  pub colors: Option<String>,
  /// `COFFEE_SSH_MAX_SESSIONS`: concurrent sessions in all (64).
  pub max_sessions: usize,
  /// `COFFEE_SSH_MAX_SESSIONS_PER_IP`: concurrent sessions from one client address (8).
  /// Twice as many connections may be open from one address (the extra ones only get
  /// to hear why they are refused).
  pub max_sessions_per_ip: usize,
  /// `COFFEE_SSH_CONNECTIONS_PER_MINUTE`: new connections one client address may open
  /// per minute, in bursts of as many (30; 0 no limit). Beyond it a connection is
  /// dropped before the handshake.
  pub connections_per_minute: Option<u32>,
  /// `COFFEE_SSH_IDLE_TIMEOUT`: seconds without input before a session is closed,
  /// counted from the session program's first output (900; 0 never).
  pub idle_timeout: Option<Duration>,
  /// `COFFEE_SSH_AUTH_TIMEOUT`: seconds a connection may take from accept to a running
  /// session: handshake, login and the shell request (30).
  pub auth_timeout: Duration,
}

/// A setting that does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for ConfigError {}

impl Config {
  /// The settings from this process's environment.
  pub fn from_env() -> Result<Self, ConfigError> {
    let exe = std::env::current_exe().map_err(|e| ConfigError(format!("current_exe: {e}")))?;
    Self::from_vars(|name| std::env::var(name).ok(), exe.parent())
  }

  /// The settings `var` gives; `exe_dir` is where the default `coffee-tui` lives.
  pub fn from_vars(
    var: impl Fn(&str) -> Option<String>,
    exe_dir: Option<&Path>,
  ) -> Result<Self, ConfigError> {
    let var = |name: &str| var(name).filter(|value| !value.trim().is_empty());
    let number = |name: &str, default: u64| -> Result<u64, ConfigError> {
      var(name).map_or(Ok(default), |value| {
        value
          .trim()
          .parse()
          .map_err(|_| ConfigError(format!("{name}: not a whole number: {value:?}")))
      })
    };
    let addr = var("COFFEE_SSH_ADDR").unwrap_or_else(|| "127.0.0.1:2222".into());
    let addr = addr
      .parse()
      .map_err(|_| ConfigError(format!("COFFEE_SSH_ADDR: not host:port: {addr:?}")))?;
    let tui_bin = var("COFFEE_TUI_BIN").map_or_else(
      || exe_dir.unwrap_or(Path::new(".")).join("coffee-tui"),
      PathBuf::from,
    );
    let max_sessions = usize::try_from(number("COFFEE_SSH_MAX_SESSIONS", 64)?)
      .map_err(|_| ConfigError("COFFEE_SSH_MAX_SESSIONS: too large".into()))?;
    let max_sessions_per_ip = usize::try_from(number("COFFEE_SSH_MAX_SESSIONS_PER_IP", 8)?)
      .map_err(|_| ConfigError("COFFEE_SSH_MAX_SESSIONS_PER_IP: too large".into()))?;
    if max_sessions == 0 || max_sessions_per_ip == 0 {
      return Err(ConfigError("session limits must be at least 1".into()));
    }
    let idle = number("COFFEE_SSH_IDLE_TIMEOUT", 900)?;
    let per_minute = u32::try_from(number("COFFEE_SSH_CONNECTIONS_PER_MINUTE", 30)?)
      .map_err(|_| ConfigError("COFFEE_SSH_CONNECTIONS_PER_MINUTE: too large".into()))?;
    Ok(Self {
      addr,
      host_key: var("COFFEE_SSH_HOST_KEY").map_or_else(
        || PathBuf::from(".data/ssh_host_ed25519_key"),
        PathBuf::from,
      ),
      tui_bin,
      api_url: var("COFFEE_API_URL").unwrap_or_else(|| "http://127.0.0.1:3000".into()),
      internal_key: var("COFFEE_INTERNAL_KEY"),
      colors: var("COFFEE_COLORS"),
      max_sessions,
      max_sessions_per_ip,
      connections_per_minute: (per_minute > 0).then_some(per_minute),
      idle_timeout: (idle > 0).then(|| Duration::from_secs(idle)),
      auth_timeout: Duration::from_secs(number("COFFEE_SSH_AUTH_TIMEOUT", 30)?.max(1)),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| {
      pairs
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| (*value).to_owned())
    }
  }

  #[test]
  fn defaults() {
    let config = Config::from_vars(vars(&[]), Some(Path::new("/opt/coffee/bin"))).unwrap();
    assert_eq!(config.addr, "127.0.0.1:2222".parse().unwrap());
    assert_eq!(config.host_key, PathBuf::from(".data/ssh_host_ed25519_key"));
    assert_eq!(config.tui_bin, PathBuf::from("/opt/coffee/bin/coffee-tui"));
    assert_eq!(config.api_url, "http://127.0.0.1:3000");
    assert_eq!(config.internal_key, None);
    assert_eq!(config.colors, None);
    assert_eq!(config.max_sessions, 64);
    assert_eq!(config.max_sessions_per_ip, 8);
    assert_eq!(config.connections_per_minute, Some(30));
    assert_eq!(config.idle_timeout, Some(Duration::from_secs(900)));
    assert_eq!(config.auth_timeout, Duration::from_secs(30));
  }

  #[test]
  fn overrides() {
    let config = Config::from_vars(
      vars(&[
        ("COFFEE_SSH_ADDR", "0.0.0.0:22"),
        ("COFFEE_SSH_HOST_KEY", "/keys/host"),
        ("COFFEE_TUI_BIN", "/bin/fake"),
        ("COFFEE_API_URL", "http://api:3000"),
        ("COFFEE_INTERNAL_KEY", "s3cret"),
        ("COFFEE_SSH_MAX_SESSIONS", "3"),
        ("COFFEE_SSH_MAX_SESSIONS_PER_IP", "1"),
        ("COFFEE_SSH_IDLE_TIMEOUT", "0"),
        ("COFFEE_SSH_CONNECTIONS_PER_MINUTE", "0"),
        ("COFFEE_COLORS", "256"),
      ]),
      None,
    )
    .unwrap();
    assert_eq!(config.addr, "0.0.0.0:22".parse().unwrap());
    assert_eq!(config.host_key, PathBuf::from("/keys/host"));
    assert_eq!(config.tui_bin, PathBuf::from("/bin/fake"));
    assert_eq!(config.internal_key.as_deref(), Some("s3cret"));
    assert_eq!((config.max_sessions, config.max_sessions_per_ip), (3, 1));
    assert_eq!(config.idle_timeout, None);
    assert_eq!(config.connections_per_minute, None);
    assert_eq!(config.colors.as_deref(), Some("256"));
  }

  #[test]
  fn bad_values_are_errors() {
    assert!(Config::from_vars(vars(&[("COFFEE_SSH_ADDR", "nowhere")]), None).is_err());
    assert!(Config::from_vars(vars(&[("COFFEE_SSH_MAX_SESSIONS", "lots")]), None).is_err());
    assert!(Config::from_vars(vars(&[("COFFEE_SSH_MAX_SESSIONS", "0")]), None).is_err());
  }
}
