//! `coffee-tui`: one ROAST/TTY session in the current terminal.
//!
//! `coffee-sshd` starts one per SSH session on a PTY; locally, run it in any terminal.

fn main() -> std::io::Result<()> {
  coffee_host::run(coffee_host::session_env_from_process())
}
