//! The system row's clock: the server's local time as `HH:MM`, turning on each minute
//! (states.md: "时钟 每分钟"). The process runs on the server, so its local time zone is the
//! server's: `TZ` (a zone name, a path into the zone database, or a fixed POSIX offset such
//! as `CST-8`), else `/etc/localtime`, else UTC. The offset comes from the zone's TZif file
//! (the last transition before now; past the file's last one, the standard offset of its
//! footer rule, so a zone that changes to summer time after that last transition shows
//! standard time).

use std::time::Duration;

use nsh_core::prelude::*;

/// The local time of `unix_secs` shifted by `offset` seconds east of UTC, as `HH:MM`.
pub fn hh_mm(unix_secs: i64, offset: i64) -> String {
  let day = (unix_secs + offset).rem_euclid(86_400);
  format!("{:02}:{:02}", day / 3600, day % 3600 / 60)
}

/// The milliseconds from `unix_ms` to the next whole minute (at least 1).
pub fn until_next_minute(unix_ms: f64) -> Duration {
  let ms = unix_ms.max(0.0) as u64;
  Duration::from_millis((60_000 - ms % 60_000).max(1))
}

fn be_i32(bytes: &[u8], at: usize) -> Option<i32> {
  Some(i32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be_i64(bytes: &[u8], at: usize) -> Option<i64> {
  Some(i64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// The counts of a TZif header at `at`: (isut, isstd, leap, time, type, char).
fn counts(bytes: &[u8], at: usize) -> Option<[usize; 6]> {
  if bytes.get(at..at + 4)? != b"TZif" {
    return None;
  }
  let mut counts = [0; 6];
  for (i, count) in counts.iter_mut().enumerate() {
    *count = usize::try_from(be_i32(bytes, at + 20 + i * 4)?).ok()?;
  }
  Some(counts)
}

/// The UTC offset (seconds east) that the TZif file `bytes` gives at `unix_secs`.
pub fn tzif_offset(bytes: &[u8], unix_secs: i64) -> Option<i64> {
  let [isut, isstd, leap, time, types, chars] = counts(bytes, 0)?;
  let version = *bytes.get(4)?;
  // Version 2 and later repeat the data with 64-bit times after the first block.
  let (at, wide, [_, _, _, time, types, _]) = if version >= b'2' {
    let second = 44 + time * 5 + types * 6 + chars + leap * 8 + isstd + isut;
    (second + 44, true, counts(bytes, second)?)
  } else {
    (44, false, [isut, isstd, leap, time, types, chars])
  };
  let size = if wide { 8 } else { 4 };
  let times = at;
  let indices = times + time * size;
  let infos = indices + time;
  let offset_of = |index: usize| -> Option<i64> {
    (index < types).then_some(())?;
    be_i32(bytes, infos + index * 6).map(i64::from)
  };
  let mut current = None;
  for i in 0..time {
    let at = if wide {
      be_i64(bytes, times + i * 8)?
    } else {
      i64::from(be_i32(bytes, times + i * 4)?)
    };
    if at > unix_secs {
      break;
    }
    current = Some(i);
  }
  match current {
    Some(i) if i + 1 < time || !wide => offset_of(usize::from(*bytes.get(indices + i)?)),
    // Past the last transition: the footer's rule, when there is one.
    _ => {
      let footer = wide
        .then(|| {
          let [isut, isstd, leap, ..] = counts(bytes, at - 44)?;
          let end = infos + types * 6 + chars + leap * 12 + isstd + isut;
          let rest = bytes.get(end..)?;
          let text = std::str::from_utf8(rest).ok()?;
          posix_offset(text.trim_matches('\n'))
        })
        .flatten();
      match (footer, current) {
        (Some(offset), _) => Some(offset),
        (None, Some(i)) => offset_of(usize::from(*bytes.get(indices + i)?)),
        (None, None) => offset_of(0),
      }
    }
  }
}

/// The standard offset (seconds east) of a POSIX `TZ` rule: `CST-8` → +8 h, `<+0530>-5:30`
/// → +5:30, `EST5EDT,M3.2.0,M11.1.0` → −5 h (summer time is not applied).
pub fn posix_offset(rule: &str) -> Option<i64> {
  let rest = if let Some(quoted) = rule.strip_prefix('<') {
    &quoted[quoted.find('>')? + 1..]
  } else {
    let name = rule.find(|c: char| !c.is_ascii_alphabetic())?;
    if name < 3 {
      return None;
    }
    &rule[name..]
  };
  let (sign, rest) = match rest.as_bytes().first()? {
    b'-' => (1, &rest[1..]),
    b'+' => (-1, &rest[1..]),
    _ => (-1, rest),
  };
  let end = rest
    .find(|c: char| !(c.is_ascii_digit() || c == ':'))
    .unwrap_or(rest.len());
  let mut parts = rest[..end].split(':');
  let hours: i64 = parts.next()?.parse().ok()?;
  let minutes: i64 = parts.next().map_or(Some(0), |m| m.parse().ok())?;
  let seconds: i64 = parts.next().map_or(Some(0), |s| s.parse().ok())?;
  Some(sign * (hours * 3600 + minutes * 60 + seconds))
}

/// This process's UTC offset at `unix_secs` (seconds east), from `TZ` or `/etc/localtime`.
pub fn local_offset(unix_secs: i64) -> i64 {
  let tz = std::env::var("TZ").ok();
  let path = match tz.as_deref() {
    None => "/etc/localtime".to_owned(),
    Some("") => return 0,
    Some(tz) => {
      let name = tz.strip_prefix(':').unwrap_or(tz);
      if name.starts_with('/') {
        name.to_owned()
      } else if let Some(offset) = posix_offset(name).filter(|_| !name.contains('/')) {
        // A name such as `CST-8` is a rule, not a zone (the database's `EST5EDT` reads the
        // same either way).
        return offset;
      } else {
        format!("/usr/share/zoneinfo/{name}")
      }
    }
  };
  std::fs::read(path)
    .ok()
    .and_then(|bytes| tzif_offset(&bytes, unix_secs))
    .unwrap_or(0)
}

/// The clock as the system row shows it: `HH:MM` now, turning on each minute.
pub fn clock() -> Memo<SharedString> {
  let tick = signal(0u64);
  effect(move || {
    tick.get();
    // A timer per minute: the next run of the effect replaces this one.
    set_timeout(until_next_minute(nsh_core::time::unix_ms()), move || {
      tick.update(|t| *t = t.wrapping_add(1));
    });
  });
  computed(move || {
    tick.get();
    let secs = (nsh_core::time::unix_ms() / 1000.0).floor() as i64;
    SharedString::from(hh_mm(secs, local_offset(secs)))
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A version-2 TZif file with one zone type per `(offset, transition)`, the 64-bit block
  /// holding the transitions and `footer` after it.
  fn tzif(transitions: &[(i64, i32)], footer: &str) -> Vec<u8> {
    let header = |time: usize, types: usize, chars: usize| {
      let mut h = b"TZif2".to_vec();
      h.extend([0u8; 15]);
      for count in [0, 0, 0, time, types, chars] {
        h.extend((count as i32).to_be_bytes());
      }
      h
    };
    // An empty first block (as "slim" files have): one type, no transitions.
    let mut bytes = header(0, 1, 4);
    bytes.extend(0i32.to_be_bytes());
    bytes.extend([0, 0]);
    bytes.extend(b"UTC\0");
    bytes.extend(header(transitions.len(), transitions.len(), 4));
    for (at, _) in transitions {
      bytes.extend(at.to_be_bytes());
    }
    for i in 0..transitions.len() {
      bytes.push(i as u8);
    }
    for (_, offset) in transitions {
      bytes.extend(offset.to_be_bytes());
      bytes.extend([0, 0]);
    }
    bytes.extend(b"XYZ\0");
    bytes.extend(format!("\n{footer}\n").as_bytes());
    bytes
  }

  #[test]
  fn the_clock_reads_hours_and_minutes() {
    // 2026-11-22 13:04:30 UTC.
    let at = 1_795_352_670;
    assert_eq!(hh_mm(at, 0), "13:04");
    assert_eq!(hh_mm(at, 8 * 3600), "21:04");
    assert_eq!(hh_mm(at, -14 * 3600), "23:04");
    assert_eq!(
      until_next_minute(60_000.0 * 7.0 + 59_500.0).as_millis(),
      500
    );
    assert_eq!(until_next_minute(120_000.0).as_millis(), 60_000);
  }

  #[test]
  fn posix_rules_give_their_standard_offset() {
    assert_eq!(posix_offset("CST-8"), Some(8 * 3600));
    assert_eq!(posix_offset("<+0530>-5:30"), Some(5 * 3600 + 1800));
    assert_eq!(posix_offset("EST5EDT,M3.2.0,M11.1.0"), Some(-5 * 3600));
    assert_eq!(posix_offset("UTC0"), Some(0));
    assert_eq!(posix_offset("Europe/Berlin"), None);
    assert_eq!(posix_offset("GMT"), None);
  }

  #[test]
  fn tzif_files_give_the_offset_in_force() {
    let file = tzif(
      &[(0, 3600), (1_000, 7200), (2_000, 3600)],
      "CET-1CEST,M3.5.0,M10.5.0/3",
    );
    assert_eq!(tzif_offset(&file, 500), Some(3600));
    assert_eq!(tzif_offset(&file, 1_500), Some(7200));
    // Past the last transition: the footer's standard offset.
    assert_eq!(tzif_offset(&file, 5_000), Some(3600));
    let shanghai = tzif(&[(0, 8 * 3600)], "CST-8");
    assert_eq!(tzif_offset(&shanghai, 1_795_352_670), Some(8 * 3600));
    assert_eq!(tzif_offset(b"not a zone", 0), None);
  }
}
