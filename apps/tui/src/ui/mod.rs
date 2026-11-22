//! The app's styled terminal components: the only layer that imports `nsh_base` or reads
//! theme tokens. Pages and the shell compose these and never pick a colour themselves.
//!
//! Each component follows its spec in `docs/design/components.md` (the design system's
//! `raw/design-system/components/<Name>/README.md` translated to cells): a state is a glyph
//! plus an attribute (bold, underline, reverse, strike-through), never a colour alone, and
//! every colour is a token from [`theme`] (which explains the carriers the host's palette pin
//! turns back into the design's exact colours). Presses are mouse conveniences: every
//! pressable here is left out of the Tab order (`focusable(false)`) because every action has
//! a key in the command table, and a focused control would take Enter and Space from those
//! keys. Interactive components take their state, key, label and handler from the caller.
//!
//! The gallery ([`gallery::UiGallery`]) shows every component in every variant and state.

pub mod action;
pub mod address_card;
pub mod art;
pub mod button;
pub mod chip;
pub mod data_row;
pub mod dialog;
pub mod empty_state;
pub mod error_state;
pub mod frame;
pub mod gallery;
pub mod input;
pub mod kbd;
pub mod label;
pub mod layout;
pub mod leader;
pub mod loading;
pub mod meter;
pub mod nav_tabs;
pub mod panel;
pub mod radar;
pub mod stack;
pub mod status_bar;
pub mod stepper;
pub mod tag;
pub mod text;
pub mod theme;
pub mod toast;
pub mod toggle;
pub mod window;
pub mod wordmark;

pub use action::{action, follow_pointer, pressable, track_hover};
pub use address_card::{AddressView, add_card, address_card, mask_phone};
pub use art::{EMPTY_SET, cup, empty_cup, steam};
pub use button::{Button, ButtonKind, button};
pub use chip::{ChipPad, chip, group_label, section_head, segmented, sku_option};
pub use data_row::{
  Align, CartActions, CartForm, CartLine, Column, OrderStatus, Width, cart_row, data_row,
  name_cell, order_row, pips, table_head, table_rule,
};
pub use dialog::{ConfirmText, DialogFocus, DialogSize, confirm, dialog, dialog_box};
pub use empty_state::{EmptyAction, EmptyArt, empty_state, empty_state_with};
pub use error_state::{error_line, error_state};
pub use frame::{FrameStyle, Legend, frame};
pub use gallery::UiGallery;
pub use input::{InputForm, InputSpec, LabelPlace, input};
pub use kbd::{kbd, kbd_on_fill, kbd_wide};
pub use label::{label, strong, toned, toned_strong};
pub use layout::{Bp, Layout, Viewport};
pub use leader::{leader, receipt_leader};
pub use loading::{loading, loading_live, spinner};
pub use meter::{bar, meter, progress, thin_bar};
pub use nav_tabs::{NavTab, nav_rule, nav_tabs, nav_tabs_with};
pub use panel::{card, heading, inset, panel, panel_plain, panel_toned, rule};
pub use radar::{radar, radar_fits, radar_label};
pub use stack::{bottom_margin, content_cols, content_rows, page, page_stack, top_margin};
pub use status_bar::{Hint, Mode, StatusRight, mode_line, status_bar};
pub use stepper::{StepperForm, stepper, stock_note};
pub use tag::{TagKind, count_badge, pill, pill_text, tag};
pub use theme::{Fill, Tone};
pub use toast::{ToastKind, toast, toast_box, toast_line};
pub use toggle::{checkbox, radio, switch, switch_row, toggle_note};
pub use window::{DEMO_NOTICE, content_margins, panel_gap, system_row};
pub use wordmark::{wordmark, wordmark_block, wordmark_for, wordmark_line};

use nsh_core::prelude::*;

/// The app's root surface: the whole terminal, on the window colour, in the body text
/// colour. (The real host mounts the component gallery as its root instead of the app when
/// `COFFEE_UI_GALLERY=1` is set: see [`gallery::requested`].)
pub fn surface_root() -> View {
  view().flex_1().bg(theme::bg).text_color(theme::fg)
}
