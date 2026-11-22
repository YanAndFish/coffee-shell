//! The status bar: the mode now (an overlay's or the page's), the command table's hints for
//! the context now (fitted to the width), the demo notice and the help hint. Pressing a
//! hint runs its command.

use nsh_core::prelude::*;

use super::DEMO_NOTICE;
use crate::{
  commands::{self, HELP},
  stores::Stores,
  ui::{Hint, Layout, Mode, status_bar, text::display_width},
};

/// The `ui` mode of a command-table mode.
fn ui_mode(mode: commands::Mode) -> Mode {
  Mode {
    zh: mode.zh,
    en: mode.en,
  }
}

/// The `ui` hint of a command-table hint.
fn ui_hint(hint: commands::FooterHint) -> Hint {
  Hint {
    id: hint.id,
    key: hint.key,
    text: hint.text,
  }
}

/// The cells left for the page's hints in a bar `cols` wide showing `mode`: the mode
/// block, the hints' padding (a cell each side), the notice and the help hint with its
/// padding take the rest.
pub fn room(cols: usize, mode: commands::Mode) -> usize {
  let help = help_hint().width();
  cols.saturating_sub(ui_mode(mode).width() + 2 + display_width(DEMO_NOTICE) + help + 2)
}

fn help_hint() -> Hint {
  let help = commands::by_id(HELP).expect("the command table has the help command");
  Hint {
    id: help.id,
    key: help.key.glyph,
    text: help.text(),
  }
}

/// The status bar.
pub fn footer(stores: Stores, layout: Layout) -> impl IntoElement {
  status_bar(
    move || ui_mode(commands::mode(&stores)),
    move || {
      let viewport = layout.viewport();
      let ctx = commands::context(&stores);
      let room = room(usize::from(viewport.cols), commands::mode(&stores));
      commands::footer(&ctx, viewport.bp(), room, |command| {
        (command.enabled)(&stores)
      })
      .into_iter()
      .map(ui_hint)
      .collect()
    },
    DEMO_NOTICE,
    help_hint(),
    move |id| commands::press(&stores, id),
  )
}
