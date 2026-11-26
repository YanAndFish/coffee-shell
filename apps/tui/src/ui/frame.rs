//! Box frames drawn in glyphs: the design's four line styles, with an optional legend on
//! the top line.
//!
//! ```text
//! ╭─ 优惠码 ───╮   ┏━ 用户名 ━━━┓   ╔═ 快捷键 KEYMAP ═╗   ╭╌╌╌╌╌╌╌╌╌╮
//! │ TTY10     │   ┃ › 林晓      ┃   ║                 ║   ╎    +    ╎
//! ╰───────────╯   ┗━━━━━━━━━━━━━┛   ╚═════════════════╝   ╰╌╌╌╌╌╌╌╌╌╯
//! ```
//!
//! nsh (0.6.1, 0.7.1) draws a view's border itself only as a thin single line, only where its
//! terminal layout gives the border a cell (never on a small box), and only in a tone nsh
//! can tell apart. These frames are text instead, so they take any style (heavy for focus,
//! double for dialogs, dashed for the "add" card), any tone, and a legend inline. The
//! horizontal lines are long runs clipped by a flexible box (nsh never ends a run of line
//! glyphs with `…`); the sides are a column of glyphs: exact when the caller knows the
//! height, else a long column clipped to the content's height.
//!
//! A clipped run is drawn only inside its box, but nsh hit-tests the run's own (much wider
//! or taller) box: every run therefore takes no pointer events ([`overflowing`]), so a press
//! reaches what is under it — a dialog's buttons, the fields under a toast — and a pressable
//! frame still gets the presses on its own lines.

use nsh_core::{EllipsizeMode, prelude::*};

use super::{layout, text::display_width, theme::Tone};

/// A frame's line style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameStyle {
  /// `╭─╮ │ ╰─╯`: panels, inputs, toasts.
  Round,
  /// `┏━┓ ┃ ┗━┛`: the focused input, the current address card.
  Heavy,
  /// `╔═╗ ║ ╚═╝`: dialogs.
  Double,
  /// `╭╌╮ ╎ ╰╌╯`: the "add" placeholder card.
  Dashed,
}

/// The glyphs of a style: corners (top left, top right, bottom left, bottom right), the
/// horizontal line and the vertical line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyphs {
  /// `╭`.
  pub top_left: char,
  /// `╮`.
  pub top_right: char,
  /// `╰`.
  pub bottom_left: char,
  /// `╯`.
  pub bottom_right: char,
  /// `─`.
  pub across: char,
  /// `│`.
  pub down: char,
}

impl FrameStyle {
  /// Its glyphs.
  pub const fn glyphs(self) -> Glyphs {
    let (top_left, top_right, bottom_left, bottom_right, across, down) = match self {
      FrameStyle::Round => ('╭', '╮', '╰', '╯', '─', '│'),
      FrameStyle::Heavy => ('┏', '┓', '┗', '┛', '━', '┃'),
      FrameStyle::Double => ('╔', '╗', '╚', '╝', '═', '║'),
      FrameStyle::Dashed => ('╭', '╮', '╰', '╯', '╌', '╎'),
    };
    Glyphs {
      top_left,
      top_right,
      bottom_left,
      bottom_right,
      across,
      down,
    }
  }
}

/// The longest line a frame draws (the content is at most 200 columns wide).
const RUN: usize = 240;
/// The tallest side drawn when the height is not known.
const COLUMN: usize = 120;

/// A frame's top line for a box `width` cells wide with `legend` (as text, for tests and
/// fixed-size boxes): `╭─ 标题 ──╮`. The legend is cut with `…` to fit.
pub fn top_line(style: FrameStyle, legend: Option<&str>, width: usize) -> String {
  let g = style.glyphs();
  if width < 2 {
    return g.across.to_string().repeat(width);
  }
  let inner = width - 2;
  let mut out = String::new();
  out.push(g.top_left);
  match legend {
    Some(legend) if inner >= 5 => {
      let legend = super::text::truncate(legend, inner - 4);
      out.push(g.across);
      out.push(' ');
      out.push_str(&legend);
      out.push(' ');
      let used = 3 + display_width(&legend);
      out.extend(std::iter::repeat_n(g.across, inner - used));
    }
    _ => out.extend(std::iter::repeat_n(g.across, inner)),
  }
  out.push(g.top_right);
  out
}

/// A frame's bottom line `width` cells wide: `╰──╯`.
pub fn bottom_line(style: FrameStyle, width: usize) -> String {
  let g = style.glyphs();
  if width < 2 {
    return g.across.to_string().repeat(width);
  }
  let mut out = String::new();
  out.push(g.bottom_left);
  out.extend(std::iter::repeat_n(g.across, width - 2));
  out.push(g.bottom_right);
  out
}

/// The legend on a frame's top line.
pub struct Legend {
  /// Its text: "中文 ENGLISH" or "购物车 · 3 件".
  pub text: Reactive<SharedString>,
  /// Its tone (bold): `Acc`, or `Acc2` on a success receipt.
  pub tone: Tone,
}

impl Legend {
  /// A legend in the brand accent.
  pub fn new(text: impl IntoReactive<SharedString>) -> Self {
    Self {
      text: text.into_reactive(),
      tone: Tone::Acc,
    }
  }

  /// The same legend in `tone`.
  pub fn tone(self, tone: Tone) -> Self {
    Self { tone, ..self }
  }
}

/// The absolute box at the top left of a clipping parent that holds a run longer than the
/// parent: it takes no pointer events (the run's box reaches far outside what is drawn).
pub fn overflowing() -> View {
  view()
    .absolute()
    .top(px(0.0))
    .left(px(0.0))
    .pointer_events(PointerEvents::None)
}

/// Glyph text in the tone `tone()` gives now (with its carrier's mark).
fn line_text(content: impl IntoReactive<SharedString>, tone: impl Fn() -> Tone + 'static) -> Text {
  super::label::toned(content, tone)
}

/// A line of `glyph` across its parent (one row, clipped where the parent ends): a rule.
pub fn line_run(glyph: char, tone: Tone) -> View {
  view()
    .flex_none()
    .w_full()
    .h(layout::rows(1))
    .overflow_hidden()
    .child(
      overflowing().child(
        super::label::label(glyph.to_string().repeat(RUN), tone)
          .number_of_lines(1)
          .text_overflow(EllipsizeMode::Clip),
      ),
    )
}

/// One horizontal run, clipped by a flexible box: `first` then the style's line.
fn run(
  style: impl Fn() -> FrameStyle + 'static,
  first: impl Fn(Glyphs) -> String + 'static,
  tone: impl Fn() -> Tone + Copy + 'static,
) -> View {
  view()
    .flex_1()
    .min_w(px(0.0))
    .h(layout::rows(1))
    .overflow_hidden()
    .child(
      overflowing().child(
        line_text(
          move || {
            let g = style().glyphs();
            let mut out = first(g);
            out.extend(std::iter::repeat_n(g.across, RUN));
            SharedString::from(out)
          },
          tone,
        )
        .number_of_lines(1)
        .text_overflow(EllipsizeMode::Clip),
      ),
    )
}

/// One corner glyph.
fn corner(
  style: impl Fn() -> FrameStyle + 'static,
  pick: fn(Glyphs) -> char,
  tone: impl Fn() -> Tone + Copy + 'static,
) -> Text {
  line_text(
    move || SharedString::from(pick(style().glyphs()).to_string()),
    tone,
  )
}

/// A side: `rows` glyphs when the height is known, else a long column clipped to the
/// content's height.
fn side(
  style: impl Fn() -> FrameStyle + 'static,
  rows: Option<u16>,
  tone: impl Fn() -> Tone + Copy + 'static,
) -> View {
  let count = rows.map_or(COLUMN, usize::from);
  let column = line_text(
    move || {
      let down = style().glyphs().down;
      let lines: Vec<String> = std::iter::repeat_n(down.to_string(), count).collect();
      SharedString::from(lines.join("\n"))
    },
    tone,
  );
  let side = view().flex_none().w(layout::cols(1));
  match rows {
    Some(_) => side.child(column),
    // Stretched between its side's top and bottom, clipping the column there: what is
    // absolute takes no room, so the content alone sets the height.
    None => side.child(
      view()
        .absolute()
        .top(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .w(layout::cols(1))
        .overflow_hidden()
        .child(overflowing().child(column)),
    ),
  }
}

/// A box drawn in `style()` and `tone()` (both tracked: an input turns heavy and amber while
/// focused) around `content`, with an optional `legend` on its top line and `pad` blank
/// columns between each side line and the content. `rows` is the frame's whole height in
/// rows when the caller fixes it (a dialog, a card: the content is stretched to it); `None`
/// lets the content decide. The width comes from the caller (`.w(..)`, `.flex_1()`, ...).
pub fn frame(
  style: impl Fn() -> FrameStyle + Copy + 'static,
  tone: impl Fn() -> Tone + Copy + 'static,
  legend: Option<Legend>,
  rows: Option<u16>,
  pad: u16,
  content: impl IntoElement,
) -> View {
  let inner = rows.map(|rows| rows.saturating_sub(2));
  let top = match legend {
    Some(Legend {
      text,
      tone: legend_tone,
    }) => view()
      .flex_row()
      .h(layout::rows(1))
      .child(line_text(
        move || {
          let g = style().glyphs();
          SharedString::from(format!("{}{} ", g.top_left, g.across))
        },
        tone,
      ))
      .child(
        super::label::strong(text, legend_tone)
          .number_of_lines(1)
          .flex_shrink_0(),
      )
      .child(run(style, |_| " ".to_owned(), tone))
      .child(corner(style, |g| g.top_right, tone)),
    None => view()
      .flex_row()
      .h(layout::rows(1))
      .child(run(style, |g| g.top_left.to_string(), tone))
      .child(corner(style, |g| g.top_right, tone)),
  };
  let middle = view()
    .flex_row()
    .min_h(px(0.0))
    .child(side(style, inner, tone))
    .child((pad > 0).then(|| layout::hspace(pad)))
    .child(view().flex_1().min_w(px(0.0)).child(content))
    .child((pad > 0).then(|| layout::hspace(pad)))
    .child(side(style, inner, tone));
  let middle = match inner {
    Some(inner) => middle.h(layout::rows(inner)),
    None => middle,
  };
  let bottom = view()
    .flex_row()
    .h(layout::rows(1))
    .child(run(style, |g| g.bottom_left.to_string(), tone))
    .child(corner(style, |g| g.bottom_right, tone));
  let frame = view().flex_col().min_w(layout::cols(4));
  let frame = match rows {
    Some(rows) => frame.h(layout::rows(rows)),
    None => frame,
  };
  frame.child(top).child(middle).child(bottom)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lines_are_exactly_as_wide_as_the_box() {
    let top = top_line(FrameStyle::Double, Some("退出登录 LOGOUT"), 56);
    assert_eq!(display_width(&top), 56);
    assert!(top.starts_with("╔═ 退出登录 LOGOUT ═"));
    assert!(top.ends_with("═╗"));
    assert_eq!(bottom_line(FrameStyle::Double, 56).chars().count(), 56);
    assert_eq!(top_line(FrameStyle::Round, None, 4), "╭──╮");
    assert_eq!(
      top_line(FrameStyle::Heavy, Some("用户名"), 12),
      "┏━ 用户名 ━┓"
    );
    // A legend wider than the box is cut, never the frame.
    let cut = top_line(FrameStyle::Round, Some("一个很长很长很长的标题"), 12);
    assert_eq!(display_width(&cut), 12);
    assert!(cut.contains('…') && cut.ends_with('╮'));
    assert_eq!(bottom_line(FrameStyle::Dashed, 5), "╰╌╌╌╯");
  }

  #[test]
  fn every_style_has_its_own_glyphs() {
    let styles = [
      FrameStyle::Round,
      FrameStyle::Heavy,
      FrameStyle::Double,
      FrameStyle::Dashed,
    ];
    for (i, a) in styles.iter().enumerate() {
      for b in &styles[i + 1..] {
        assert_ne!(a.glyphs(), b.glyphs());
      }
    }
    assert_eq!(FrameStyle::Heavy.glyphs().down, '┃');
  }
}
