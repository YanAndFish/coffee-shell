//! Toasts: short notices, one showing at a time (states.md: "同时一条"), the rest queued.
//! The shell shows [`ToastStore::current`] and dismisses it after [`ToastStore::duration`]
//! ([`TOAST_DURATION`], or [`QUEUED_DURATION`] once another toast waits, so news never lags
//! behind an old notice); then the next one shows. The queue is short ([`QUEUE_LIMIT`]): when it is full, the oldest
//! waiting toast gives way, so a burst of key presses never leaves stale notices behind.
//! Copy for each kind of toast: `docs/design/copy.md` ("Toast").

use std::time::Duration;

use nsh_core::prelude::*;

/// How long a toast shows (states.md: 2.6 s).
pub const TOAST_DURATION: Duration = Duration::from_millis(2600);

/// How long a toast shows at most once another one waits behind it.
pub const QUEUED_DURATION: Duration = Duration::from_millis(800);

/// How many toasts wait behind the one showing, at most.
pub const QUEUE_LIMIT: usize = 2;

/// A toast's kind (copy.md: `ok`, `info`, `warn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToastKind {
  /// Done (`✓`).
  Ok,
  /// For your information (`i`).
  Info,
  /// Something needs you (`!`).
  Warn,
}

impl ToastKind {
  /// Its glyph.
  pub fn glyph(self) -> &'static str {
    match self {
      ToastKind::Ok => "✓",
      ToastKind::Info => "i",
      ToastKind::Warn => "!",
    }
  }
}

/// A notice: what happened, and the next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
  /// Its kind.
  pub kind: ToastKind,
  /// What happened (4–8 characters): `已加入购物车`.
  pub title: SharedString,
  /// The detail or the next key: `耶加雪菲 孔加 · 250g · 整豆 × 1`.
  pub sub: SharedString,
  /// Which toast this is: a dismissal names the one it means.
  pub seq: u64,
}

/// The toast queue behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct ToastStore {
  queue: Signal<Vec<Toast>>,
  next_seq: Signal<u64>,
}

impl ToastStore {
  /// No toast.
  pub fn new() -> Self {
    Self {
      queue: signal(Vec::new()),
      next_seq: signal(1),
    }
  }

  /// The toast showing (tracked).
  pub fn current(&self) -> Option<Toast> {
    self.queue.with(|queue| queue.first().cloned())
  }

  /// The toast showing and those waiting, in order (tracked).
  pub fn queue(&self) -> Vec<Toast> {
    self.queue.get()
  }

  /// How long the toast showing stays now (tracked): shorter while others wait.
  pub fn duration(&self) -> Duration {
    if self.waiting() > 0 {
      QUEUED_DURATION
    } else {
      TOAST_DURATION
    }
  }

  /// How many toasts wait behind the one showing (tracked).
  pub fn waiting(&self) -> usize {
    self.queue.with(|queue| queue.len().saturating_sub(1))
  }

  /// Queues a toast (it shows at once when none is showing); its sequence number.
  pub fn show(
    &self,
    kind: ToastKind,
    title: impl Into<SharedString>,
    sub: impl Into<SharedString>,
  ) -> u64 {
    let seq = self.next_seq.get_untracked();
    self.next_seq.set(seq + 1);
    let toast = Toast {
      kind,
      title: title.into(),
      sub: sub.into(),
      seq,
    };
    self.queue.update(|queue| {
      if queue.len() > QUEUE_LIMIT {
        queue.remove(1);
      }
      queue.push(toast);
    });
    seq
  }

  /// `✓` toast.
  pub fn ok(&self, title: impl Into<SharedString>, sub: impl Into<SharedString>) -> u64 {
    self.show(ToastKind::Ok, title, sub)
  }

  /// `i` toast.
  pub fn info(&self, title: impl Into<SharedString>, sub: impl Into<SharedString>) -> u64 {
    self.show(ToastKind::Info, title, sub)
  }

  /// `!` toast.
  pub fn warn(&self, title: impl Into<SharedString>, sub: impl Into<SharedString>) -> u64 {
    self.show(ToastKind::Warn, title, sub)
  }

  /// Dismisses toast `seq` (showing or waiting); the next one shows.
  pub fn dismiss(&self, seq: u64) {
    if self
      .queue
      .with_untracked(|queue| queue.iter().any(|t| t.seq == seq))
    {
      self.queue.update(|queue| queue.retain(|t| t.seq != seq));
    }
  }

  /// Dismisses every toast.
  pub fn clear(&self) {
    self.queue.set(Vec::new());
  }
}

impl Default for ToastStore {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use nsh_core::element::{Runtime, create_root};

  use super::*;

  #[test]
  fn toasts_queue_and_show_one_at_a_time() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, toasts) = create_root(ToastStore::new);
    assert_eq!(toasts.current(), None);
    let first = toasts.info("提示", "第一条");
    let second = toasts.ok("完成", "第二条");
    assert_ne!(first, second);
    assert_eq!(toasts.current().map(|t| t.sub), Some("第一条".into()));
    assert_eq!(toasts.waiting(), 1);
    assert_eq!(toasts.duration(), QUEUED_DURATION, "news does not lag");
    toasts.dismiss(first);
    assert_eq!(toasts.current().map(|t| t.kind), Some(ToastKind::Ok));
    // A stale dismissal does nothing.
    toasts.dismiss(first);
    assert_eq!(toasts.current().map(|t| t.seq), Some(second));
    assert_eq!(toasts.duration(), TOAST_DURATION);
    toasts.dismiss(second);
    assert_eq!(toasts.current(), None);
    owner.dispose();
  }

  #[test]
  fn a_full_queue_drops_the_oldest_waiting_toast() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, toasts) = create_root(ToastStore::new);
    let showing = toasts.warn("a", "1");
    for n in 2..=6 {
      toasts.ok("b", n.to_string());
    }
    assert_eq!(toasts.current().map(|t| t.seq), Some(showing));
    assert_eq!(toasts.waiting(), QUEUE_LIMIT);
    toasts.dismiss(showing);
    assert_eq!(toasts.current().map(|t| t.sub), Some("5".into()));
    toasts.clear();
    assert_eq!(toasts.current(), None);
    assert_eq!(ToastKind::Warn.glyph(), "!");
    owner.dispose();
  }
}
