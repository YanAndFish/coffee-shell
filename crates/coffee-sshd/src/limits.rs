//! Limits: sessions in all and per client address; before any handshake, open
//! connections and new connections per minute per client address.

use std::{
  collections::HashMap,
  net::IpAddr,
  sync::{Arc, Mutex},
  time::{Duration, Instant},
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

  /// Whether a session from `ip` would get a place now (nothing is taken).
  pub fn check(&self, ip: IpAddr) -> Result<(), Refusal> {
    let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    self.admits(&state, ip)
  }

  fn admits(&self, state: &State, ip: IpAddr) -> Result<(), Refusal> {
    if state.total >= self.max_total {
      return Err(Refusal::Full {
        max: self.max_total,
      });
    }
    if state.per_ip.get(&ip).copied().unwrap_or(0) >= self.max_per_ip {
      return Err(Refusal::PerIp {
        max: self.max_per_ip,
      });
    }
    Ok(())
  }

  /// Takes a place for a session from `ip`, if there is one.
  pub fn acquire(self: &Arc<Self>, ip: IpAddr) -> Result<Slot, Refusal> {
    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    self.admits(&state, ip)?;
    *state.per_ip.entry(ip).or_default() += 1;
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

/// Why a connection was dropped before its handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turned {
  /// The address has `max` connections open already.
  Open { max: usize },
  /// The address opened more than `per_minute` connections in the last minute or so.
  Rate { per_minute: u32 },
}

impl Turned {
  /// The reason, for logs.
  pub fn reason(self) -> &'static str {
    match self {
      Turned::Open { .. } => "connections_per_ip",
      Turned::Rate { .. } => "connections_per_minute",
    }
  }
}

/// The door before the handshake: open connections per address, and a token bucket of
/// new connections per address (`per_minute` tokens, refilled evenly over a minute).
#[derive(Debug)]
pub struct Gate {
  max_open: usize,
  per_minute: Option<u32>,
  state: Mutex<HashMap<IpAddr, Door>>,
}

#[derive(Clone, Copy, Debug)]
struct Door {
  open: usize,
  tokens: f64,
  refilled: Instant,
  /// The last connection from here was turned away (so the next one is not news).
  turned: bool,
}

/// One admitted connection, counted until it is dropped.
#[derive(Debug)]
pub struct Ticket {
  gate: Arc<Gate>,
  ip: IpAddr,
}

/// How many addresses the gate remembers before it forgets the settled ones.
const GATE_MEMORY: usize = 4096;

impl Gate {
  /// `per_minute` of `None` or 0 turns the rate limit off.
  pub fn new(max_open: usize, per_minute: Option<u32>) -> Arc<Self> {
    Arc::new(Self {
      max_open,
      per_minute: per_minute.filter(|rate| *rate > 0),
      state: Mutex::new(HashMap::new()),
    })
  }

  /// Lets a new connection from `ip` in at `now`, or says why not, and whether the one
  /// before from there was turned away too (worth one log line, not one per attempt).
  pub fn admit(self: &Arc<Self>, ip: IpAddr, now: Instant) -> Result<Ticket, (Turned, bool)> {
    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    if state.len() >= GATE_MEMORY {
      let per_minute = self.per_minute;
      state.retain(|_, door| door.open > 0 || !door.settled(per_minute, now));
    }
    let door = state.entry(ip).or_insert(Door {
      open: 0,
      tokens: self.per_minute.map_or(0.0, f64::from),
      refilled: now,
      turned: false,
    });
    let mut turned = None;
    if door.open >= self.max_open {
      turned = Some(Turned::Open { max: self.max_open });
    } else if let Some(per_minute) = self.per_minute {
      door.refill(per_minute, now);
      if door.tokens < 1.0 {
        turned = Some(Turned::Rate { per_minute });
      } else {
        door.tokens -= 1.0;
      }
    }
    if let Some(turned) = turned {
      let again = std::mem::replace(&mut door.turned, true);
      return Err((turned, again));
    }
    door.turned = false;
    door.open += 1;
    Ok(Ticket {
      gate: self.clone(),
      ip,
    })
  }
}

impl Door {
  fn refill(&mut self, per_minute: u32, now: Instant) {
    let per_second = f64::from(per_minute) / Duration::from_secs(60).as_secs_f64();
    let elapsed = now.saturating_duration_since(self.refilled).as_secs_f64();
    self.tokens = (self.tokens + elapsed * per_second).min(f64::from(per_minute));
    self.refilled = now;
  }

  /// Whether forgetting this door changes nothing: its bucket would be full again.
  fn settled(&self, per_minute: Option<u32>, now: Instant) -> bool {
    per_minute.is_none_or(|per_minute| {
      let mut door = *self;
      door.refill(per_minute, now);
      door.tokens >= f64::from(per_minute)
    })
  }
}

impl Drop for Ticket {
  fn drop(&mut self) {
    let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(door) = state.get_mut(&self.ip) {
      door.open = door.open.saturating_sub(1);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_check_takes_nothing() {
    let limits = Limits::new(1, 1);
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    assert!(limits.check(a).is_ok());
    assert!(limits.check(a).is_ok());
    let slot = limits.acquire(a).unwrap();
    assert_eq!(limits.check(a).unwrap_err(), Refusal::Full { max: 1 });
    drop(slot);
    assert!(limits.check(a).is_ok());
  }

  #[test]
  fn the_gate_caps_open_connections_per_address() {
    let gate = Gate::new(2, None);
    let now = Instant::now();
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    let b: IpAddr = "10.0.0.2".parse().unwrap();
    let first = gate.admit(a, now).unwrap();
    let _second = gate.admit(a, now).unwrap();
    assert_eq!(
      gate.admit(a, now).unwrap_err(),
      (Turned::Open { max: 2 }, false)
    );
    // Only the first refusal in a row is news.
    assert!(gate.admit(a, now).unwrap_err().1);
    assert!(gate.admit(b, now).is_ok());
    drop(first);
    assert!(gate.admit(a, now).is_ok());
  }

  #[test]
  fn the_gate_limits_new_connections_per_minute() {
    let gate = Gate::new(100, Some(3));
    let start = Instant::now();
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    for _ in 0..3 {
      drop(gate.admit(a, start).unwrap());
    }
    assert_eq!(
      gate.admit(a, start).unwrap_err(),
      (Turned::Rate { per_minute: 3 }, false)
    );
    // Each address has its own bucket; one token comes back every 20 seconds.
    assert!(gate.admit("10.0.0.2".parse().unwrap(), start).is_ok());
    assert!(gate.admit(a, start + Duration::from_secs(19)).is_err());
    assert!(gate.admit(a, start + Duration::from_secs(21)).is_ok());
    assert!(gate.admit(a, start + Duration::from_secs(22)).is_err());
    // 0 turns it off.
    let open = Gate::new(100, Some(0));
    for _ in 0..50 {
      drop(open.admit(a, start).unwrap());
    }
  }

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
