//! `coffee-tui`: one ROAST/TTY session in the current terminal.
//!
//! `tty-gateway` starts one per SSH session on a PTY; locally, run it in any terminal.

fn main() -> std::io::Result<()> {
  coffee_tui::run(coffee_tui::session_env_from_process())
}
