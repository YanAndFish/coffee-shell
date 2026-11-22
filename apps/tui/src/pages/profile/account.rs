//! `个人信息 ACCOUNT`: the avatar, the name, the 记住我 state of this connection's key, the
//! member tier and its meter, the three stats. Every value is the server's.

use coffee_api::{Profile, RememberState, RememberStatus};
use nsh_core::prelude::*;

use super::{
  geometry::{AccountForm, Geometry},
  section,
};
use crate::{
  commands::press,
  pages::format,
  stores::{Stores, remote::RemoteState},
  ui::{
    self, Fill, FrameStyle, Tone, bar, error_state, frame, inset, label,
    layout::{self, hspace, vspace},
    loading_live, pill_text, strong,
  },
};

/// The avatar's letter: the name's first character, a Latin letter upper-cased.
pub fn initial(name: &str) -> String {
  name
    .chars()
    .next()
    .map(|c| c.to_uppercase().collect())
    .unwrap_or_default()
}

/// The 记住我 line: `● 已记住当前公钥 · 至 12-22`, `○ 未记住当前公钥`, `○ 本次连接没有公钥`.
pub fn key_status(remembered: &RememberStatus) -> String {
  match remembered.state {
    RememberState::Remembered => match remembered.expires_on.as_deref() {
      Some(date) => format!("● 已记住当前公钥 · 至 {}", format::month_day(date)),
      None => "● 已记住当前公钥".to_owned(),
    },
    RememberState::NotRemembered => "○ 未记住当前公钥".to_owned(),
    RememberState::NoKey => "○ 本次连接没有公钥".to_owned(),
  }
}

fn key_line(remembered: &RememberStatus) -> Text {
  let tone = if remembered.state == RememberState::Remembered {
    Tone::Acc2
  } else {
    Tone::Mute
  };
  label(key_status(remembered), tone)
}

/// `熟豆会员 Lv.3`, `1,280 / 2,000 豆`, `再积 720 豆升级 Lv.4`.
pub fn tier_texts(profile: &Profile) -> (String, String, String) {
  let tier = &profile.user.tier;
  (
    format!("{} Lv.{}", tier.name, tier.level),
    format!(
      "{} / {} 豆",
      format::thousands(tier.points),
      format::thousands(tier.next_level_points)
    ),
    format!(
      "再积 {} 豆升级 Lv.{}",
      format::thousands(tier.points_to_next),
      tier.next_level
    ),
  )
}

/// `订单 3 · 地址 3 · 优惠券 1`.
pub fn stats_line(profile: &Profile) -> String {
  let stats = &profile.stats;
  format!(
    "订单 {} · 地址 {} · 优惠券 {}",
    stats.orders, stats.addresses, stats.coupons
  )
}

/// The 6×3 avatar block: a heavy `acc` frame around the bold initial.
fn avatar(name: &str) -> View {
  view().flex_none().w(layout::cols(6)).child(frame(
    || FrameStyle::Heavy,
    || Tone::Acc,
    None,
    Some(3),
    0,
    view()
      .flex_1()
      .items_center()
      .child(strong(initial(name), Tone::Fg)),
  ))
}

/// `▐林▌ 林晓`: the pill and the bold name.
fn pill_line(name: &str) -> View {
  view()
    .flex_row()
    .child(pill_text(Fill::Acc, initial(name), false))
    .child(hspace(1))
    .child(strong(name.to_owned(), Tone::Fg))
}

fn tier_rows(profile: &Profile, width: u16, stats_inline: bool) -> View {
  let (tier, points, next) = tier_texts(profile);
  let progress = profile.user.tier.progress as f32;
  let last = if stats_inline {
    view()
      .flex_row()
      .child(label(stats_line(profile), Tone::Soft))
      .child(view().flex_1())
      .child(label(next, Tone::Mute))
  } else {
    view().child(label(next, Tone::Mute))
  };
  view()
    .flex_none()
    .child(
      view()
        .flex_row()
        .child(strong(tier, Tone::Acc))
        .child(view().flex_1())
        .child(label(points, Tone::Mute)),
    )
    .child(bar(move || progress, 100.0, usize::from(width), Tone::Acc))
    .child(last)
}

/// The three stat tiles: number (bold) over label (`mute`), in inset frames a cell apart.
fn tiles(profile: &Profile) -> View {
  let stats = profile.stats;
  view().flex_row().gap(layout::gap_cols(1)).children(
    [
      ("订单", stats.orders),
      ("地址", stats.addresses),
      ("优惠券", stats.coupons),
    ]
    .into_iter()
    .map(|(name, count)| {
      view().flex_1().min_w(px(0.0)).child(inset(
        view()
          .items_center()
          .child(strong(count.to_string(), Tone::Fg))
          .child(label(name, Tone::Mute)),
      ))
    })
    .collect::<Vec<_>>(),
  )
}

fn body(profile: &Profile, geo: Geometry) -> View {
  let name = profile.user.username.as_str();
  let width = geo.left_inner();
  match geo.account {
    AccountForm::Full => {
      // The avatar is 6 wide; its gap to the name shrinks to keep the key line whole.
      let gap = if width >= 35 { 2 } else { 1 };
      view()
        .flex_none()
        .child(vspace(1))
        .child(
          view()
            .flex_row()
            .child(avatar(name))
            .child(hspace(gap))
            .child(
              view()
                .flex_1()
                .min_w(px(0.0))
                .child(strong(name.to_owned(), Tone::Fg))
                .child(key_line(&profile.remembered)),
            ),
        )
        .child(vspace(1))
        .child(tier_rows(profile, width, false))
        .child(vspace(1))
        .child(tiles(profile))
        .child(vspace(1))
    }
    AccountForm::Compact => view()
      .flex_none()
      .child(pill_line(name))
      .child(key_line(&profile.remembered))
      .child(tier_rows(profile, width, false))
      .child(tiles(profile)),
    AccountForm::Inline if geo.one_column => view()
      .flex_none()
      .child(
        view()
          .flex_row()
          .child(pill_line(name))
          .child(label(" · ", Tone::Mute))
          .child(key_line(&profile.remembered)),
      )
      .child(tier_rows(profile, width, true)),
    AccountForm::Inline => view()
      .flex_none()
      .child(pill_line(name))
      .child(key_line(&profile.remembered))
      .child(tier_rows(profile, width, true)),
  }
}

/// The account panel: the profile, or its loading line, or its error with `⏎ 重试`.
pub fn account_panel(stores: Stores, geo: Geometry) -> View {
  let profile = stores.profile;
  let settings = stores.settings;
  section(
    "个人信息 ACCOUNT",
    geo,
    dynamic(move || match profile.state() {
      RemoteState::Failed(failure, None) => view()
        .flex_none()
        .child(vspace(1))
        .child(error_state(
          "个人信息加载失败",
          failure.detail(),
          "⏎",
          "重试",
          move || press(&stores, "profile.retry"),
        ))
        .child(vspace(1))
        .into_element(),
      state => match state.data() {
        Some(data) => body(data, geo).into_element(),
        None => view()
          .flex_none()
          .child(vspace(1))
          .child(loading_live("正在载入…", move || {
            settings.animations()
          }))
          .child(vspace(1))
          .into_element(),
      },
    }),
  )
}

/// The logout button `[ q 退出登录 ]` (danger), as wide as its column: it only asks the
/// shell for the confirmation (U4).
pub fn logout_button(stores: Stores, geo: Geometry) -> ui::Button {
  let profile = stores.profile;
  ui::Button::new(ui::ButtonKind::Danger, "q", "退出登录")
    .width(geo.button())
    .focused(move || profile.is_focused(crate::stores::profile::ProfileFocus::Logout))
    .on_press(move || press(&stores, "profile.logout"))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn profile(json: &str) -> Profile {
    serde_json::from_str(json).unwrap()
  }

  #[test]
  fn the_account_reads_the_server() {
    let member = profile(include_str!(
      "../../../../../packages/contract/fixtures/v1/profile/profile.json"
    ));
    assert_eq!(initial(&member.user.username), "林");
    assert_eq!(initial("bean_lover"), "B");
    assert_eq!(initial(""), "");
    assert_eq!(
      key_status(&member.remembered),
      "● 已记住当前公钥 · 至 12-22"
    );
    assert_eq!(
      tier_texts(&member),
      (
        "熟豆会员 Lv.3".to_owned(),
        "1,280 / 2,000 豆".to_owned(),
        "再积 720 豆升级 Lv.4".to_owned()
      )
    );
    assert_eq!(stats_line(&member), "订单 4 · 地址 3 · 优惠券 1");
    let no_key = profile(include_str!(
      "../../../../../packages/contract/fixtures/v1/profile/profile-no-key.json"
    ));
    assert_eq!(key_status(&no_key.remembered), "○ 本次连接没有公钥");
    let new = profile(include_str!(
      "../../../../../packages/contract/fixtures/v1/profile/profile-new.json"
    ));
    assert_eq!(key_status(&new.remembered), "○ 未记住当前公钥");
  }
}
