//! 开屏 (00-splash.md): what a new connection shows while it starts, instead of a blank
//! screen or a login page that flashes and goes. It shows the brand at once and what the
//! connection is doing, one line per step, each with its glyph. It never mentions the SSH
//! key: only the login card's 记住我 does.
//!
//! ```text
//!     (   )  (
//!      )  (   )                          ← steam and cup: 32 rows or more
//!   ┌────────────┐
//!   …
//! ████   ███   ███   ████ █████     █ █████ █████ █   █
//! …                                                      ← the block wordmark (53×5)
//!            一家开在终端里的精品咖啡烘焙店。
//!
//! ✓ 终端已连接 · 160×50
//! ⠹ 正在恢复登录…                                    ○ 请登录或以访客浏览 (no remembered login)
//! ⠹ 正在载入今日豆单…
//!
//! ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━─────────────────
//! ```
//!
//! Meanwhile the shell's `actions::start` resumes a remembered key and the page prefetches
//! the bean list (the most likely next page, public to guests too). It leaves
//! ([`actions::leave_splash`]) once [`SPLASH_MIN`] has passed and the work it waits for has
//! settled — the resume, and for a resumed member the bean list as well, so the catalog
//! opens loaded —, at the latest after [`SPLASH_MAX`], or at once on `⏎ 跳过`. A resumed
//! member lands on the catalog, everyone else on the login page.

pub mod commands;

use std::time::Duration;

use nsh_core::{element::Screen, prelude::*};

use super::PageModule;
use crate::{
  api::ApiFailure,
  commands::Mode,
  router::{Page, Route},
  stores::{Phase, Stores, actions},
  ui::{self, Layout, Tone, Viewport, layout},
};

/// `欢迎 WELCOME`.
pub const MODE: Mode = Mode::new("欢迎", "WELCOME");

/// The page's registration.
pub const MODULE: PageModule = PageModule {
  page: Page::Splash,
  mode: |_| MODE,
  commands: commands::COMMANDS,
  view: render,
  dialogs: &[],
};

/// The shortest the splash shows: long enough to read the brand and see the steam move,
/// never a flash.
pub const SPLASH_MIN: Duration = Duration::from_millis(900);
/// The longest it waits for a slow backend: then the pages show their own loading states.
pub const SPLASH_MAX: Duration = Duration::from_secs(6);

/// The column the splash is laid out in: the block wordmark's width.
pub const COLUMN: u16 = 53;

/// `一家开在终端里的精品咖啡烘焙店。`
pub const TAGLINE: &str = "一家开在终端里的精品咖啡烘焙店。";

/// How far one step is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
  /// Under way: the spinner (`⠹`), `acc`.
  Working,
  /// Done: `✓`, `acc2`.
  Done,
  /// Done, without: `○`, `mute`.
  Off,
  /// Failed: `✗`, `danger`.
  Failed,
}

impl StepState {
  /// Whether the step is over.
  pub fn settled(self) -> bool {
    self != StepState::Working
  }
}

/// One line of the splash's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
  /// How far it is.
  pub state: StepState,
  /// What it says.
  pub text: String,
}

impl Step {
  fn new(state: StepState, text: impl Into<String>) -> Self {
    Self {
      state,
      text: text.into(),
    }
  }
}

/// What the splash shows its steps from (tracked reads of the stores).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
  /// The terminal's size.
  pub viewport: Viewport,
  /// The connection has an SSH key.
  pub has_key: bool,
  /// The resume is under way.
  pub resuming: bool,
  /// Who the resume signed in.
  pub member: Option<String>,
  /// Why the resume ended without a session.
  pub resume_failure: Option<ApiFailure>,
  /// The bean list's phase.
  pub catalog: Phase,
  /// How many beans it holds.
  pub count: usize,
  /// Why it failed.
  pub catalog_failure: Option<ApiFailure>,
}

impl Facts {
  /// The facts now (tracked).
  pub fn read(stores: &Stores, viewport: Viewport) -> Self {
    Self {
      viewport,
      has_key: stores.session.has_key(),
      resuming: stores.session.resuming(),
      member: stores.session.member().map(|member| member.user.username),
      resume_failure: stores.session.resume_failure(),
      catalog: stores.catalog.phase(),
      count: stores.catalog.count(),
      catalog_failure: stores.catalog.failure(),
    }
  }

  /// The steps, in order: the terminal, the account (resuming a remembered login, or
  /// where to go from here), the bean list.
  pub fn steps(&self) -> Vec<Step> {
    let Viewport { cols, rows } = self.viewport;
    let mut steps = vec![Step::new(
      StepState::Done,
      format!("终端已连接 · {cols}×{rows}"),
    )];
    let resuming = Step::new(StepState::Working, "正在恢复登录…");
    let choose = Step::new(StepState::Off, "请登录或以访客浏览");
    steps.push(match (&self.member, &self.resume_failure) {
      (Some(name), _) => Step::new(StepState::Done, format!("欢迎回来，{name}")),
      _ if !self.has_key => choose,
      _ if self.resuming => resuming,
      (None, Some(failure)) if failure.is(crate::api::ErrorCode::NotRemembered) => choose,
      (None, Some(failure)) => Step::new(
        StepState::Failed,
        format!("未能恢复登录 · {}", failure.message()),
      ),
      // Not started yet (the shell starts it after the first render).
      (None, None) => resuming,
    });
    steps.push(match self.catalog {
      Phase::Idle | Phase::Loading => Step::new(StepState::Working, "正在载入今日豆单…"),
      Phase::Ready => Step::new(StepState::Done, format!("今日豆单 · {} 款", self.count)),
      Phase::Empty => Step::new(StepState::Off, "今天还没有上架的豆子"),
      Phase::Failed => Step::new(
        StepState::Failed,
        format!(
          "豆单没有载入 · {}",
          self.catalog_failure.as_ref().map_or_else(
            || crate::api::NETWORK_MESSAGE.to_owned(),
            ApiFailure::message
          )
        ),
      ),
    });
    steps
  }

  /// Whether the work the splash waits for has settled: the resume, and for a resumed
  /// member the bean list too (a guest's prefetch goes on behind the login page).
  pub fn settled(&self) -> bool {
    let resumed = !self.has_key || self.member.is_some() || self.resume_failure.is_some();
    resumed && (self.member.is_none() || !self.catalog.pending())
  }
}

/// The share of the steps settled, 0–1.
pub fn progress(steps: &[Step]) -> f32 {
  if steps.is_empty() {
    return 1.0;
  }
  steps.iter().filter(|step| step.state.settled()).count() as f32 / steps.len() as f32
}

fn glyph(state: StepState, animate: impl Fn() -> bool + 'static) -> Text {
  match state {
    StepState::Working => ui::spinner(animate),
    StepState::Done => ui::strong("✓", Tone::Acc2),
    StepState::Off => ui::label("○", Tone::Mute),
    StepState::Failed => ui::strong("✗", Tone::Danger),
  }
}

fn step_row(step: Step, animate: impl Fn() -> bool + 'static) -> View {
  let tone = match step.state {
    StepState::Off => Tone::Mute,
    _ => Tone::Soft,
  };
  view()
    .flex_none()
    .flex_row()
    .child(glyph(step.state, animate))
    // A blank cell, not a space of text: nsh clears braille next to a text's box.
    .child(layout::hspace(1))
    .child(ui::label(step.text, tone))
}

fn centred(child: impl IntoElement) -> View {
  view()
    .flex_none()
    .w_full()
    .flex_row()
    .justify_center()
    .child(child)
}

/// The splash.
pub fn render(_: &Route) -> Screen {
  let stores = Stores::get();
  let layout = Layout::get();
  let settings = stores.settings;
  let animate = move || settings.animations();

  // Prefetch the bean list: the catalog opens loaded, for a member or a guest.
  effect(move || untracked(|| stores.catalog.ensure_loaded()));
  let shown = signal(false);
  effect(move || {
    let least = set_timeout(SPLASH_MIN, move || shown.set(true));
    let most = set_timeout(SPLASH_MAX, move || {
      batch(|| actions::leave_splash(&stores));
    });
    cleanup(move || {
      least.cancel();
      most.cancel();
    });
  });
  let facts = computed(move || Facts::read(&stores, layout.viewport()));
  effect(move || {
    if shown.get() && facts.with(Facts::settled) {
      untracked(|| batch(|| actions::leave_splash(&stores)));
    }
  });

  let steps = computed(move || facts.with(Facts::steps));
  let list = view()
    .flex_none()
    .w(layout::cols(COLUMN))
    .child(dynamic(move || {
      let steps = steps.get();
      untracked(|| {
        view()
          .flex_none()
          .label("启动进度")
          .children(steps.into_iter().map(|step| step_row(step, animate)))
      })
    }));
  let bar = ui::thin_bar(
    move || steps.with(|steps| progress(steps)),
    1.0,
    COLUMN as usize,
  );
  let tall = computed(move || !layout.viewport().short());

  let column = view()
    .flex_none()
    .w(layout::cols(COLUMN))
    .child(when(
      move || tall.get(),
      move || {
        view()
          .flex_none()
          .child(centred(
            view()
              .flex_none()
              .child(ui::steam(animate))
              .child(ui::cup()),
          ))
          .child(layout::vspace(1))
      },
    ))
    .child(ui::wordmark_block())
    .child(layout::vspace(1))
    .child(centred(ui::strong(TAGLINE, Tone::Fg)))
    .child(layout::vspace(1))
    .child(list)
    .child(layout::vspace(1))
    .child(bar);

  ui::page(
    "欢迎 WELCOME",
    view().flex_1().justify_center().child(centred(column)),
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::api::ErrorCode;

  fn facts() -> Facts {
    Facts {
      viewport: Viewport::new(100, 30),
      has_key: false,
      resuming: false,
      member: None,
      resume_failure: None,
      catalog: Phase::Loading,
      count: 0,
      catalog_failure: None,
    }
  }

  fn states(facts: &Facts) -> Vec<StepState> {
    facts.steps().iter().map(|step| step.state).collect()
  }

  #[test]
  fn a_guest_waits_for_nothing() {
    let guest = facts();
    assert_eq!(
      states(&guest),
      [StepState::Done, StepState::Off, StepState::Working]
    );
    assert_eq!(guest.steps()[1].text, "请登录或以访客浏览");
    assert_eq!(guest.steps()[0].text, "终端已连接 · 100×30");
    assert!(
      guest.settled(),
      "the prefetch goes on behind the login page"
    );
    let ready = Facts {
      catalog: Phase::Ready,
      count: 8,
      ..facts()
    };
    assert_eq!(ready.steps()[2].text, "今日豆单 · 8 款");
    assert!(ready.steps().iter().all(|step| !step.text.contains("公钥")));
    assert_eq!(progress(&ready.steps()), 1.0);
  }

  #[test]
  fn a_key_waits_for_the_resume_and_a_member_for_the_list() {
    let resuming = Facts {
      has_key: true,
      resuming: true,
      ..facts()
    };
    assert_eq!(
      states(&resuming),
      [StepState::Done, StepState::Working, StepState::Working]
    );
    assert_eq!(resuming.steps()[1].text, "正在恢复登录…");
    assert!(!resuming.settled());
    assert!((progress(&resuming.steps()) - 1.0 / 3.0).abs() < 1e-6);
    let member = Facts {
      resuming: false,
      member: Some("林晓".into()),
      ..resuming.clone()
    };
    assert_eq!(member.steps()[1].text, "欢迎回来，林晓");
    assert!(!member.settled(), "the catalog opens loaded");
    assert!(
      Facts {
        catalog: Phase::Ready,
        ..member
      }
      .settled()
    );
    let before = Facts {
      resuming: false,
      ..resuming
    };
    assert!(!before.settled(), "the resume has not started yet");
  }

  #[test]
  fn a_failed_resume_says_why() {
    let unknown = Facts {
      has_key: true,
      resume_failure: Some(ApiFailure::of(ErrorCode::NotRemembered)),
      ..facts()
    };
    assert_eq!(
      unknown.steps()[1],
      Step::new(StepState::Off, "请登录或以访客浏览")
    );
    assert!(unknown.settled());
    let down = Facts {
      has_key: true,
      resume_failure: Some(ApiFailure::Network("refused".into())),
      catalog: Phase::Failed,
      ..facts()
    };
    assert_eq!(
      states(&down),
      [StepState::Done, StepState::Failed, StepState::Failed]
    );
    assert!(down.steps()[2].text.starts_with("豆单没有载入 · "));
    for facts in [down, unknown] {
      assert!(
        facts.steps().iter().all(|step| !step.text.contains("公钥")),
        "the splash never mentions the key"
      );
    }
  }
}
