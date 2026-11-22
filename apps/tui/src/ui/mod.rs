//! The app's styled terminal components: the only layer that imports `nsh_base` or reads
//! theme tokens. Pages and the shell compose these and never pick a colour themselves.
//!
//! Each component follows its design-system README (`docs/design/raw/design-system/
//! components/<Name>/README.md`) translated to terminal cells: a state is a glyph plus an
//! attribute (bold, underline, dim), never a colour alone. Presses are mouse conveniences:
//! every pressable here is left out of the Tab order (`focusable(false)`) because every
//! action has a key in the command table, and a focused control would take Enter and Space
//! from those keys.

pub mod action;
pub mod data_row;
pub mod empty_state;
pub mod error_state;
pub mod kbd;
pub mod label;
pub mod layout;
pub mod loading;
pub mod nav_tabs;
pub mod panel;
pub mod stack;
pub mod status_bar;
pub mod text;
pub mod theme;
pub mod toast;
pub mod wordmark;

pub use action::action;
pub use data_row::{Align, Column, Width, data_row, table_head};
pub use empty_state::{EmptyAction, empty_state};
pub use error_state::error_state;
pub use kbd::kbd;
pub use label::{label, strong, toned};
pub use layout::{Bp, Layout, Viewport};
pub use loading::loading;
pub use nav_tabs::{NavTab, nav_tabs};
pub use panel::panel;
pub use stack::{page, page_stack};
pub use status_bar::{Hint, Mode, status_bar};
pub use theme::Tone;
pub use toast::{ToastKind, toast_line};
pub use wordmark::wordmark;

use nsh_core::prelude::*;

/// The app's root surface: the whole terminal, on the window colour, in the body text
/// colour.
pub fn surface_root() -> View {
  view().flex_1().bg(theme::bg).text_color(theme::fg)
}
