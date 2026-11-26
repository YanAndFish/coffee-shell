//! `tty-gateway`: an SSH front end for terminal programs.
//!
//! One port; the SSH user name picks a route (a configured user, else the default
//! route), and each session starts that route's program on its own PTY, with an
//! environment the route spells out. Any SSH client logs in without a password: any
//! public key is taken (its SHA256 fingerprint is `${fingerprint}`), and a client without
//! one logs in through keyboard-interactive with zero prompts. The program goes when the
//! client does. The configuration is a TOML file: see [`config`].

pub mod config;
pub mod hostkey;
pub mod limits;
pub mod server;
pub mod session;
pub mod tasks;
pub mod upstream;

pub use config::{ConfigError, Snapshot, load, load_str};
pub use server::serve;

/// Logs to stderr, one line per event with its fields; `TTY_GATEWAY_LOG` (or
/// `RUST_LOG`) filters, `info,russh=warn` by default.
pub fn init_logging() {
  let filter = std::env::var("TTY_GATEWAY_LOG")
    .or_else(|_| std::env::var("RUST_LOG"))
    .unwrap_or_else(|_| "info,russh=warn".into());
  let _ = tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
    .with_writer(std::io::stderr)
    .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
    .with_target(false)
    .try_init();
}
