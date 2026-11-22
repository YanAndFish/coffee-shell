//! `coffee-sshd`: the ROAST/TTY SSH front end.
//!
//! Accepts any SSH client without a password: any public key is taken (its SHA256
//! fingerprint goes to the session), and a client without one logs in through
//! keyboard-interactive with zero prompts. Each session gets its own `coffee-tui` child
//! on a real PTY, with the backend's URL and the session's details in its environment;
//! the child goes when the client does. See `docs/adr/0001-ssh-session-model.md`.

pub mod config;
pub mod hostkey;
pub mod limits;
pub mod server;

pub use config::Config;
pub use server::{BANNER, serve};

/// Logs to stderr, one line per event with its fields; `COFFEE_SSH_LOG` (or
/// `RUST_LOG`) filters, `info` by default.
pub fn init_logging() {
  let filter = std::env::var("COFFEE_SSH_LOG")
    .or_else(|_| std::env::var("RUST_LOG"))
    .unwrap_or_else(|_| "info,russh=warn".into());
  let _ = tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
    .with_writer(std::io::stderr)
    .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
    .with_target(false)
    .try_init();
}
