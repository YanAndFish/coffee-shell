//! Session limits: in all, and per client address.

use std::{
  collections::HashMap,
  net::IpAddr,
  sync::{Arc, Mutex},
};

/// The sessions open now.
#[derive(Debug)]
pub struct Limits {
  max_total: usize,
  max_per_ip: usize,
  state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
  total: usize,
  per_ip: HashMap<IpAddr, usize>,
}

/// Why a session was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
  /// The server is at `COFFEE_SSH_MAX_SESSIONS`.
  Full { max: usize },
  /// This address is at `COFFEE_SSH_MAX_SESSIONS_PER_IP`.
  PerIp { max: usize },
}

impl Refusal {
  /// What the client is told.
  pub fn message(self) -> String {
    match self {
      Refusal::Full { max } => {
        format!("ROAST/TTY 当前在线会话已满（上限 {max}），请稍后再试。")
      }
      Refusal::PerIp { max } => {
        format!("ROAST/TTY 来自你的地址的会话已达上限（{max} 个），请先关闭其他会话。")
      }
    }
  }

  /// The reason, for logs.
  pub fn reason(self) -> &'static str {
    match self {
      Refusal::Full { .. } => "max_sessions",
      Refusal::PerIp { .. } => "max_sessions_per_ip",
    }
  }
}

/// One session's place, given back on drop.
#[derive(Debug)]
pub struct Slot {
  limits: Arc<Limits>,
  ip: IpAddr,
}

impl Limits {
  pub fn new(max_total: usize, max_per_ip: usize) -> Arc<Self> {
    Arc::new(Self {
      max_total,
      max_per_ip,
      state: Mutex::new(State::default()),
    })
  }

  /// Takes a place for a session from `ip`, if there is one.
  pub fn acquire(self: &Arc<Self>, ip: IpAddr) -> Result<Slot, Refusal> {
    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    if state.total >= self.max_total {
      return Err(Refusal::Full {
        max: self.max_total,
      });
    }
    let count = state.per_ip.entry(ip).or_default();
    if *count >= self.max_per_ip {
      return Err(Refusal::PerIp {
        max: self.max_per_ip,
      });
    }
    *count += 1;
    state.total += 1;
    Ok(Slot {
      limits: self.clone(),
      ip,
    })
  }

  /// Sessions open now, in all.
  pub fn active(&self) -> usize {
    self.state.lock().unwrap_or_else(|e| e.into_inner()).total
  }
}

impl Drop for Slot {
  fn drop(&mut self) {
    let mut state = self.limits.state.lock().unwrap_or_else(|e| e.into_inner());
    state.total = state.total.saturating_sub(1);
    if let Some(count) = state.per_ip.get_mut(&self.ip) {
      *count -= 1;
      if *count == 0 {
        state.per_ip.remove(&self.ip);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn per_ip_and_total_limits_hold_and_free_on_drop() {
    let limits = Limits::new(3, 2);
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    let b: IpAddr = "10.0.0.2".parse().unwrap();
    let a1 = limits.acquire(a).unwrap();
    let _a2 = limits.acquire(a).unwrap();
    assert_eq!(limits.acquire(a).unwrap_err(), Refusal::PerIp { max: 2 });
    let _b1 = limits.acquire(b).unwrap();
    assert_eq!(limits.acquire(b).unwrap_err(), Refusal::Full { max: 3 });
    assert_eq!(limits.active(), 3);
    drop(a1);
    assert_eq!(limits.active(), 2);
    assert!(limits.acquire(b).is_ok());
  }

  #[test]
  fn refusals_explain_themselves() {
    assert!(Refusal::Full { max: 64 }.message().contains("64"));
    assert_eq!(Refusal::PerIp { max: 1 }.reason(), "max_sessions_per_ip");
  }
}
