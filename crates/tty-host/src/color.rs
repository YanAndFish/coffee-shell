//! The colour depth a session draws with, and why.
//!
//! In order, the first rule that applies decides:
//!
//! | # | When                                                     | Depth                   |
//! | - | -------------------------------------------------------- | ----------------------- |
//! | 1 | the forced mode (`HostOptions::color_mode`) is `truecolor`/`24bit`, `256`, `16`, `mono`/`none` | that one |
//! | 2 | `NO_COLOR` is set and not empty                          | mono                    |
//! | 3 | `COLORTERM` is `truecolor` or `24bit`                    | 24-bit                  |
//! | 4 | a terminal brand known for 24-bit colour ([`brand`])     | 24-bit                  |
//! | 5 | `TERM` contains `256color`                               | 256, or 24-bit when the terminal answers XTGETTCAP `RGB`/`Tc` |
//! | 6 | `TERM` is `dumb`                                         | mono                    |
//! | 7 | anything else                                            | 16                      |
//!
//! Over SSH only `TERM` (and `COLORTERM` when the client sends it) arrives, so rule 4
//! reads the brands that live in `TERM` itself (`xterm-kitty`, `xterm-ghostty`, `foot`,
//! `alacritty`, `wezterm`, `rio`, any `*-direct`); `TERM_PROGRAM` and the like count only
//! when the program runs in a local terminal. Rule 5 asks the terminal at startup, inside
//! the query that already ends with DA1: an answer can only raise 256 colours to 24-bit,
//! silence (most terminals) keeps 256, and no other rule ever asks.

use nsh_platform_tui::ColorDepth;

/// Which rule decided the depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
  /// The forced mode the caller gave.
  Forced,
  /// `NO_COLOR`.
  NoColor,
  /// `COLORTERM`.
  ColorTerm,
  /// A terminal brand known for 24-bit colour.
  Brand,
  /// `TERM`'s colour count; for `256color` the terminal may raise it.
  Term,
  /// The terminal said it draws 24-bit colour (XTGETTCAP `RGB` or `Tc`).
  Answer,
}

/// The depth and the rule behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
  pub depth: ColorDepth,
  pub rule: Rule,
}

impl Choice {
  /// The depth `color_mode` (a mode the server forces) and the environment `var` reads
  /// give, before asking the terminal anything.
  pub fn from_env(color_mode: Option<&str>, var: impl Fn(&str) -> Option<String>) -> Self {
    let choice = |depth, rule| Self { depth, rule };
    match color_mode.map(str::to_ascii_lowercase).as_deref() {
      Some("truecolor" | "24bit") => return choice(ColorDepth::TrueColor, Rule::Forced),
      Some("256") => return choice(ColorDepth::Ansi256, Rule::Forced),
      Some("16") => return choice(ColorDepth::Ansi16, Rule::Forced),
      Some("mono" | "none") => return choice(ColorDepth::Monochrome, Rule::Forced),
      _ => {}
    }
    let lower = |name: &str| var(name).map(|value| value.to_ascii_lowercase());
    let term = lower("TERM").unwrap_or_default();
    if var("NO_COLOR").is_some_and(|value| !value.is_empty()) {
      choice(ColorDepth::Monochrome, Rule::NoColor)
    } else if lower("COLORTERM").is_some_and(|c| c == "truecolor" || c == "24bit") {
      choice(ColorDepth::TrueColor, Rule::ColorTerm)
    } else if brand(&term, &var) {
      choice(ColorDepth::TrueColor, Rule::Brand)
    } else if term.contains("256color") {
      choice(ColorDepth::Ansi256, Rule::Term)
    } else if term == "dumb" {
      choice(ColorDepth::Monochrome, Rule::Term)
    } else {
      choice(ColorDepth::Ansi16, Rule::Term)
    }
  }

  /// Whether to ask the terminal for 24-bit colour: only when its answer can change the
  /// depth, and only of a terminal that claims 256 colours (a modern emulator, which
  /// skips a device control string it does not know instead of printing it).
  pub fn asks_terminal(self) -> bool {
    self.rule == Rule::Term && self.depth == ColorDepth::Ansi256
  }

  /// The choice once the terminal answered (`truecolor`: it reported `RGB` or `Tc`).
  pub fn answered(self, truecolor: bool) -> Self {
    if self.asks_terminal() && truecolor {
      Self {
        depth: ColorDepth::TrueColor,
        rule: Rule::Answer,
      }
    } else {
      self
    }
  }
}

/// Whether `term` (lower case) or a local terminal's own variables name a terminal known
/// to draw 24-bit colour (as nsh-tui-host's `truecolor_brand` reads them, unchanged from 0.6.1 to 0.7.1).
pub fn brand(term: &str, var: &impl Fn(&str) -> Option<String>) -> bool {
  const IN_TERM: [&str; 6] = ["kitty", "ghostty", "alacritty", "foot", "wezterm", "rio"];
  const PROGRAMS: [&str; 7] = [
    "iterm.app",
    "ghostty",
    "wezterm",
    "vscode",
    "warpterminal",
    "rio",
    "alacritty",
  ];
  const MARKERS: [&str; 5] = [
    "KITTY_WINDOW_ID",
    "ALACRITTY_WINDOW_ID",
    "WEZTERM_EXECUTABLE",
    "GHOSTTY_RESOURCES_DIR",
    "WT_SESSION",
  ];
  IN_TERM.iter().any(|brand| term.contains(brand))
    || term.ends_with("-direct")
    || var("TERM_PROGRAM")
      .is_some_and(|program| PROGRAMS.contains(&program.to_ascii_lowercase().as_str()))
    || MARKERS
      .iter()
      .any(|name| var(name).is_some_and(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name: &str| {
      pairs
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| (*value).to_owned())
    }
  }

  /// The table in the module documentation, row by row, with and without an answer.
  #[test]
  fn the_decision_table() {
    use ColorDepth::{Ansi16, Ansi256, Monochrome, TrueColor};
    type Row = (
      Option<&'static str>,
      &'static [(&'static str, &'static str)],
      ColorDepth,
      Rule,
      ColorDepth,
    );
    // (forced mode, environment, depth, rule, depth after a yes from the terminal)
    let rows: &[Row] = &[
      // 1: the server's override beats everything, an answer included.
      (
        Some("256"),
        &[("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")],
        Ansi256,
        Rule::Forced,
        Ansi256,
      ),
      (
        Some("TrueColor"),
        &[("NO_COLOR", "1")],
        TrueColor,
        Rule::Forced,
        TrueColor,
      ),
      (
        Some("16"),
        &[("TERM", "xterm-256color")],
        Ansi16,
        Rule::Forced,
        Ansi16,
      ),
      (
        Some("mono"),
        &[("TERM", "xterm-256color")],
        Monochrome,
        Rule::Forced,
        Monochrome,
      ),
      // 2: NO_COLOR beats COLORTERM and brands.
      (
        None,
        &[
          ("NO_COLOR", "1"),
          ("COLORTERM", "truecolor"),
          ("TERM", "xterm-kitty"),
        ],
        Monochrome,
        Rule::NoColor,
        Monochrome,
      ),
      // An empty NO_COLOR is no NO_COLOR; an unknown forced mode is ignored.
      (
        Some("bogus"),
        &[("NO_COLOR", ""), ("TERM", "xterm-256color")],
        Ansi256,
        Rule::Term,
        TrueColor,
      ),
      // 3: COLORTERM.
      (
        None,
        &[("COLORTERM", "24bit"), ("TERM", "xterm")],
        TrueColor,
        Rule::ColorTerm,
        TrueColor,
      ),
      // 4: brands in TERM (they survive SSH) and local markers.
      (
        None,
        &[("TERM", "xterm-kitty")],
        TrueColor,
        Rule::Brand,
        TrueColor,
      ),
      (
        None,
        &[("TERM", "xterm-ghostty")],
        TrueColor,
        Rule::Brand,
        TrueColor,
      ),
      (None, &[("TERM", "foot")], TrueColor, Rule::Brand, TrueColor),
      (
        None,
        &[("TERM", "alacritty")],
        TrueColor,
        Rule::Brand,
        TrueColor,
      ),
      (
        None,
        &[("TERM", "wezterm")],
        TrueColor,
        Rule::Brand,
        TrueColor,
      ),
      (None, &[("TERM", "rio")], TrueColor, Rule::Brand, TrueColor),
      (
        None,
        &[("TERM", "xterm-direct")],
        TrueColor,
        Rule::Brand,
        TrueColor,
      ),
      (
        None,
        &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "iTerm.app")],
        TrueColor,
        Rule::Brand,
        TrueColor,
      ),
      (
        None,
        &[("TERM", "tmux-256color"), ("KITTY_WINDOW_ID", "1")],
        TrueColor,
        Rule::Brand,
        TrueColor,
      ),
      // 5: 256 colours, raised only by an answer.
      (
        None,
        &[("TERM", "xterm-256color")],
        Ansi256,
        Rule::Term,
        TrueColor,
      ),
      (
        None,
        &[("TERM", "xterm-256color"), ("COLORTERM", "yes")],
        Ansi256,
        Rule::Term,
        TrueColor,
      ),
      (
        None,
        &[
          ("TERM", "xterm-256color"),
          ("TERM_PROGRAM", "Apple_Terminal"),
        ],
        Ansi256,
        Rule::Term,
        TrueColor,
      ),
      // 6, 7: never asked.
      (
        None,
        &[("TERM", "dumb")],
        Monochrome,
        Rule::Term,
        Monochrome,
      ),
      (None, &[("TERM", "xterm")], Ansi16, Rule::Term, Ansi16),
      (None, &[("TERM", "linux")], Ansi16, Rule::Term, Ansi16),
      (None, &[], Ansi16, Rule::Term, Ansi16),
    ];
    for &(mode, vars, depth, rule, answered) in rows {
      let choice = Choice::from_env(mode, env(vars));
      assert_eq!(
        (choice.depth, choice.rule),
        (depth, rule),
        "{mode:?} {vars:?}"
      );
      assert_eq!(
        choice.asks_terminal(),
        rule == Rule::Term && depth == Ansi256,
        "{mode:?} {vars:?}"
      );
      assert_eq!(choice.answered(true).depth, answered, "{mode:?} {vars:?}");
      // Silence never changes anything.
      assert_eq!(choice.answered(false), choice, "{mode:?} {vars:?}");
    }
  }

  #[test]
  fn an_answer_is_recorded_as_the_reason() {
    let choice = Choice::from_env(None, env(&[("TERM", "xterm-256color")])).answered(true);
    assert_eq!(
      choice,
      Choice {
        depth: ColorDepth::TrueColor,
        rule: Rule::Answer
      }
    );
  }
}
