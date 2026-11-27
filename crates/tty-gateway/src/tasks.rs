//! Spawning connection and session tasks: each holds a [`Tracker`] (so a shutdown can
//! wait for all of them), and a panic in one is logged with its connection id and taken
//! down with that task only.

use std::{future::Future, time::Duration};

use tokio::sync::mpsc;

/// Held by every task; [`Tasks::wait`] returns once every copy is gone.
#[derive(Clone, Debug)]
pub struct Tracker(#[allow(dead_code)] mpsc::Sender<()>);

/// The other end of the trackers.
#[derive(Debug)]
pub struct Tasks(mpsc::Receiver<()>);

/// A new tracker and what waits for it.
pub fn tracker() -> (Tracker, Tasks) {
  let (sender, receiver) = mpsc::channel(1);
  (Tracker(sender), Tasks(receiver))
}

impl Tasks {
  /// Waits until no tracker is left, at most `limit`; whether none is.
  pub async fn wait(&mut self, limit: Duration) -> bool {
    tokio::time::timeout(limit, async { while self.0.recv().await.is_some() {} })
      .await
      .is_ok()
  }
}

/// Runs `task` on its own, holding `tracker`; a panic is logged as `what` of `conn`.
pub fn spawn<F>(what: &'static str, conn: u64, tracker: &Tracker, task: F)
where
  F: Future<Output = ()> + Send + 'static,
{
  let tracker = tracker.clone();
  let handle = tokio::spawn(async move {
    let _tracker = tracker;
    task.await;
  });
  tokio::spawn(async move {
    if let Err(error) = handle.await
      && error.is_panic()
    {
      tracing::error!(conn, task = what, "task panicked");
    }
  });
}
