//! The clipboard over the terminal (OSC 52): what the app copies reaches the clipboard
//! of the machine the SSH client runs on, never the server's.
//!
//! Adapted from `nsh-tui-host` 0.6.1 (`clipboard.rs`, Apache-2.0), without its fallback
//! to the local desktop clipboard: on a server that would be the wrong machine's.

use std::{
  io::{self, Write},
  sync::Mutex,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nsh_core::clipboard::ClipboardBackend;

/// Text copied in this session, sent to the terminal between frames.
#[derive(Default)]
pub struct TerminalClipboard {
  state: Mutex<State>,
}

#[derive(Default)]
struct State {
  copied: Option<String>,
  pending: Option<String>,
}

impl TerminalClipboard {
  /// Writes a pending copy as OSC 52. Engine calls arrive off the host thread; this
  /// runs between frames, so the escape never splits a frame's.
  pub fn flush(&self, output: &mut impl Write) -> io::Result<()> {
    let pending = self
      .state
      .lock()
      .unwrap_or_else(|error| error.into_inner())
      .pending
      .take();
    if let Some(encoded) = pending {
      write!(output, "\x1b]52;c;{encoded}\x07")?;
    }
    Ok(())
  }
}

impl ClipboardBackend for TerminalClipboard {
  fn text(&self) -> Option<String> {
    // OSC 52 reads are widely disabled; paste arrives as bracketed paste instead.
    self
      .state
      .lock()
      .unwrap_or_else(|error| error.into_inner())
      .copied
      .clone()
  }

  fn set_text(&self, text: Option<String>) {
    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
    state.pending = Some(STANDARD.encode(text.as_deref().unwrap_or_default()));
    state.copied = text;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_copy_goes_out_once_as_osc_52() {
    let clipboard = TerminalClipboard::default();
    clipboard.set_text(Some("RT2611220001".into()));
    assert_eq!(clipboard.text().as_deref(), Some("RT2611220001"));
    let mut out = Vec::new();
    clipboard.flush(&mut out).unwrap();
    assert_eq!(out, b"\x1b]52;c;UlQyNjExMjIwMDAx\x07");
    out.clear();
    clipboard.flush(&mut out).unwrap();
    assert!(out.is_empty());
  }
}
