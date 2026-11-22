//! Wordmark, the header form (`components/Wordmark`): `◆ROAST/TTY`, the diamond in `acc`, the
//! slash in `mute`, the rest bold `fg`. It is also the button home. (The five-row block
//! form belongs to the login page, phase 2.)

use nsh_core::{element::AccessibilityRole, prelude::*};

use super::theme;

/// The header wordmark; a press runs `on_press` (home).
pub fn wordmark(on_press: impl Fn() + 'static) -> impl IntoElement {
  view()
    .role(AccessibilityRole::Button)
    .focusable(false)
    .flex_none()
    .label("ROAST/TTY 首页")
    .on_press(on_press)
    .child(
      text("")
        .child(text("◆").text_color(theme::acc))
        .child(
          text("ROAST")
            .text_color(theme::fg)
            .font_weight(FontWeight::BOLD),
        )
        .child(text("/").text_color(theme::mute))
        .child(
          text("TTY")
            .text_color(theme::fg)
            .font_weight(FontWeight::BOLD),
        ),
    )
}
