//! [`Script`]: a transport for unit tests that answers each call only when the test says so,
//! in the order the test picks. The fixture transport answers at once and the same way
//! every time; races between requests (an old answer landing after a newer one) need this.

use std::{cell::RefCell, fmt, rc::Rc, task::Poll};

use super::{ApiFailure, Call};

type Slot = Rc<RefCell<Option<Result<(u16, String), ApiFailure>>>>;

struct Waiting {
  call: Call,
  answer: Slot,
}

/// The calls sent and not answered yet, oldest first. `Clone`: the test keeps one, the
/// client another.
#[derive(Clone, Default)]
pub struct Script(Rc<RefCell<Vec<Waiting>>>);

impl fmt::Debug for Script {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("Script").field(&self.calls()).finish()
  }
}

impl PartialEq for Script {
  fn eq(&self, other: &Self) -> bool {
    Rc::ptr_eq(&self.0, &other.0)
  }
}

impl Eq for Script {}

impl Script {
  /// The calls waiting for an answer, oldest first: `"PATCH /cart/lines/…"`.
  pub fn calls(&self) -> Vec<String> {
    self
      .0
      .borrow()
      .iter()
      .map(|w| format!("{} {}", w.call.endpoint.method.as_str(), w.call.path))
      .collect()
  }

  /// The JSON body of waiting call `index`.
  pub fn body(&self, index: usize) -> serde_json::Value {
    let waiting = self.0.borrow();
    let body = waiting[index].call.body.as_deref().unwrap_or(b"null");
    serde_json::from_slice(body).expect("request bodies are JSON")
  }

  /// Answers waiting call `index` with `status` and `body`.
  pub fn answer(&self, index: usize, status: u16, body: impl Into<String>) {
    self.settle(index, Ok((status, body.into())));
  }

  /// Fails waiting call `index` as a lost connection does.
  pub fn drop_connection(&self, index: usize) {
    self.settle(
      index,
      Err(ApiFailure::Network("script: connection lost".into())),
    );
  }

  fn settle(&self, index: usize, answer: Result<(u16, String), ApiFailure>) {
    let waiting = self.0.borrow_mut().remove(index);
    *waiting.answer.borrow_mut() = Some(answer);
  }

  /// Sends `call`: waits until the test answers it.
  pub(super) async fn send(&self, call: &Call) -> Result<(u16, String), ApiFailure> {
    let answer: Slot = Rc::default();
    self.0.borrow_mut().push(Waiting {
      call: call.clone(),
      answer: answer.clone(),
    });
    std::future::poll_fn(|_| match answer.borrow_mut().take() {
      Some(answer) => Poll::Ready(answer),
      None => Poll::Pending,
    })
    .await
  }
}

/// A future the test steps by hand: [`Self::poll`] runs it until it waits or ends.
pub struct Stepped<T>(Option<std::pin::Pin<Box<dyn std::future::Future<Output = T>>>>);

impl<T> Stepped<T> {
  /// Starts `future`: it runs until its first wait.
  pub fn start(future: impl std::future::Future<Output = T> + 'static) -> Self {
    let mut stepped = Self(Some(Box::pin(future)));
    assert!(stepped.poll().is_none(), "the future ended without waiting");
    stepped
  }

  /// Runs it on; its output once it ended.
  pub fn poll(&mut self) -> Option<T> {
    let future = self.0.as_mut().expect("polled after it ended");
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(output) => {
        self.0 = None;
        Some(output)
      }
      Poll::Pending => None,
    }
  }

  /// Runs it on; it must end now.
  pub fn finish(mut self) -> T {
    self.poll().expect("the future still waits")
  }
}
