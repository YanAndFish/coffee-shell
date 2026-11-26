//! `tty-gateway`'s configuration: a TOML file (see [`load`]), checked as a whole when it
//! is read. What a running server uses is a [`Snapshot`]: every connection takes the one
//! in force when it is accepted and keeps it.

use std::{
  collections::{BTreeMap, HashMap},
  fmt,
  net::{IpAddr, SocketAddr},
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use serde::Deserialize;

/// A configuration that cannot be used; the message names the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for ConfigError {}

fn error(key: impl fmt::Display, what: impl fmt::Display) -> ConfigError {
  ConfigError(format!("{key}: {what}"))
}

/// One complete, checked configuration.
#[derive(Debug)]
pub struct Snapshot {
  pub server: Server,
  pub limits: Limits,
  /// The global messages (with the defaults filled in).
  pub messages: Messages,
  /// Every route, by name.
  pub routes: Vec<Arc<Route>>,
  default_route: usize,
  users: HashMap<String, usize>,
}

/// `[server]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
  pub listen: SocketAddr,
  /// Relative paths are relative to the working directory.
  pub host_key: PathBuf,
  pub server_id: String,
  /// Empty: no banner.
  pub banner: String,
  /// The name of the route for user names no route lists.
  pub default_route: String,
  pub shutdown_grace: Duration,
}

/// `[limits]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
  pub max_sessions: usize,
  pub max_sessions_per_ip: usize,
  /// 0: no rate limit.
  pub connections_per_minute: u32,
  pub auth_timeout: Duration,
  /// `None`: never.
  pub idle_timeout: Option<Duration>,
  /// `None`: never.
  pub max_session_time: Option<Duration>,
}

/// What clients are told, in full (defaults filled in).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Messages {
  pub no_pty: String,
  pub unavailable: String,
  /// `{code}`: the exit status.
  pub crashed: String,
  /// `{minutes}`: the idle timeout.
  pub idle: String,
  pub time_limit: String,
  /// `{max}` (in this and the next two): the cap that was reached.
  pub full: String,
  pub full_per_ip: String,
  pub full_route: String,
  pub shutting_down: String,
}

impl Default for Messages {
  fn default() -> Self {
    Self {
      no_pty: "This service needs a terminal: connect with `ssh -t` (not -T).".into(),
      unavailable: "The program could not be started. Please try again later.".into(),
      crashed: "The program stopped with an error (code {code}). Please reconnect; if this keeps happening, try again later.".into(),
      idle: "No input for {minutes} minutes: the connection was closed.".into(),
      time_limit: "The session reached its time limit: the connection was closed.".into(),
      full: "The server is full ({max} sessions). Please try again later.".into(),
      full_per_ip: "Your address has {max} sessions open already. Close one and try again.".into(),
      full_route: "This service is full ({max} sessions). Please try again later.".into(),
      shutting_down: "The server is shutting down. Please reconnect in a moment.".into(),
    }
  }
}

impl Messages {
  pub fn crashed(&self, code: u32) -> String {
    self.crashed.replace("{code}", &code.to_string())
  }

  pub fn idle(&self, minutes: u64) -> String {
    self.idle.replace("{minutes}", &minutes.to_string())
  }

  pub fn full(&self, max: usize) -> String {
    self.full.replace("{max}", &max.to_string())
  }

  pub fn full_per_ip(&self, max: usize) -> String {
    self.full_per_ip.replace("{max}", &max.to_string())
  }

  pub fn full_route(&self, max: usize) -> String {
    self.full_route.replace("{max}", &max.to_string())
  }

  fn overlay(&mut self, raw: &RawMessages) {
    let fields = [
      (&mut self.no_pty, &raw.no_pty),
      (&mut self.unavailable, &raw.unavailable),
      (&mut self.crashed, &raw.crashed),
      (&mut self.idle, &raw.idle),
      (&mut self.time_limit, &raw.time_limit),
      (&mut self.full, &raw.full),
      (&mut self.full_per_ip, &raw.full_per_ip),
      (&mut self.full_route, &raw.full_route),
      (&mut self.shutting_down, &raw.shutting_down),
    ];
    for (field, value) in fields {
      if let Some(value) = value {
        field.clone_from(value);
      }
    }
  }
}

/// `[routes.NAME]`: who lands here and what they get.
#[derive(Debug)]
pub struct Route {
  pub name: String,
  pub users: Vec<String>,
  pub upstream: Upstream,
  /// The client environment variables passed on.
  pub accept_env: Vec<EnvPattern>,
  /// What the route adds to every session's environment (sorted by name).
  pub env: Vec<(String, Template)>,
  pub limits: RouteLimits,
  /// This route's messages: its own, else the global ones, else the defaults.
  pub messages: Messages,
}

/// A route's limits, with `[limits]` filled in where the route sets none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteLimits {
  /// `None`: never.
  pub idle_timeout: Option<Duration>,
  /// `None`: never.
  pub max_session_time: Option<Duration>,
  /// Sessions on this route at once, besides the global caps.
  pub max_sessions: Option<usize>,
}

/// What a route connects a session to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Upstream {
  /// A local program on a PTY.
  Command(CommandSpec),
}

/// A local program and how to start it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
  /// Absolute, or a bare name looked up at each start (see [`CommandSpec::resolve`]).
  pub program: PathBuf,
  pub args: Vec<String>,
  pub cwd: PathBuf,
  /// Where the gateway's executable is: bare names are looked for here first.
  pub exe_dir: Option<PathBuf>,
}

impl CommandSpec {
  /// The file to run now: a path as given; a bare name next to the gateway's executable,
  /// else the first executable of that name on `PATH`.
  pub fn resolve(&self) -> Option<PathBuf> {
    resolve_program(
      &self.program,
      self.exe_dir.as_deref(),
      std::env::var_os("PATH").as_deref(),
    )
  }
}

fn resolve_program(
  program: &Path,
  exe_dir: Option<&Path>,
  path: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
  if program.components().count() != 1 || program.is_absolute() {
    return Some(program.to_owned());
  }
  let executable = |candidate: &Path| {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(candidate)
      .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
  };
  exe_dir
    .map(|dir| dir.join(program))
    .into_iter()
    .chain(
      path
        .into_iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(program)),
    )
    .find(|candidate| executable(candidate))
}

/// An `accept_env` entry: a name, or a prefix ending in `*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvPattern {
  Exact(String),
  Prefix(String),
}

impl EnvPattern {
  pub fn matches(&self, name: &str) -> bool {
    match self {
      EnvPattern::Exact(exact) => name == exact,
      EnvPattern::Prefix(prefix) => name.starts_with(prefix.as_str()),
    }
  }

  fn parse(text: &str) -> Option<Self> {
    let (body, prefix) = match text.strip_suffix('*') {
      Some(body) => (body, true),
      None => (text, false),
    };
    let fine = body.len() <= 32 && body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if !fine || (!prefix && body.is_empty()) {
      return None;
    }
    Some(if prefix {
      EnvPattern::Prefix(body.into())
    } else {
      EnvPattern::Exact(body.into())
    })
  }
}

/// What a session knows about itself, for [`Template::render`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionVars {
  /// The SSH user name, only if it is plain (see [`SessionVars::sanitize_user`]).
  pub user: Option<String>,
  pub route: String,
  /// `SHA256:…`, only after a verified public key.
  pub fingerprint: Option<String>,
  pub client_ip: Option<IpAddr>,
}

impl SessionVars {
  /// `user` if it is 1 to 64 of `A-Z a-z 0-9 . _ @ + -`; anything else is never passed on.
  pub fn sanitize_user(user: &str) -> Option<String> {
    (!user.is_empty()
      && user.len() <= 64
      && user
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._@+-".contains(&b)))
    .then(|| user.to_owned())
  }
}

/// A `[routes.NAME.env]` value: literal text and session variables. `${env:…}` was
/// resolved when the file was read. Its `Debug` shows no text (values can be secrets).
#[derive(Clone, PartialEq, Eq)]
pub struct Template {
  /// `None`: a `${env:…}` without a value: the entry never exists.
  parts: Option<Vec<Part>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Var {
  User,
  Route,
  Fingerprint,
  ClientIp,
}

#[derive(Clone, PartialEq, Eq)]
enum Part {
  Text(String),
  Var(Var),
}

impl fmt::Debug for Template {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let vars: Vec<Var> = self
      .parts
      .iter()
      .flatten()
      .filter_map(|part| match part {
        Part::Var(var) => Some(*var),
        Part::Text(_) => None,
      })
      .collect();
    f.debug_struct("Template")
      .field("present", &self.parts.is_some())
      .field("vars", &vars)
      .finish_non_exhaustive()
  }
}

impl Template {
  /// Parses `text`; `env` answers `${env:NAME}`. An unknown `${…}` is an error.
  pub fn parse(text: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
    let mut parts = Vec::new();
    let mut present = true;
    let mut literal = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("${") {
      literal.push_str(&rest[..at]);
      let after = &rest[at + 2..];
      let end = after
        .find('}')
        .ok_or_else(|| format!("unclosed `${{` in {text:?}"))?;
      let name = &after[..end];
      rest = &after[end + 1..];
      let var = match name {
        "user" => Var::User,
        "route" => Var::Route,
        "fingerprint" => Var::Fingerprint,
        "client_ip" => Var::ClientIp,
        _ => {
          let Some(spec) = name.strip_prefix("env:") else {
            return Err(format!("unknown variable `${{{name}}}`"));
          };
          let (var, default) = match spec.split_once(":-") {
            Some((var, default)) => (var, Some(default)),
            None => (spec, None),
          };
          if !env_name_ok(var) {
            return Err(format!("not an environment variable name: `${{{name}}}`"));
          }
          match env(var)
            .filter(|value| !value.is_empty())
            .or(default.map(str::to_owned))
          {
            Some(value) => literal.push_str(&value),
            None => present = false,
          }
          continue;
        }
      };
      if !literal.is_empty() {
        parts.push(Part::Text(std::mem::take(&mut literal)));
      }
      parts.push(Part::Var(var));
    }
    literal.push_str(rest);
    if !literal.is_empty() {
      parts.push(Part::Text(literal));
    }
    Ok(Self {
      parts: present.then_some(parts),
    })
  }

  /// The value for this session, or `None` when any variable in it has none.
  pub fn render(&self, vars: &SessionVars) -> Option<String> {
    let mut out = String::new();
    for part in self.parts.as_ref()? {
      match part {
        Part::Text(text) => out.push_str(text),
        Part::Var(Var::User) => out.push_str(vars.user.as_deref()?),
        Part::Var(Var::Route) => out.push_str(&vars.route),
        Part::Var(Var::Fingerprint) => out.push_str(vars.fingerprint.as_deref()?),
        Part::Var(Var::ClientIp) => out.push_str(&vars.client_ip?.to_string()),
      }
    }
    Some(out)
  }
}

fn env_name_ok(name: &str) -> bool {
  let mut bytes = name.bytes();
  bytes
    .next()
    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
    && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Snapshot {
  /// The route for SSH user `user`: the one that lists it, else the default.
  pub fn route_for(&self, user: &str) -> &Arc<Route> {
    let index = self.users.get(user).copied().unwrap_or(self.default_route);
    &self.routes[index]
  }

  /// The route used for user names no route lists.
  pub fn default_route(&self) -> &Arc<Route> {
    &self.routes[self.default_route]
  }

  /// A summary for `--check`: no environment values, only names.
  pub fn summary(&self) -> String {
    let mut out = String::new();
    let limits = &self.limits;
    let duration = |d: Option<Duration>| d.map_or_else(|| "never".to_owned(), format_duration);
    out.push_str(&format!("listen: {}\n", self.server.listen));
    out.push_str(&format!("host_key: {}\n", self.server.host_key.display()));
    out.push_str(&format!("server_id: {}\n", self.server.server_id));
    out.push_str(&format!(
      "banner: {}\n",
      if self.server.banner.is_empty() {
        "none"
      } else {
        "set"
      }
    ));
    out.push_str(&format!(
      "limits: max_sessions {} · max_sessions_per_ip {} · connections_per_minute {} · auth_timeout {} · idle_timeout {} · max_session_time {}\n",
      limits.max_sessions,
      limits.max_sessions_per_ip,
      if limits.connections_per_minute == 0 { "off".to_owned() } else { limits.connections_per_minute.to_string() },
      format_duration(limits.auth_timeout),
      duration(limits.idle_timeout),
      duration(limits.max_session_time),
    ));
    for (index, route) in self.routes.iter().enumerate() {
      let Upstream::Command(command) = &route.upstream;
      let mut line = vec![command.program.display().to_string()];
      line.extend(command.args.iter().cloned());
      out.push_str(&format!(
        "route {}{}\n  users: {}\n  command: {}\n  cwd: {}\n  env: {}\n  idle_timeout {} · max_session_time {} · max_sessions {}\n",
        route.name,
        if index == self.default_route { " (default)" } else { "" },
        route.users.join(", "),
        line.join(" "),
        command.cwd.display(),
        if route.env.is_empty() {
          "-".to_owned()
        } else {
          route.env.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(", ")
        },
        duration(route.limits.idle_timeout),
        duration(route.limits.max_session_time),
        route.limits.max_sessions.map_or_else(|| "-".to_owned(), |max| max.to_string()),
      ));
    }
    out
  }
}

/// A duration as the file would say it.
pub fn format_duration(duration: Duration) -> String {
  let millis = duration.as_millis();
  match millis {
    0 => "0".into(),
    _ if millis.is_multiple_of(3_600_000) => format!("{}h", millis / 3_600_000),
    _ if millis.is_multiple_of(60_000) => format!("{}m", millis / 60_000),
    _ if millis.is_multiple_of(1000) => format!("{}s", millis / 1000),
    _ => format!("{millis}ms"),
  }
}

/// `"500ms"`, `"30s"`, `"15m"`, `"2h"` or `"0"`.
pub fn parse_duration(text: &str) -> Result<Duration, String> {
  if text == "0" {
    return Ok(Duration::ZERO);
  }
  let digits = text.bytes().take_while(u8::is_ascii_digit).count();
  let (number, unit) = text.split_at(digits);
  let bad =
    || format!("not a duration like \"500ms\", \"30s\", \"15m\", \"2h\" or \"0\": {text:?}");
  let number: u64 = number.parse().map_err(|_| bad())?;
  let millis = match unit {
    "ms" => Some(number),
    "s" => number.checked_mul(1000),
    "m" => number.checked_mul(60_000),
    "h" => number.checked_mul(3_600_000),
    _ => return Err(bad()),
  };
  millis.map(Duration::from_millis).ok_or_else(bad)
}

/// Reads the file at `path` (see [`load_str`]).
pub fn load(
  path: &Path,
  env: impl Fn(&str) -> Option<String>,
  exe_dir: Option<&Path>,
) -> Result<(Snapshot, Vec<String>), ConfigError> {
  let text =
    std::fs::read_to_string(path).map_err(|e| ConfigError(format!("{}: {e}", path.display())))?;
  load_str(&text, env, exe_dir).map_err(|e| ConfigError(format!("{}: {e}", path.display())))
}

/// Checks the configuration `text`. `env` is the gateway's environment: it answers
/// `${env:…}` and the overrides `TTY_GATEWAY_ADDR` / `TTY_GATEWAY_HOST_KEY`; `exe_dir` is
/// where bare program names are looked for first. Returns the snapshot and warnings
/// (things that do not stop the server, like a program not there yet).
pub fn load_str(
  text: &str,
  env: impl Fn(&str) -> Option<String>,
  exe_dir: Option<&Path>,
) -> Result<(Snapshot, Vec<String>), ConfigError> {
  let raw: RawFile =
    toml::from_str(text).map_err(|e| ConfigError(e.to_string().trim_end().to_owned()))?;
  let env = |name: &str| env(name).filter(|value| !value.is_empty());
  let mut warnings = Vec::new();

  let server = &raw.server;
  let listen = env("TTY_GATEWAY_ADDR")
    .map(|value| ("TTY_GATEWAY_ADDR", value))
    .or_else(|| server.listen.clone().map(|value| ("server.listen", value)))
    .unwrap_or(("server.listen", "127.0.0.1:2222".into()));
  let listen: SocketAddr = listen
    .1
    .parse()
    .map_err(|_| error(listen.0, format!("not host:port: {:?}", listen.1)))?;
  let host_key = env("TTY_GATEWAY_HOST_KEY")
    .or_else(|| server.host_key.clone())
    .unwrap_or_else(|| ".data/ssh_host_ed25519_key".into());
  if host_key.is_empty() {
    return Err(error("server.host_key", "empty"));
  }
  let server_id = server
    .server_id
    .clone()
    .unwrap_or_else(|| "SSH-2.0-tty_gateway".into());
  let id_ok = server_id.strip_prefix("SSH-2.0-").is_some_and(|rest| {
    (1..=64).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_graphic() && b != b'-')
  });
  if !id_ok {
    return Err(error(
      "server.server_id",
      format!(
        "must be \"SSH-2.0-\" and 1 to 64 printable ASCII characters without spaces or '-': {server_id:?}"
      ),
    ));
  }
  let duration =
    |key: &str, value: &Option<String>, default: Duration| -> Result<Duration, ConfigError> {
      value.as_deref().map_or(Ok(default), |text| {
        parse_duration(text).map_err(|e| error(key, e))
      })
    };
  let never_if_zero = |d: Duration| (!d.is_zero()).then_some(d);
  let shutdown_grace = duration(
    "server.shutdown_grace",
    &server.shutdown_grace,
    Duration::from_secs(2),
  )?;

  let raw_limits = &raw.limits;
  let count = |key: &str, value: Option<u64>, default: usize| -> Result<usize, ConfigError> {
    let value = value.map_or(Ok(default), |value| {
      usize::try_from(value).map_err(|_| error(key, "too large"))
    })?;
    if value == 0 {
      return Err(error(key, "must be at least 1"));
    }
    Ok(value)
  };
  let auth_timeout = duration(
    "limits.auth_timeout",
    &raw_limits.auth_timeout,
    Duration::from_secs(30),
  )?;
  if auth_timeout < Duration::from_secs(1) {
    return Err(error("limits.auth_timeout", "must be at least 1s"));
  }
  let limits = Limits {
    max_sessions: count("limits.max_sessions", raw_limits.max_sessions, 64)?,
    max_sessions_per_ip: count(
      "limits.max_sessions_per_ip",
      raw_limits.max_sessions_per_ip,
      8,
    )?,
    connections_per_minute: raw_limits.connections_per_minute.unwrap_or(30),
    auth_timeout,
    idle_timeout: never_if_zero(duration(
      "limits.idle_timeout",
      &raw_limits.idle_timeout,
      Duration::from_secs(15 * 60),
    )?),
    max_session_time: never_if_zero(duration(
      "limits.max_session_time",
      &raw_limits.max_session_time,
      Duration::ZERO,
    )?),
  };

  let mut messages = Messages::default();
  messages.overlay(&raw.messages);

  if raw.routes.is_empty() {
    return Err(error(
      "routes",
      "at least one route is needed ([routes.NAME])",
    ));
  }
  let mut routes = Vec::new();
  let mut users: HashMap<String, usize> = HashMap::new();
  for (index, (name, raw_route)) in raw.routes.iter().enumerate() {
    let key = format!("routes.{name}");
    let name_ok = name.len() <= 32
      && name
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
      && name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    if !name_ok {
      return Err(error(
        &key,
        "a route name is 1 to 32 of a-z 0-9 _ - (starting with a letter or digit)",
      ));
    }
    let route_users = raw_route
      .users
      .clone()
      .unwrap_or_else(|| vec![name.clone()]);
    for user in &route_users {
      if SessionVars::sanitize_user(user).is_none() {
        return Err(error(
          format!("{key}.users"),
          format!("not a plain user name (1 to 64 of A-Z a-z 0-9 . _ @ + -): {user:?}"),
        ));
      }
      if let Some(other) = users.insert(user.clone(), index) {
        let other = raw.routes.keys().nth(other).map_or("", String::as_str);
        return Err(error(
          format!("{key}.users"),
          format!("{user:?} is in routes.{other} too"),
        ));
      }
    }
    let Some((program, args)) = raw_route.command.split_first() else {
      return Err(error(format!("{key}.command"), "must not be empty"));
    };
    if program.is_empty() {
      return Err(error(format!("{key}.command"), "the program is empty"));
    }
    let working_dir = || std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let mut program = PathBuf::from(program);
    if program.components().count() > 1 && program.is_relative() {
      program = working_dir().join(program);
    }
    let cwd = raw_route
      .cwd
      .as_ref()
      .map_or_else(std::env::temp_dir, |cwd| {
        let cwd = PathBuf::from(cwd);
        if cwd.is_relative() {
          working_dir().join(cwd)
        } else {
          cwd
        }
      });
    let command = CommandSpec {
      program,
      args: args.to_vec(),
      cwd,
      exe_dir: exe_dir.map(Path::to_owned),
    };
    match command.resolve() {
      Some(file) if file.is_file() => {}
      Some(file) => warnings.push(format!(
        "{key}.command: {} does not exist (yet)",
        file.display()
      )),
      None => warnings.push(format!(
        "{key}.command: {} is neither next to the gateway nor on PATH (yet)",
        command.program.display()
      )),
    }
    if !command.cwd.is_dir() {
      warnings.push(format!(
        "{key}.cwd: {} is not a directory (yet)",
        command.cwd.display()
      ));
    }
    let accept_env = match &raw_route.accept_env {
      None => ["LANG", "LC_*", "COLORTERM"]
        .iter()
        .filter_map(|text| EnvPattern::parse(text))
        .collect(),
      Some(list) => list
        .iter()
        .map(|text| {
          EnvPattern::parse(text).ok_or_else(|| {
            error(
              format!("{key}.accept_env"),
              format!("a name of A-Z a-z 0-9 _, or a prefix ending in '*': {text:?}"),
            )
          })
        })
        .collect::<Result<_, _>>()?,
    };
    let mut route_env = Vec::new();
    for (var, value) in &raw_route.env {
      if !env_name_ok(var) {
        return Err(error(
          format!("{key}.env"),
          format!("not an environment variable name: {var:?}"),
        ));
      }
      let template =
        Template::parse(value, &env).map_err(|e| error(format!("{key}.env.{var}"), e))?;
      route_env.push((var.clone(), template));
    }
    let route_limits = RouteLimits {
      idle_timeout: match &raw_route.idle_timeout {
        Some(_) => never_if_zero(duration(
          &format!("{key}.idle_timeout"),
          &raw_route.idle_timeout,
          Duration::ZERO,
        )?),
        None => limits.idle_timeout,
      },
      max_session_time: match &raw_route.max_session_time {
        Some(_) => never_if_zero(duration(
          &format!("{key}.max_session_time"),
          &raw_route.max_session_time,
          Duration::ZERO,
        )?),
        None => limits.max_session_time,
      },
      max_sessions: raw_route
        .max_sessions
        .map(|max| count(&format!("{key}.max_sessions"), Some(max), 1))
        .transpose()?,
    };
    let mut route_messages = messages.clone();
    route_messages.overlay(&raw_route.messages);
    routes.push(Arc::new(Route {
      name: name.clone(),
      users: route_users,
      upstream: Upstream::Command(command),
      accept_env,
      env: route_env,
      limits: route_limits,
      messages: route_messages,
    }));
  }

  let default_name = match (&server.default_route, routes.len()) {
    (Some(name), _) => name.clone(),
    (None, 1) => routes[0].name.clone(),
    (None, _) => {
      return Err(error(
        "server.default_route",
        "needed when there is more than one route",
      ));
    }
  };
  let default_route = routes
    .iter()
    .position(|route| route.name == default_name)
    .ok_or_else(|| {
      error(
        "server.default_route",
        format!("no route named {default_name:?}"),
      )
    })?;

  let snapshot = Snapshot {
    server: Server {
      listen,
      host_key: PathBuf::from(host_key),
      server_id,
      banner: server.banner.clone().unwrap_or_default(),
      default_route: default_name,
      shutdown_grace,
    },
    limits,
    messages,
    routes,
    default_route,
    users,
  };
  Ok((snapshot, warnings))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawFile {
  #[serde(default)]
  server: RawServer,
  #[serde(default)]
  limits: RawLimits,
  #[serde(default)]
  messages: RawMessages,
  #[serde(default)]
  routes: BTreeMap<String, RawRoute>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawServer {
  listen: Option<String>,
  host_key: Option<String>,
  server_id: Option<String>,
  banner: Option<String>,
  default_route: Option<String>,
  shutdown_grace: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawLimits {
  max_sessions: Option<u64>,
  max_sessions_per_ip: Option<u64>,
  connections_per_minute: Option<u32>,
  auth_timeout: Option<String>,
  idle_timeout: Option<String>,
  max_session_time: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawMessages {
  no_pty: Option<String>,
  unavailable: Option<String>,
  crashed: Option<String>,
  idle: Option<String>,
  time_limit: Option<String>,
  full: Option<String>,
  full_per_ip: Option<String>,
  full_route: Option<String>,
  shutting_down: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoute {
  users: Option<Vec<String>>,
  command: Vec<String>,
  cwd: Option<String>,
  accept_env: Option<Vec<String>>,
  idle_timeout: Option<String>,
  max_session_time: Option<String>,
  max_sessions: Option<u64>,
  #[serde(default)]
  env: BTreeMap<String, String>,
  #[serde(default)]
  messages: RawMessages,
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

  fn load_ok(text: &str) -> Snapshot {
    load_str(text, vars(&[]), None).unwrap().0
  }

  fn load_err(text: &str) -> String {
    load_str(text, vars(&[]), None).unwrap_err().0
  }

  const ONE: &str = "[routes.app]\ncommand = [\"/bin/true\"]\n";

  #[test]
  fn defaults() {
    let snapshot = load_ok(ONE);
    let server = &snapshot.server;
    assert_eq!(server.listen, "127.0.0.1:2222".parse().unwrap());
    assert_eq!(server.host_key, PathBuf::from(".data/ssh_host_ed25519_key"));
    assert_eq!(server.server_id, "SSH-2.0-tty_gateway");
    assert_eq!(server.banner, "");
    assert_eq!(server.default_route, "app");
    assert_eq!(server.shutdown_grace, Duration::from_secs(2));
    assert_eq!(
      snapshot.limits,
      Limits {
        max_sessions: 64,
        max_sessions_per_ip: 8,
        connections_per_minute: 30,
        auth_timeout: Duration::from_secs(30),
        idle_timeout: Some(Duration::from_secs(900)),
        max_session_time: None,
      }
    );
    assert_eq!(snapshot.messages, Messages::default());
    let route = snapshot.route_for("anyone");
    assert_eq!(route.name, "app");
    assert_eq!(route.users, ["app"]);
    let Upstream::Command(command) = &route.upstream;
    assert_eq!(command.program, PathBuf::from("/bin/true"));
    assert!(command.args.is_empty());
    assert_eq!(command.cwd, std::env::temp_dir());
    assert_eq!(
      route.accept_env,
      [
        EnvPattern::Exact("LANG".into()),
        EnvPattern::Prefix("LC_".into()),
        EnvPattern::Exact("COLORTERM".into())
      ]
    );
    assert!(route.env.is_empty());
    assert_eq!(
      route.limits,
      RouteLimits {
        idle_timeout: Some(Duration::from_secs(900)),
        max_session_time: None,
        max_sessions: None,
      }
    );
  }

  #[test]
  fn everything_set() {
    let text = r#"
[server]
listen = "0.0.0.0:22"
host_key = "/keys/host"
server_id = "SSH-2.0-Shop_1.0"
banner = "hello\n"
default_route = "b"
shutdown_grace = "500ms"

[limits]
max_sessions = 3
max_sessions_per_ip = 1
connections_per_minute = 0
auth_timeout = "5s"
idle_timeout = "0"
max_session_time = "2h"

[messages]
full = "full {max}"
idle = "idle {minutes}"

[routes.a]
users = ["alice", "al"]
command = ["/bin/echo", "-n", "hi"]
cwd = "/"
accept_env = ["LANG", "X_*"]
idle_timeout = "1m"
max_session_time = "0"
max_sessions = 2
[routes.a.env]
FIXED = "x"
WHO = "${user}@${route} from ${client_ip}"
[routes.a.messages]
full = "a is full"

[routes.b]
command = ["/bin/cat"]
"#;
    let snapshot = load_ok(text);
    assert_eq!(snapshot.server.listen, "0.0.0.0:22".parse().unwrap());
    assert_eq!(snapshot.server.host_key, PathBuf::from("/keys/host"));
    assert_eq!(snapshot.server.server_id, "SSH-2.0-Shop_1.0");
    assert_eq!(snapshot.server.banner, "hello\n");
    assert_eq!(snapshot.server.shutdown_grace, Duration::from_millis(500));
    assert_eq!(snapshot.limits.connections_per_minute, 0);
    assert_eq!(snapshot.limits.idle_timeout, None);
    assert_eq!(
      snapshot.limits.max_session_time,
      Some(Duration::from_secs(7200))
    );
    assert_eq!(snapshot.route_for("alice").name, "a");
    assert_eq!(snapshot.route_for("al").name, "a");
    assert_eq!(snapshot.route_for("a").name, "b", "only listed users");
    assert_eq!(snapshot.route_for("b").name, "b");
    assert_eq!(snapshot.route_for("zed").name, "b");
    let a = snapshot.route_for("alice");
    let Upstream::Command(command) = &a.upstream;
    assert_eq!(command.args, ["-n", "hi"]);
    assert_eq!(command.cwd, PathBuf::from("/"));
    assert_eq!(
      a.limits,
      RouteLimits {
        idle_timeout: Some(Duration::from_secs(60)),
        max_session_time: None,
        max_sessions: Some(2),
      }
    );
    // The route's own message, then the global ones, then the defaults.
    assert_eq!(a.messages.full(3), "a is full");
    assert_eq!(a.messages.idle(5), "idle 5");
    assert_eq!(a.messages.no_pty, Messages::default().no_pty);
    let b = snapshot.route_for("b");
    assert_eq!(b.messages.full(3), "full 3");
    assert_eq!(b.limits.max_session_time, Some(Duration::from_secs(7200)));
    let names: Vec<&str> = a.env.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["FIXED", "WHO"]);
    let session = SessionVars {
      user: Some("alice".into()),
      route: "a".into(),
      fingerprint: None,
      client_ip: Some("10.0.0.1".parse().unwrap()),
    };
    assert_eq!(
      a.env[1].1.render(&session).as_deref(),
      Some("alice@a from 10.0.0.1")
    );
  }

  #[test]
  fn the_environment_overrides_listen_and_host_key() {
    let (snapshot, _) = load_str(
      &format!("[server]\nlisten = \"127.0.0.1:1\"\n{ONE}"),
      vars(&[
        ("TTY_GATEWAY_ADDR", "0.0.0.0:2022"),
        ("TTY_GATEWAY_HOST_KEY", "/k"),
      ]),
      None,
    )
    .unwrap();
    assert_eq!(snapshot.server.listen, "0.0.0.0:2022".parse().unwrap());
    assert_eq!(snapshot.server.host_key, PathBuf::from("/k"));
    let error = load_str(ONE, vars(&[("TTY_GATEWAY_ADDR", "nowhere")]), None).unwrap_err();
    assert!(error.0.contains("TTY_GATEWAY_ADDR"), "{error}");
  }

  #[test]
  fn templates() {
    let env = vars(&[("SECRET", "s3cret"), ("EMPTY", "")]);
    let parse = |text: &str| Template::parse(text, &env);
    let keyless = SessionVars {
      user: Some("bob".into()),
      route: "r".into(),
      fingerprint: None,
      client_ip: Some("::1".parse().unwrap()),
    };
    let keyed = SessionVars {
      fingerprint: Some("SHA256:abc".into()),
      ..keyless.clone()
    };
    let render = |text: &str, session: &SessionVars| parse(text).unwrap().render(session);
    assert_eq!(
      render("plain $HOME $ {x}", &keyless).as_deref(),
      Some("plain $HOME $ {x}")
    );
    assert_eq!(render("${env:SECRET}", &keyless).as_deref(), Some("s3cret"));
    assert_eq!(render("k=${env:MISSING}", &keyless), None);
    assert_eq!(render("${env:EMPTY}", &keyless), None);
    assert_eq!(
      render("${env:MISSING:-dflt}", &keyless).as_deref(),
      Some("dflt")
    );
    assert_eq!(render("${env:EMPTY:-}", &keyless).as_deref(), Some(""));
    assert_eq!(
      render("${env:SECRET:-dflt}", &keyless).as_deref(),
      Some("s3cret")
    );
    assert_eq!(render("${fingerprint}", &keyless), None);
    assert_eq!(
      render("fp ${fingerprint}", &keyed).as_deref(),
      Some("fp SHA256:abc")
    );
    assert_eq!(
      render("${user}/${route}/${client_ip}", &keyed).as_deref(),
      Some("bob/r/::1")
    );
    let anonymous = SessionVars {
      user: None,
      ..keyed.clone()
    };
    assert_eq!(render("${user}", &anonymous), None);
    assert!(parse("${nope}").unwrap_err().contains("${nope}"));
    assert!(parse("${env:1BAD}").is_err());
    assert!(parse("${user").is_err());
    // Values never show in Debug output.
    let debug = format!("{:?}", parse("${env:SECRET} ${user}").unwrap());
    assert!(!debug.contains("s3cret"), "{debug}");
    let snapshot = load_str(
      "[routes.app]\ncommand = [\"/bin/true\"]\nenv = { KEY = \"${env:SECRET}\" }\n",
      env,
      None,
    )
    .unwrap()
    .0;
    let debug = format!("{snapshot:?}");
    assert!(
      !debug.contains("s3cret") && debug.contains("KEY"),
      "{debug}"
    );
  }

  #[test]
  fn user_names_are_sanitised() {
    assert_eq!(
      SessionVars::sanitize_user("alice.b-c_d+e@x").as_deref(),
      Some("alice.b-c_d+e@x")
    );
    assert_eq!(SessionVars::sanitize_user(""), None);
    assert_eq!(SessionVars::sanitize_user("a b"), None);
    assert_eq!(SessionVars::sanitize_user("$(reboot)"), None);
    assert_eq!(SessionVars::sanitize_user("ünï"), None);
    assert_eq!(SessionVars::sanitize_user(&"a".repeat(65)), None);
  }

  #[test]
  fn durations() {
    assert_eq!(parse_duration("0"), Ok(Duration::ZERO));
    assert_eq!(parse_duration("500ms"), Ok(Duration::from_millis(500)));
    assert_eq!(parse_duration("30s"), Ok(Duration::from_secs(30)));
    assert_eq!(parse_duration("15m"), Ok(Duration::from_secs(900)));
    assert_eq!(parse_duration("2h"), Ok(Duration::from_secs(7200)));
    for bad in [
      "",
      "15",
      "1.5s",
      "-1s",
      "s",
      "10 s",
      "3d",
      "99999999999999999999h",
    ] {
      assert!(parse_duration(bad).is_err(), "{bad:?}");
    }
    assert_eq!(format_duration(Duration::from_secs(900)), "15m");
    assert_eq!(format_duration(Duration::from_millis(1500)), "1500ms");
  }

  #[test]
  fn bad_files_name_the_key() {
    let with = |head: &str| format!("{head}\n{ONE}");
    let cases: Vec<(String, &str)> = vec![
      (String::new(), "routes"),
      ("[routes.app]\n".into(), "command"),
      ("[routes.app]\ncommand = []\n".into(), "routes.app.command"),
      (
        "[routes.App]\ncommand = [\"/bin/true\"]\n".into(),
        "routes.App",
      ),
      (with("[server]\nlisten = \"nowhere\""), "server.listen"),
      (
        with("[server]\nserver_id = \"SSH-2.0-has space\""),
        "server.server_id",
      ),
      (
        with("[server]\nserver_id = \"SSH-2.0-a-b\""),
        "server.server_id",
      ),
      (
        with("[server]\nserver_id = \"SSH-1.0-x\""),
        "server.server_id",
      ),
      (
        with("[server]\nshutdown_grace = \"soon\""),
        "server.shutdown_grace",
      ),
      (
        with("[server]\ndefault_route = \"nope\""),
        "server.default_route",
      ),
      (with("[server]\nport = 22"), "port"),
      (with("[limits]\nmax_sessions = 0"), "limits.max_sessions"),
      (
        with("[limits]\nmax_sessions_per_ip = 0"),
        "limits.max_sessions_per_ip",
      ),
      (
        with("[limits]\nauth_timeout = \"500ms\""),
        "limits.auth_timeout",
      ),
      (
        with("[limits]\nidle_timeout = \"15\""),
        "limits.idle_timeout",
      ),
      (with("[limits]\nmax_sessions = -1"), "max_sessions"),
      (with("[messages]\nhello = \"x\""), "hello"),
      (
        format!("{ONE}[routes.b]\ncommand = [\"/bin/true\"]\n"),
        "server.default_route",
      ),
      (
        "[server]\ndefault_route = \"a\"\n[routes.a]\ncommand = [\"/bin/true\"]\nusers = [\"x\"]\n[routes.b]\ncommand = [\"/bin/true\"]\nusers = [\"x\"]\n".into(),
        "routes.b.users",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\nusers = [\"a b\"]\n".into(),
        "routes.app.users",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\naccept_env = [\"LC_*X\"]\n".into(),
        "routes.app.accept_env",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\nenv = { \"A-B\" = \"x\" }\n".into(),
        "routes.app.env",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\nenv = { A = \"${who}\" }\n".into(),
        "routes.app.env.A",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\nmax_sessions = 0\n".into(),
        "routes.app.max_sessions",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\nidle_timeout = \"x\"\n".into(),
        "routes.app.idle_timeout",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\nshell = true\n".into(),
        "shell",
      ),
      (
        "[routes.app]\ncommand = [\"/bin/true\"]\n[routes.app.messages]\nnope = \"x\"\n".into(),
        "nope",
      ),
    ];
    for (text, key) in &cases {
      let error = load_err(text);
      assert!(error.contains(key), "{text:?}: {error}");
    }
  }

  #[test]
  fn a_missing_program_is_a_warning() {
    let (_, warnings) = load_str(
      "[routes.app]\ncommand = [\"/no/such/program\"]\ncwd = \"/no/such/dir\"\n",
      vars(&[]),
      None,
    )
    .unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("routes.app.command"));
    assert!(warnings[1].contains("routes.app.cwd"));
    let (_, warnings) = load_str(
      "[routes.app]\ncommand = [\"no-such-program-anywhere\"]\n",
      vars(&[]),
      None,
    )
    .unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
  }

  #[test]
  fn bare_names_are_looked_for_next_to_the_gateway_then_on_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("tty-gateway-resolve-{}", std::process::id()));
    let (exe_dir, on_path) = (dir.join("bin"), dir.join("path"));
    for sub in [&exe_dir, &on_path] {
      std::fs::create_dir_all(sub).unwrap();
    }
    let make = |path: &Path, mode: u32| {
      std::fs::write(path, "#!/bin/sh\n").unwrap();
      std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    make(&exe_dir.join("both"), 0o755);
    make(&on_path.join("both"), 0o755);
    make(&on_path.join("only"), 0o755);
    make(&exe_dir.join("plain"), 0o644);
    let path = std::env::join_paths([&on_path]).unwrap();
    let resolve = |name: &str| resolve_program(Path::new(name), Some(&exe_dir), Some(&path));
    assert_eq!(resolve("both"), Some(exe_dir.join("both")));
    assert_eq!(resolve("only"), Some(on_path.join("only")));
    assert_eq!(resolve("plain"), None, "not executable");
    assert_eq!(resolve("nothing"), None);
    assert_eq!(resolve("/bin/sh"), Some(PathBuf::from("/bin/sh")));
    std::fs::remove_dir_all(&dir).unwrap();
  }
}
