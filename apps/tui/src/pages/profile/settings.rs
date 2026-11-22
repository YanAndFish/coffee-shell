//! `设置 SETTINGS`: `界面动画 m` and `鼠标追踪 o` with their notes and `[ ■ 开 ]` /
//! `[ □ 关 ]` switches. A member's are saved on the account, a guest's live in this session
//! (`SettingsStore`); a toggle shows its toast (`actions::toggle_setting`).

use nsh_core::prelude::*;

use super::{geometry::Geometry, section};
use crate::{
  commands::press,
  stores::{Stores, profile::ProfileFocus},
  ui::{Tone, label, layout::vspace, switch, toned_strong},
};

/// One setting: its title, key, note, command and focus target.
struct Item {
  title: &'static str,
  key: &'static str,
  note: &'static str,
  command: &'static str,
  focus: ProfileFocus,
  on: fn(&Stores) -> bool,
}

const ITEMS: [Item; 2] = [
  Item {
    title: "界面动画",
    key: "m",
    note: "蒸汽、加载指示与进度条",
    command: "profile.animations",
    focus: ProfileFocus::Animations,
    on: |stores| stores.settings.animations(),
  },
  Item {
    title: "鼠标追踪",
    key: "o",
    note: "悬停即移动选择高亮",
    command: "profile.mouse",
    focus: ProfileFocus::MouseTracking,
    on: |stores| stores.settings.mouse_tracking(),
  },
];

/// A setting's row: the focus mark, `title key` (bold while focused), the note on the same
/// line when `inline`, and the switch at the right end.
fn setting_row(stores: Stores, item: &Item, inline: bool) -> View {
  let profile = stores.profile;
  let target = item.focus;
  let on = item.on;
  let command = item.command;
  let focused = move || profile.is_focused(target);
  view()
    .flex_row()
    .child(
      view()
        .absolute()
        .top(px(0.0))
        .left(px(-crate::ui::layout::CELL_WIDTH))
        .child(dynamic(move || {
          focused().then(|| toned_strong("›", || Tone::Acc, || true))
        })),
    )
    .child(
      text("")
        .child(toned_strong(item.title, || Tone::Fg, focused))
        .child(label(format!(" {}", item.key), Tone::Mute))
        .child(inline.then(|| label(format!("  {}", item.note), Tone::Mute))),
    )
    .child(view().flex_1())
    .child(switch(move || on(&stores), move || press(&stores, command)))
}

/// The settings panel.
pub fn settings_panel(stores: Stores, geo: Geometry) -> View {
  let inline = geo.one_column;
  let rows = ITEMS.iter().enumerate().map(|(index, item)| {
    view()
      .flex_none()
      .child((index > 0 && !inline).then(|| vspace(1)))
      .child(setting_row(stores, item, inline))
      .child((!inline).then(|| label(item.note, Tone::Mute)))
  });
  section(
    "设置 SETTINGS",
    geo,
    view()
      .flex_none()
      .child(geo.roomy.then(|| vspace(1)))
      .children(rows.collect::<Vec<_>>())
      .child(geo.roomy.then(|| vspace(1))),
  )
}
