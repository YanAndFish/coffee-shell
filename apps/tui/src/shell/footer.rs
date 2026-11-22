//! The status bar of the page on top: its mode, the command table's hints for it (fitted
//! to the width), the demo notice and the help hint. Pressing a hint runs its command.

use nsh_core::prelude::*;

use super::DEMO_NOTICE;
use crate::{
  commands::{self, HELP},
  stores::Stores,
  ui::{Hint, Layout, status_bar},
};

/// The status bar.
pub fn footer(stores: Stores, layout: Layout) -> impl IntoElement {
  let help: Hint = commands::by_id(HELP)
    .map(commands::Command::hint)
    .expect("the command table has the help command");
  status_bar(
    move || stores.router.top().mode(),
    move || {
      let top = stores.router.top();
      let viewport = layout.viewport();
      commands::footer(
        top.page(),
        viewport.bp(),
        usize::from(viewport.cols),
        top.mode(),
        DEMO_NOTICE,
        |command| (command.enabled)(&stores),
      )
    },
    DEMO_NOTICE,
    help,
    move |id| commands::press(&stores, id),
  )
}
