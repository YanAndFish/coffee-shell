//! `coffee-tui`: one ROAST/TTY session in the current terminal.
//!
//! `tty-gateway` starts one per SSH session on a PTY; locally, run it in any terminal.

fn main() -> std::process::ExitCode {
  coffee_tui::main(coffee_tui::session_env_from_process())
}
