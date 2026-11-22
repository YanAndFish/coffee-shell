//! Toasts (stub): one short notice at a time, a new one replacing the last. The shell shows
//! the current one above the status bar and dismisses it after [`TOAST_DURATION`].
//! Phase 2 adds the design's kinds of use (added to cart, address needed, …).

use std::time::Duration;

use nsh_core::prelude::*;

pub use crate::ui::ToastKind;

/// How long a toast shows.
pub const TOAST_DURATION: Duration = Duration::from_secs(3);

/// A notice: what happened, and the next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
  /// Its kind.
  pub kind: ToastKind,
  /// What happened (4–8 characters): `已加入购物车`.
  pub title: SharedString,
  /// The detail or the next key: `耶加雪菲 孔加 · 250g × 1`.
  pub sub: SharedString,
  /// Which toast this is: a dismissal names the one it means.
  pub seq: u64,
}

/// The toast state behind its functions. `Copy`: hold it anywhere.
#[derive(Clone, Copy)]
pub struct ToastStore {
  current: Signal<Option<Toast>>,
  next_seq: Signal<u64>,
}

impl ToastStore {
  /// No toast.
  pub fn new() -> Self {
    Self {
      current: signal(None),
      next_seq: signal(1),
    }
  }

  /// The toast showing (tracked).
  pub fn current(&self) -> Option<Toast> {
    self.current.get()
  }

  /// Shows a toast in place of any other; its sequence number.
  pub fn show(
    &self,
    kind: ToastKind,
    title: impl Into<SharedString>,
    sub: impl Into<SharedString>,
  ) -> u64 {
    let seq = self.next_seq.get_untracked();
    self.next_seq.set(seq + 1);
    self.current.set(Some(Toast {
      kind,
      title: title.into(),
      sub: sub.into(),
      seq,
    }));
    seq
  }

  /// Dismisses toast `seq` if it still shows (a later one stays).
  pub fn dismiss(&self, seq: u64) {
    if self
      .current
      .with_untracked(|toast| toast.as_ref().is_some_and(|t| t.seq == seq))
    {
      self.current.set(None);
    }
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
  fn a_new_toast_replaces_the_last_and_stale_dismissals_do_nothing() {
    let runtime = Runtime::default();
    let _entered = runtime.enter();
    let (owner, toasts) = create_root(ToastStore::new);
    assert_eq!(toasts.current(), None);
    let first = toasts.show(ToastKind::Info, "提示", "第一条");
    let second = toasts.show(ToastKind::Ok, "完成", "第二条");
    assert_ne!(first, second);
    assert_eq!(toasts.current().map(|t| t.sub), Some("第二条".into()));
    toasts.dismiss(first);
    assert!(
      toasts.current().is_some(),
      "dismissing the first keeps the second"
    );
    toasts.dismiss(second);
    assert_eq!(toasts.current(), None);
    owner.dispose();
  }
}
