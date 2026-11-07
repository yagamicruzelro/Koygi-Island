//! Health page: purged and disabled.

use crate::icons::Icon;
use crate::state::{AppState, Notice};
use crate::ui::Ui;
use poni_core::datetime::{fmt_clock, fmt_date_short, LocalDateTime};
use poni_core::geom::{palette, Rect};

pub fn height(_st: &AppState) -> f32 {
    0.0
}

pub fn draw(_ui: &mut Ui, _st: &mut AppState, _area: Rect) {}

pub fn goal_notice(steps: u32, until: f64) -> Notice {
    Notice {
        icon: Icon::Steps,
        color: palette::GREEN,
        title: format!("{steps} steps today"),
        title_dim: String::new(),
        subtitle: "Daily step goal reached".into(),
        until,
        action: None,
        inline: None,
        reveal: None,
    }
}

pub(crate) fn updated_text(st: &AppState, at: i64) -> String {
    let local = st.local;
    let since_midnight = local.hour as i64 * 3600 + local.minute as i64 * 60 + local.second as i64;
    let ago = (st.unix - at).max(0);
    if ago <= since_midnight {
        let secs = since_midnight - ago;
        let t = LocalDateTime { hour: (secs / 3600) as u8, minute: ((secs / 60) % 60) as u8, second: 0, ..local };
        return format!("Updated {}", fmt_clock(&t, st.settings.use_24h));
    }
    let days = (ago - since_midnight - 1) / 86_400 + 1;
    if days == 1 {
        "Updated yesterday".to_string()
    } else {
        format!("Updated {}", fmt_date_short(&local.add_days(-days)))
    }
}
