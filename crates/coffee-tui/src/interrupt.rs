//! Ctrl+C: press twice to quit.
//!
//! nsh's terminal key scheme makes Ctrl+C copy a selection, else quit at once. Over SSH
//! one stray Ctrl+C would then drop the whole session (and a guest's cart). The host
//! keeps Ctrl+C from the app: the first press arms a short window and shows a hint on
//! the bottom row, a second press within it quits. See `docs/adr/0001-ssh-session-model.md`.

use std::time::{Duration, Instant};

use ratatui_core::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Modifier, Style},
};

/// How long the first Ctrl+C waits for the second.
pub const WINDOW: Duration = Duration::from_secs(2);

/// The hint shown while a Ctrl+C waits for the second.
pub const HINT: &str = " 再按一次 Ctrl+C 退出 ";

/// What a Ctrl+C press does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
  /// The first press: show the hint and wait for a second.
  Armed,
  /// The second press within the window: quit.
  Quit,
}

/// The double-press state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Interrupt {
  armed_until: Option<Instant>,
}

impl Interrupt {
  /// A Ctrl+C press at `now`.
  pub fn press(&mut self, now: Instant) -> Press {
    match self.armed_until {
      Some(until) if now < until => {
        self.armed_until = None;
        Press::Quit
      }
      _ => {
        self.armed_until = Some(now + WINDOW);
        Press::Armed
      }
    }
  }

  /// Whether the hint shows at `now`.
  pub fn armed(&self, now: Instant) -> bool {
    self.armed_until.is_some_and(|until| now < until)
  }

  /// When the hint goes away, while it shows.
  pub fn deadline(&self) -> Option<Instant> {
    self.armed_until
  }

  /// Clears an expired window; `true` when the hint just went away (a redraw is due).
  pub fn expire(&mut self, now: Instant) -> bool {
    match self.armed_until {
      Some(until) if now >= until => {
        self.armed_until = None;
        true
      }
      _ => false,
    }
  }

  /// Any other key disarms: Ctrl+C, then typing, then Ctrl+C asks again.
  pub fn disarm(&mut self) -> bool {
    self.armed_until.take().is_some()
  }
}

/// Draws [`HINT`] centred on the bottom row of `area`, reversed so it stands out in
/// every colour depth (in pinned colours: amber with the dark ink).
pub fn draw_hint(buffer: &mut Buffer, area: Rect, accent: Color) {
  if area.height == 0 || area.width == 0 {
    return;
  }
  let width = unicode_width(HINT).min(area.width);
  let x = area.x + (area.width - width) / 2;
  let y = area.y + area.height - 1;
  let style = Style::new()
    .fg(accent)
    .bg(Color::Reset)
    .add_modifier(Modifier::REVERSED | Modifier::BOLD);
  buffer.set_stringn(x, y, HINT, usize::from(width), style);
}

/// Display width of `text`: CJK characters take two cells.
fn unicode_width(text: &str) -> u16 {
  let mut buffer = Buffer::empty(Rect::new(0, 0, 200, 1));
  let (end, _) = buffer.set_stringn(0, 0, text, usize::MAX, Style::new());
  end
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn two_presses_within_the_window_quit() {
    let start = Instant::now();
    let mut interrupt = Interrupt::default();
    assert_eq!(interrupt.press(start), Press::Armed);
    assert!(interrupt.armed(start + Duration::from_millis(500)));
    assert_eq!(
      interrupt.press(start + Duration::from_millis(500)),
      Press::Quit
    );
  }

  #[test]
  fn a_late_second_press_arms_again() {
    let start = Instant::now();
    let mut interrupt = Interrupt::default();
    interrupt.press(start);
    let late = start + WINDOW + Duration::from_millis(1);
    assert!(!interrupt.armed(late));
    assert!(interrupt.expire(late), "the hint goes away");
    assert!(!interrupt.expire(late), "once");
    assert_eq!(interrupt.press(late), Press::Armed);
  }

  #[test]
  fn other_keys_disarm() {
    let start = Instant::now();
    let mut interrupt = Interrupt::default();
    interrupt.press(start);
    assert!(interrupt.disarm());
    assert_eq!(interrupt.press(start), Press::Armed);
  }

  #[test]
  fn the_hint_sits_centred_on_the_last_row() {
    let area = Rect::new(0, 0, 40, 5);
    let mut buffer = Buffer::empty(area);
    draw_hint(&mut buffer, area, Color::Yellow);
    let row: String = (0..40)
      .map(|x| buffer[(x, 4)].symbol().to_string())
      .collect();
    let squeezed: String = row.chars().filter(|c| *c != ' ').collect();
    assert!(squeezed.contains("再按一次Ctrl+C退出"), "{row:?}");
    assert!(buffer[(20, 4)].modifier.contains(Modifier::REVERSED));
    // A narrow terminal clips the hint instead of panicking.
    let tiny = Rect::new(0, 0, 6, 1);
    let mut buffer = Buffer::empty(tiny);
    draw_hint(&mut buffer, tiny, Color::Yellow);
  }
}
