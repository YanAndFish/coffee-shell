//! Limits: sessions in all, per client address and per route; before any handshake,
//! open connections and new connections per minute per client address.
//!
//! The counters live as long as the process; the caps come with every call, from the
//! configuration in force at that moment, so a reload changes the caps without losing
//! count of what is open.

use std::{
  collections::HashMap,
  net::IpAddr,
  sync::{Arc, Mutex},
  time::{Duration, Instant},
};

/// The sessions open now.
#[derive(Debug, Default)]
pub struct Limits {
  state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
  total: usize,
  per_ip: HashMap<IpAddr, usize>,
  per_route: HashMap<Arc<str>, usize>,
}

/// The session caps in force: in all and per client address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
  pub max_sessions: usize,
  pub max_sessions_per_ip: usize,
}

/// Why a session was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
  /// The server is at `limits.max_sessions`.
  Full { max: usize },
  /// This address is at `limits.max_sessions_per_ip`.
  PerIp { max: usize },
  /// The route is at its `max_sessions`.
  Route { max: usize },
}

impl Refusal {
  /// The reason, for logs.
  pub fn reason(self) -> &'static str {
    match self {
      Refusal::Full { .. } => "max_sessions",
      Refusal::PerIp { .. } => "max_sessions_per_ip",
      Refusal::Route { .. } => "route_max_sessions",
    }
  }

  /// The cap that was reached.
  pub fn max(self) -> usize {
    match self {
      Refusal::Full { max } | Refusal::PerIp { max } | Refusal::Route { max } => max,
    }
  }
}

/// One session's place, given back on drop.
#[derive(Debug)]
pub struct Slot {
  limits: Arc<Limits>,
  ip: IpAddr,
  route: Arc<str>,
}

impl Limits {
  pub fn new() -> Arc<Self> {
    Arc::new(Self::default())
  }

  /// Whether a session from `ip` would get a place under `caps` now (nothing is taken;
  /// no route is known yet).
  pub fn check(&self, ip: IpAddr, caps: Caps) -> Result<(), Refusal> {
    let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    admits(&state, ip, caps)
  }

  /// Takes a place for a session from `ip` on `route`, if `caps` and the route's own cap
  /// leave one.
  pub fn acquire(
    self: &Arc<Self>,
    ip: IpAddr,
    route: &str,
    caps: Caps,
    route_max: Option<usize>,
  ) -> Result<Slot, Refusal> {
    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    admits(&state, ip, caps)?;
    if let Some(max) = route_max
      && state.per_route.get(route).copied().unwrap_or(0) >= max
    {
      return Err(Refusal::Route { max });
    }
    let route: Arc<str> = route.into();
    *state.per_ip.entry(ip).or_default() += 1;
    *state.per_route.entry(route.clone()).or_default() += 1;
    state.total += 1;
    Ok(Slot {
      limits: self.clone(),
      ip,
      route,
    })
  }

  /// Sessions open now, in all.
  pub fn active(&self) -> usize {
    self.state.lock().unwrap_or_else(|e| e.into_inner()).total
  }
}

fn admits(state: &State, ip: IpAddr, caps: Caps) -> Result<(), Refusal> {
  if state.total >= caps.max_sessions {
    return Err(Refusal::Full {
      max: caps.max_sessions,
    });
  }
  if state.per_ip.get(&ip).copied().unwrap_or(0) >= caps.max_sessions_per_ip {
    return Err(Refusal::PerIp {
      max: caps.max_sessions_per_ip,
    });
  }
  Ok(())
}

fn release<K: std::hash::Hash + Eq>(counts: &mut HashMap<K, usize>, key: &K) {
  if let Some(count) = counts.get_mut(key) {
    *count = count.saturating_sub(1);
    if *count == 0 {
      counts.remove(key);
    }
  }
}

impl Drop for Slot {
  fn drop(&mut self) {
    let mut state = self.limits.state.lock().unwrap_or_else(|e| e.into_inner());
    state.total = state.total.saturating_sub(1);
    release(&mut state.per_ip, &self.ip);
    release(&mut state.per_route, &self.route);
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
#[derive(Debug, Default)]
pub struct Gate {
  state: Mutex<HashMap<IpAddr, Door>>,
}

#[derive(Clone, Copy, Debug)]
struct Door {
  open: usize,
  tokens: f64,
  refilled: Instant,
  /// The rate the bucket was last filled at (0: off).
  rate: u32,
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
  pub fn new() -> Arc<Self> {
    Arc::new(Self::default())
  }

  /// Lets a new connection from `ip` in at `now` if the address has fewer than
  /// `max_open` open and its bucket of `per_minute` (0: no rate limit) has a token, or
  /// says why not, and whether the one before from there was turned away too (worth one
  /// log line, not one per attempt).
  pub fn admit(
    self: &Arc<Self>,
    ip: IpAddr,
    now: Instant,
    max_open: usize,
    per_minute: u32,
  ) -> Result<Ticket, (Turned, bool)> {
    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
    if state.len() >= GATE_MEMORY {
      state.retain(|_, door| door.open > 0 || !door.settled(per_minute, now));
    }
    let door = state.entry(ip).or_insert(Door {
      open: 0,
      tokens: f64::from(per_minute),
      refilled: now,
      rate: per_minute,
      turned: false,
    });
    let mut turned = None;
    if door.open >= max_open {
      turned = Some(Turned::Open { max: max_open });
    } else if per_minute > 0 {
      door.refill(per_minute, now);
      if door.tokens < 1.0 {
        turned = Some(Turned::Rate { per_minute });
      } else {
        door.tokens -= 1.0;
      }
    } else {
      door.refill(0, now);
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
  /// Brings the bucket to `now` at `per_minute` (0: off). A bucket that was off starts
  /// full when a rate comes on; a lower rate caps what is left.
  fn refill(&mut self, per_minute: u32, now: Instant) {
    if per_minute != self.rate {
      self.tokens = if self.rate == 0 {
        f64::from(per_minute)
      } else {
        self.tokens.min(f64::from(per_minute))
      };
      self.rate = per_minute;
      self.refilled = now;
      return;
    }
    let per_second = f64::from(per_minute) / Duration::from_secs(60).as_secs_f64();
    let elapsed = now.saturating_duration_since(self.refilled).as_secs_f64();
    self.tokens = (self.tokens + elapsed * per_second).min(f64::from(per_minute));
    self.refilled = now;
  }

  /// Whether forgetting this door changes nothing: its bucket would be full again.
  fn settled(&self, per_minute: u32, now: Instant) -> bool {
    per_minute == 0 || {
      let mut door = *self;
      door.refill(per_minute, now);
      door.tokens >= f64::from(per_minute)
    }
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

  fn caps(max_sessions: usize, max_sessions_per_ip: usize) -> Caps {
    Caps {
      max_sessions,
      max_sessions_per_ip,
    }
  }

  #[test]
  fn a_check_takes_nothing() {
    let limits = Limits::new();
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    assert!(limits.check(a, caps(1, 1)).is_ok());
    assert!(limits.check(a, caps(1, 1)).is_ok());
    let slot = limits.acquire(a, "app", caps(1, 1), None).unwrap();
    assert_eq!(
      limits.check(a, caps(1, 1)).unwrap_err(),
      Refusal::Full { max: 1 }
    );
    drop(slot);
    assert!(limits.check(a, caps(1, 1)).is_ok());
  }

  #[test]
  fn the_gate_caps_open_connections_per_address() {
    let gate = Gate::new();
    let now = Instant::now();
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    let b: IpAddr = "10.0.0.2".parse().unwrap();
    let first = gate.admit(a, now, 2, 0).unwrap();
    let _second = gate.admit(a, now, 2, 0).unwrap();
    assert_eq!(
      gate.admit(a, now, 2, 0).unwrap_err(),
      (Turned::Open { max: 2 }, false)
    );
    // Only the first refusal in a row is news.
    assert!(gate.admit(a, now, 2, 0).unwrap_err().1);
    assert!(gate.admit(b, now, 2, 0).is_ok());
    drop(first);
    assert!(gate.admit(a, now, 2, 0).is_ok());
  }

  #[test]
  fn the_gate_limits_new_connections_per_minute() {
    let gate = Gate::new();
    let start = Instant::now();
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    for _ in 0..3 {
      drop(gate.admit(a, start, 100, 3).unwrap());
    }
    assert_eq!(
      gate.admit(a, start, 100, 3).unwrap_err(),
      (Turned::Rate { per_minute: 3 }, false)
    );
    // Each address has its own bucket; one token comes back every 20 seconds.
    assert!(
      gate
        .admit("10.0.0.2".parse().unwrap(), start, 100, 3)
        .is_ok()
    );
    assert!(
      gate
        .admit(a, start + Duration::from_secs(19), 100, 3)
        .is_err()
    );
    assert!(
      gate
        .admit(a, start + Duration::from_secs(21), 100, 3)
        .is_ok()
    );
    assert!(
      gate
        .admit(a, start + Duration::from_secs(22), 100, 3)
        .is_err()
    );
    // 0 turns it off.
    let open = Gate::new();
    for _ in 0..50 {
      drop(open.admit(a, start, 100, 0).unwrap());
    }
  }

  #[test]
  fn per_ip_and_total_limits_hold_and_free_on_drop() {
    let limits = Limits::new();
    let caps = caps(3, 2);
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    let b: IpAddr = "10.0.0.2".parse().unwrap();
    let a1 = limits.acquire(a, "app", caps, None).unwrap();
    let _a2 = limits.acquire(a, "app", caps, None).unwrap();
    assert_eq!(
      limits.acquire(a, "app", caps, None).unwrap_err(),
      Refusal::PerIp { max: 2 }
    );
    let _b1 = limits.acquire(b, "app", caps, None).unwrap();
    assert_eq!(
      limits.acquire(b, "app", caps, None).unwrap_err(),
      Refusal::Full { max: 3 }
    );
    assert_eq!(limits.active(), 3);
    drop(a1);
    assert_eq!(limits.active(), 2);
    assert!(limits.acquire(b, "app", caps, None).is_ok());
  }

  #[test]
  fn refusals_explain_themselves() {
    assert_eq!(Refusal::Full { max: 64 }.max(), 64);
    assert_eq!(Refusal::PerIp { max: 1 }.reason(), "max_sessions_per_ip");
    assert_eq!(Refusal::Route { max: 2 }.reason(), "route_max_sessions");
  }

  #[test]
  fn a_route_cap_counts_only_its_route() {
    let limits = Limits::new();
    let caps = caps(10, 10);
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    let one = limits.acquire(a, "one", caps, Some(1)).unwrap();
    assert_eq!(
      limits.acquire(a, "one", caps, Some(1)).unwrap_err(),
      Refusal::Route { max: 1 }
    );
    let _other = limits.acquire(a, "two", caps, Some(1)).unwrap();
    drop(one);
    assert!(limits.acquire(a, "one", caps, Some(1)).is_ok());
  }

  #[test]
  fn caps_can_change_while_places_are_held() {
    let limits = Limits::new();
    let a: IpAddr = "10.0.0.1".parse().unwrap();
    let held: Vec<Slot> = (0..2)
      .map(|_| limits.acquire(a, "app", caps(3, 3), None).unwrap())
      .collect();
    // Lowered below what is open: nothing more, and the open ones stay counted.
    assert_eq!(
      limits.check(a, caps(2, 3)).unwrap_err(),
      Refusal::Full { max: 2 }
    );
    assert_eq!(
      limits.acquire(a, "app", caps(5, 2), None).unwrap_err(),
      Refusal::PerIp { max: 2 }
    );
    assert_eq!(
      limits.acquire(a, "app", caps(5, 5), Some(2)).unwrap_err(),
      Refusal::Route { max: 2 }
    );
    // Raised: room again, counted from what is held.
    let third = limits.acquire(a, "app", caps(5, 5), None).unwrap();
    assert_eq!(limits.active(), 3);
    drop(held);
    drop(third);
    assert_eq!(limits.active(), 0);
    assert!(limits.acquire(a, "app", caps(1, 1), Some(1)).is_ok());

    // The gate: open connections are counted across a cap change.
    let gate = Gate::new();
    let now = Instant::now();
    let _t1 = gate.admit(a, now, 2, 0).unwrap();
    let _t2 = gate.admit(a, now, 2, 0).unwrap();
    assert!(gate.admit(a, now, 2, 0).is_err());
    assert!(gate.admit(a, now, 1, 0).is_err());
    assert!(gate.admit(a, now, 3, 0).is_ok());
    // A rate switched on starts with a full bucket, not an empty one.
    let b: IpAddr = "10.0.0.2".parse().unwrap();
    drop(gate.admit(b, now, 100, 0).unwrap());
    for _ in 0..2 {
      drop(gate.admit(b, now, 100, 2).unwrap());
    }
    assert!(gate.admit(b, now, 100, 2).is_err());
    // A lower rate caps what is left.
    let c: IpAddr = "10.0.0.3".parse().unwrap();
    drop(gate.admit(c, now, 100, 10).unwrap());
    drop(gate.admit(c, now, 100, 1).unwrap());
    assert!(gate.admit(c, now, 100, 1).is_err());
  }
}
