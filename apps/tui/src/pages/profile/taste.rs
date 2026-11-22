//! `我的口味画像 TASTE`: the taste radar in mint (six flavour meters on short or narrow
//! terminals), `偏好` / `常买` / `少碰`, and `按画像推荐` with a button per recommended product.
//! A member without orders gets the empty rim and `下单后生成你的口味画像`.

use coffee_api::{FLAVOR_AXES, ProductRef, Profile};
use nsh_core::prelude::*;

use super::{commands::open_recommendation, geometry::Geometry, section};
use crate::{
  stores::{Stores, profile::ProfileFocus},
  ui::{
    Button, ButtonKind, Tone, label,
    layout::{hspace, vspace},
    radar,
    text::{pad_end, truncate},
  },
};

/// The radar's screen-reader label: `我的口味画像：酸度 8，甜感 7，…`.
pub fn taste_label(flavor: [u8; 6]) -> String {
  let parts: Vec<String> = FLAVOR_AXES
    .iter()
    .zip(flavor)
    .map(|(axis, value)| format!("{axis} {value}"))
    .collect();
  format!("我的口味画像：{}", parts.join("，"))
}

/// The recommendation buttons in rows no wider than `width` (a cell between buttons): the
/// indexes of each row's buttons.
pub fn button_rows(names: &[&str], width: u16) -> Vec<Vec<usize>> {
  let mut rows: Vec<Vec<usize>> = Vec::new();
  let mut used = 0usize;
  for (index, name) in names.iter().enumerate() {
    let cells = Button::new(ButtonKind::Green, "", name.to_string()).cells();
    match rows.last_mut() {
      Some(row) if used + 1 + cells <= usize::from(width) => {
        row.push(index);
        used += 1 + cells;
      }
      _ => {
        rows.push(vec![index]);
        used = cells;
      }
    }
  }
  rows
}

/// A taste line: the label (`mute`, `label_cells` wide) and the server's phrase, cut to fit.
fn taste_line(name: &str, value: &str, label_cells: u16, width: u16) -> Text {
  let room = usize::from(width.saturating_sub(label_cells));
  text("")
    .child(label(pad_end(name, usize::from(label_cells)), Tone::Mute))
    .child(label(truncate(value, room), Tone::Fg))
}

fn recommendations(stores: Stores, items: &[ProductRef], width: u16) -> View {
  let names: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
  let profile = stores.profile;
  view()
    .flex_none()
    .child(label("按画像推荐", Tone::Mute))
    .children(
      button_rows(&names, width)
        .into_iter()
        .map(|row| {
          view().flex_row().children(
            row
              .into_iter()
              .enumerate()
              .map(|(at, index)| {
                view().flex_row().child((at > 0).then(|| hspace(1))).child(
                  Button::new(ButtonKind::Green, "", items[index].name.clone())
                    .focused(move || profile.is_focused(ProfileFocus::Recommendation(index)))
                    .on_press(move || {
                      profile.set_focus(Some(ProfileFocus::Recommendation(index)));
                      open_recommendation(&stores, index);
                    }),
                )
              })
              .collect::<Vec<_>>(),
          )
        })
        .collect::<Vec<_>>(),
    )
}

fn text_column(stores: Stores, profile: &Profile, geo: Geometry) -> View {
  let width = geo.taste_text();
  let label_cells = geo.taste_label();
  let gap = || vspace(u16::from(geo.radar));
  let column = view().flex_1().min_w(px(0.0)).child(gap());
  let Some(taste) = &profile.taste else {
    return column.child(label(
      truncate("下单后生成你的口味画像", usize::from(width)),
      Tone::Soft,
    ));
  };
  let column = column
    .child(taste_line("偏好", &taste.prefer, label_cells, width))
    .child(gap())
    .child(taste_line("常买", &taste.often, label_cells, width))
    .child(gap())
    .child(taste_line("少碰", &taste.avoid, label_cells, width));
  if profile.recommendations.is_empty() {
    column
  } else {
    column
      .child(vspace(1))
      .child(recommendations(stores, &profile.recommendations, width))
  }
}

fn body(stores: Stores, profile: &Profile, geo: Geometry) -> View {
  let flavor = profile.taste.as_ref().map_or([0; 6], |taste| taste.flavor);
  let values = flavor.map(f32::from);
  let fits = geo.radar;
  // Without a taste and without room for the rim, the sentence alone.
  let chart = (fits || profile.taste.is_some()).then(|| {
    view()
      .flex_none()
      .flex_row()
      .child(fits.then(|| hspace(1)))
      .child(radar(
        move || values,
        Tone::Acc2,
        move || fits,
        taste_label(flavor),
      ))
      .child(hspace(3))
  });
  view()
    .flex_none()
    .child(geo.radar.then(|| vspace(1)))
    .child(
      view()
        .flex_row()
        .child(chart)
        .child(text_column(stores, profile, geo)),
    )
    .child(geo.radar.then(|| vspace(1)))
}

/// The taste panel (with a blank row under it in two columns), once the profile is there.
pub fn taste_panel(stores: Stores, geo: Geometry) -> AnyElement {
  let profile = stores.profile;
  dynamic(move || {
    profile.profile().map(|data| {
      view()
        .flex_none()
        .child(section("我的口味画像 TASTE", geo, body(stores, &data, geo)))
        .child((!geo.one_column).then(|| vspace(1)))
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui::text::display_width;

  #[test]
  fn labels_and_button_rows() {
    assert_eq!(
      taste_label([8, 7, 4, 2, 7, 8]),
      "我的口味画像：酸度 8，甜感 7，醇厚 4，苦度 2，余韵 7，香气 8"
    );
    // `[ 涅里 AA 双洗 → ]` 18, `[ 保山 小粒 日晒 → ]` 20, a cell apart.
    let names = ["涅里 AA 双洗", "保山 小粒 日晒"];
    assert_eq!(button_rows(&names, 72), vec![vec![0, 1]]);
    assert_eq!(button_rows(&names, 39), vec![vec![0, 1]]);
    assert_eq!(button_rows(&names, 38), vec![vec![0], vec![1]]);
    assert!(button_rows(&[], 40).is_empty());
    assert_eq!(display_width("干净甜感 · 悠长余韵 · 花香"), 26);
  }
}
