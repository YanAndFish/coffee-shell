//! StatusBar: the bottom row, a vim/tmux mode line (`components/StatusBar`):
//!
//! ```text
//! ▐ 选购 BROWSE ▌ ‹↑↓› 移动  ‹⏎› 详情  ‹a› 加入            ● 鼠标   购物车 ¥426.00   ‹?› 帮助
//! ```
//!
//! The mode block is a solid pill (`acc` fill, `ink`; the Chinese name bold), then the page's
//! key hints two cells apart (each pressable, as its key), then the right side: the mouse
//! tracking state (`●` `acc2` on, `○` `mute` off), the cart total, the help hint, three cells
//! apart and one from the edge. On S the right side keeps only the total; when the hints
//! leave fewer than two cells before it, it goes ([`right_fits`]).
//!
//! nsh draws the pill's text in the terminal's default colour; `coffee-host` pins default text
//! on an amber fill to `ink`, and nsh draws the fill reversed at 16 colours and in mono, so it
//! reads at every colour depth. The bar owns the bottom row: nsh's own toasts and search line
//! would draw there, so the app never uses them.

use nsh_core::prelude::*;

use super::{
  action::pressable,
  kbd::{kbd, kbd_width},
  label::{label, strong, toned},
  layout::{Bp, Layout},
  text::display_width,
  theme::{Fill, Tone},
};

/// The mode: `选购 BROWSE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
  /// Chinese name, `选购`.
  pub zh: &'static str,
  /// English name in capitals, `BROWSE`.
  pub en: &'static str,
}

impl Mode {
  /// The cells the mode block takes: `▐ 选购 BROWSE ▌` (half blocks and a blank each side).
  pub fn width(&self) -> usize {
    display_width(self.zh) + 1 + display_width(self.en) + 4
  }
}

/// One key hint: `↑↓ 移动`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hint {
  /// The command it runs when pressed.
  pub id: &'static str,
  /// The key cap's text, `↑↓`.
  pub key: &'static str,
  /// What it does, `移动`.
  pub text: &'static str,
}

impl Hint {
  /// The cells the hint takes: key cap (key plus a cell each side), a gap, the text.
  pub fn width(&self) -> usize {
    kbd_width(self.key) + 1 + display_width(self.text)
  }
}

/// The cells between two hints.
pub const HINT_GAP: usize = 2;
/// The cells between two groups of the right side.
pub const GROUP_GAP: usize = 3;

/// The right side of the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusRight {
  /// Mouse tracking on or off (`None`: not shown).
  pub mouse: Option<bool>,
  /// The cart total, already formatted (`¥426.00`); `None`: not shown.
  pub total: Option<SharedString>,
  /// The total without its `购物车` word (M when the row is short of room; always on S).
  pub bare_total: bool,
  /// The help hint (`‹?› 帮助`).
  pub help: Option<Hint>,
}

impl StatusRight {
  /// What the right side keeps at `bp`: everything from M up, only the total on S.
  pub fn at(&self, bp: Bp) -> Self {
    if bp <= Bp::S {
      Self {
        mouse: None,
        total: self.total.clone(),
        bare_total: true,
        help: None,
      }
    } else {
      self.clone()
    }
  }

  /// The cells it takes with its right margin (`cart` on S: just the amount).
  pub fn width(&self, bp: Bp) -> usize {
    let short = bp <= Bp::S || self.bare_total;
    let mut groups = Vec::new();
    if self.mouse.is_some() {
      groups.push(display_width("● 鼠标"));
    }
    if let Some(total) = &self.total {
      groups.push(
        display_width(total)
          + if short {
            0
          } else {
            display_width("购物车 ")
          },
      );
    }
    if let Some(help) = &self.help {
      groups.push(help.width());
    }
    let gaps = GROUP_GAP * groups.len().saturating_sub(1);
    groups.iter().sum::<usize>() + gaps + 1
  }
}

/// Whether the right side fits beside `mode` and `hints` in `cols` cells (two cells to spare).
pub fn right_fits(cols: usize, mode: Mode, hints: &[Hint], right: &StatusRight, bp: Bp) -> bool {
  let hints_width: usize =
    hints.iter().map(Hint::width).sum::<usize>() + HINT_GAP * hints.len().saturating_sub(1);
  mode.width() + 1 + hints_width + 2 + right.at(bp).width(bp) <= cols
}

/// The mode block: `▐ 选购 BROWSE ▌`.
pub fn mode_block(mode: impl Fn() -> Mode + 'static) -> View {
  let mode = computed(mode);
  super::tag::pill(
    Fill::Acc,
    text("")
      .child(strong(
        move || SharedString::from(format!(" {}", mode.get().zh)),
        Tone::Ink,
      ))
      .child(label(
        move || SharedString::from(format!(" {} ", mode.get().en)),
        Tone::Ink,
      )),
  )
}

fn hint_button(hint: Hint, on_hint: impl Fn(&'static str) + 'static) -> impl IntoElement {
  pressable()
    .flex_none()
    .flex_row()
    .items_center()
    .gap_2()
    .label(SharedString::from(format!("{} {}", hint.key, hint.text)))
    .on_press(move || on_hint(hint.id))
    .child(kbd(hint.key))
    .child(label(hint.text, Tone::Mute))
}

fn hints_row(
  hints: impl Fn() -> Vec<Hint> + 'static,
  on_hint: impl Fn(&'static str) + Copy + 'static,
) -> View {
  view()
    .flex_1()
    .min_w(px(0.0))
    .flex_row()
    .items_center()
    .gap(super::layout::gap_cols(2))
    .pl_2()
    .overflow_hidden()
    .child(each(
      hints,
      |hint| hint.id,
      move |hint, _| hint_button(hint.get_untracked(), on_hint),
    ))
}

/// The status bar of phase 1: `mode()`, the hints `hints()` (already fitted to the width),
/// the demo `notice` and the `help` hint; a press on a hint runs `on_hint(id)`. Its mode
/// block is the phase-1 one (` 选购 BROWSE ` on the fill, no half blocks); [`mode_line`] is
/// the design's bar.
pub fn status_bar(
  mode: impl Fn() -> Mode + 'static,
  hints: impl Fn() -> Vec<Hint> + 'static,
  notice: &'static str,
  help: Hint,
  on_hint: impl Fn(&'static str) + Copy + 'static,
) -> impl IntoElement {
  let mode = computed(mode);
  view()
    .flex_none()
    .flex_row()
    .items_center()
    .bg(Fill::Surface.color())
    .child(
      view()
        .flex_none()
        .px_2()
        .bg(Fill::Acc.color())
        .child(strong(
          move || {
            let mode = mode.get();
            SharedString::from(format!("{} {}", mode.zh, mode.en))
          },
          Tone::Ink,
        )),
    )
    .child(hints_row(hints, on_hint))
    .child(label(notice, Tone::Soft).flex_none())
    .child(view().flex_none().px_2().child(hint_button(help, on_hint)))
}

/// The mode line of the design: `mode()`, the hints `hints()` (already fitted to the width
/// class), and the right side `right()` (cut to what the width class keeps, and hidden when
/// the hints leave it less than two cells). A press on a hint runs `on_hint(id)`.
pub fn mode_line(
  mode: impl Fn() -> Mode + Copy + 'static,
  hints: impl Fn() -> Vec<Hint> + Copy + 'static,
  right: impl Fn() -> StatusRight + Copy + 'static,
  on_hint: impl Fn(&'static str) + Copy + 'static,
) -> View {
  let layout = Layout::get();
  let shown = move || {
    let viewport = layout.viewport();
    let bp = viewport.bp();
    let right = right();
    right_fits(usize::from(viewport.cols), mode(), &hints(), &right, bp).then(|| right.at(bp))
  };
  view()
    .flex_none()
    .flex_row()
    .items_center()
    .bg(Fill::Surface.color())
    .child(mode_block(mode))
    .child(hints_row(hints, on_hint))
    .child(dynamic(move || {
      shown().map(|right| {
        let short = layout.bp() <= Bp::S || right.bare_total;
        let mut groups: Vec<AnyElement> = Vec::new();
        if let Some(on) = right.mouse {
          groups.push(
            text("")
              .child(toned(if on { "● " } else { "○ " }, move || {
                if on { Tone::Acc2 } else { Tone::Mute }
              }))
              .child(label("鼠标", Tone::Mute))
              .into_element(),
          );
        }
        if let Some(total) = right.total {
          groups.push(
            text("")
              .child((!short).then(|| label("购物车 ", Tone::Mute)))
              .child(label(total, Tone::Fg))
              .into_element(),
          );
        }
        if let Some(help) = right.help {
          groups.push(hint_button(help, on_hint).into_element());
        }
        // Groups three cells apart (wider than any gap nsh lays out), one from the edge.
        let mut row = view().flex_none().flex_row();
        for (index, group) in groups.into_iter().enumerate() {
          if index > 0 {
            row = row.child(super::layout::hspace(GROUP_GAP as u16));
          }
          row = row.child(group);
        }
        row.child(super::layout::hspace(1))
      })
    }))
}

#[cfg(test)]
mod tests {
  use super::*;

  const BROWSE: Mode = Mode {
    zh: "选购",
    en: "BROWSE",
  };

  fn hint(key: &'static str, text: &'static str) -> Hint {
    Hint { id: "x", key, text }
  }

  #[test]
  fn widths_count_cells() {
    assert_eq!(BROWSE.width(), display_width("▐ 选购 BROWSE ▌"));
    assert_eq!(hint("↑↓", "移动").width(), display_width("‹↑↓› 移动"));
  }

  #[test]
  fn the_right_side_follows_the_design() {
    let right = StatusRight {
      mouse: Some(true),
      total: Some("¥426.00".into()),
      bare_total: false,
      help: Some(hint("?", "帮助")),
    };
    // `● 鼠标   购物车 ¥426.00   ‹?› 帮助 `
    assert_eq!(
      right.width(Bp::Xl),
      display_width("● 鼠标   购物车 ¥426.00   ‹?› 帮助 ")
    );
    assert_eq!(right.at(Bp::S).width(Bp::S), display_width("¥426.00 "));
    let hints = [
      hint("↑↓", "移动"),
      hint("⏎", "详情"),
      hint("a", "加入"),
      hint("f", "产区"),
    ];
    assert!(right_fits(80, BROWSE, &hints, &right, Bp::S));
    assert!(!right_fits(40, BROWSE, &hints, &right, Bp::S));
  }
}
