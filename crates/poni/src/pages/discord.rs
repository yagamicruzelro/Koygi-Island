//! Discord Voice popup card.
//!
//! Shows current Discord voice channel status, real-time microphone audio wave,
//! and interactive action shortcuts (Open Voice, Mute, Deafen).

use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::{wid, PillStyle, Ui, PILL_REGULAR};
use poni_core::geom::{palette, Color, Rect};

pub const HEIGHT: f32 = 144.0;

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

pub fn next_wake(st: &AppState) -> Option<f64> {
    if st.discord.in_voice {
        Some(st.now + 0.05)
    } else {
        None
    }
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    ui.request_anim();
    let cy = area.y + 36.0;

    let peak = crate::services::discord::current_mic_peak();
    let norm_peak = ((peak - 0.035).max(0.0) / 0.25).min(1.0);
    let is_speaking = norm_peak > 0.05 && !st.discord.is_muted;

    // 1. Left avatar/status badge (52x52 circle)
    let badge_size = 52.0;
    let badge_rect = Rect::new(area.x + 8.0, cy - badge_size / 2.0, badge_size, badge_size);
    let badge_center = badge_rect.center();

    let (mic_color, bg_color, status_text) = if st.discord.is_muted {
        (
            Color::rgba(0.95, 0.28, 0.28, 1.0),
            Color::rgba(0.95, 0.28, 0.28, 0.16),
            "Muted",
        )
    } else if is_speaking {
        let pulse = (peak * 2.0).min(1.0);
        (
            Color::rgba(0.2, 0.95, 0.45, 1.0),
            Color::rgba(0.14, 0.85, 0.42, 0.25 + 0.15 * pulse),
            "Speaking",
        )
    } else {
        (
            Color::rgba(0.16, 0.8, 0.42, 1.0),
            Color::rgba(0.16, 0.8, 0.42, 0.14),
            "Connected",
        )
    };

    // Circular background badge with glow
    ui.c.fill_circle(badge_center, badge_size / 2.0, bg_color);
    let stroke_alpha = if is_speaking { 0.6 } else { 0.35 };
    ui.c.stroke_circle(badge_center, badge_size / 2.0, mic_color.with_alpha(stroke_alpha), 1.5);
    ui.c.icon(Icon::Mic, Rect::from_center(badge_center, 24.0, 24.0), mic_color);

    // 2. Middle info column
    let info_x = area.x + 8.0 + badge_size + 16.0;
    let right_pad = 72.0;
    let info_w = (area.w - (info_x - area.x) - right_pad).max(60.0);

    // Service label
    let service_style = TextStyle::footnote().weight(Weight::Semibold).color(Color::rgba(0.45, 0.52, 0.96, 1.0));
    ui.c.text("DISCORD VOICE", Rect::new(info_x, cy - 24.0, info_w, 16.0), &service_style);

    // Channel or Server title
    let channel_title = if !st.discord.channel.is_empty() && st.discord.channel != "Voice Connected" {
        st.discord.channel.as_str()
    } else if !st.discord.server.is_empty() {
        st.discord.server.as_str()
    } else {
        "Voice Connected"
    };

    let sub_server = if !st.discord.server.is_empty() && st.discord.server != channel_title {
        st.discord.server.as_str()
    } else {
        "RTC 144Hz"
    };
    let sub_text = format!("● {status_text} • {sub_server}");

    let title_style = TextStyle::headline().weight(Weight::Bold).color(palette::TEXT);
    ui.c.text(channel_title, Rect::new(info_x, cy - 6.0, info_w, 22.0), &title_style);

    let rtc_style = TextStyle::caption().weight(Weight::Medium).color(mic_color);
    ui.c.text(&sub_text, Rect::new(info_x, cy + 16.0, info_w, 16.0), &rtc_style);

    // 3. Top-right real-time equalizer (reacts directly to actual human voice input)
    let eq_r = Rect::new(area.right() - 44.0, cy - 9.0, 32.0, 18.0);
    let n = 4;
    let gap = 2.5;
    let bw = (eq_r.w - gap * (n as f32 - 1.0)) / n as f32;
    for i in 0..n {
        let phase = st.now * (7.0 + i as f64 * 2.2) + i as f64 * 1.5;
        let amp = if is_speaking {
            (0.35 + 0.65 * (0.5 + 0.5 * phase.sin()) as f32 * norm_peak).clamp(0.25, 1.0)
        } else {
            0.18
        };
        let h = if is_speaking { (eq_r.h * amp).max(3.0) } else { 3.0 };
        let x = eq_r.x + i as f32 * (bw + gap);
        ui.c.fill_rrect(Rect::new(x, eq_r.center().y - h / 2.0, bw, h), bw / 2.0, mic_color);
    }

    // 4. Action buttons row (bottom) - EXACTLY 3 buttons: Open Voice, Mute, Deafen
    let btn_y = area.y + 78.0;
    let btn_h = PILL_REGULAR;

    // Button 1: "Open Voice" (deep links straight to active server & channel)
    let open_btn_r = Rect::new(area.x + 8.0, btn_y, 114.0, btn_h);
    let open_resp = ui.pill(
        wid("discord.open"),
        open_btn_r,
        "Open Voice",
        Some(Icon::Open),
        PillStyle::Primary(Color::rgba(0.35, 0.42, 0.92, 1.0)),
    );
    if open_resp.clicked {
        if !st.discord.guild_id.is_empty() && !st.discord.channel_id.is_empty() {
            let url = format!("discord://-/channels/{}/{}", st.discord.guild_id, st.discord.channel_id);
            crate::shell::open_discord_app(&url);
        } else {
            crate::shell::open_discord_app("discord://");
        }
    }

    // Button 2: Mute / Unmute (seamless toggle without foreground collision)
    let mute_btn_r = Rect::new(area.x + 130.0, btn_y, 96.0, btn_h);
    let (mute_label, mute_style) = if st.discord.is_muted {
        ("Unmute", PillStyle::Primary(Color::rgba(0.92, 0.32, 0.32, 1.0)))
    } else {
        ("Mute", PillStyle::Neutral)
    };
    let mute_resp = ui.pill(wid("discord.mute"), mute_btn_r, mute_label, Some(Icon::Mic), mute_style);
    if mute_resp.clicked {
        crate::shell::send_discord_shortcut_seamless(true, true, 0x4D); // Ctrl+Shift+M
    }

    // Button 3: Deafen / Undeafen (seamless toggle without foreground collision)
    let deaf_btn_r = Rect::new(area.x + 234.0, btn_y, 96.0, btn_h);
    let (deaf_label, deaf_style) = if st.discord.is_deafened {
        ("Undeafen", PillStyle::Primary(Color::rgba(0.92, 0.32, 0.32, 1.0)))
    } else {
        ("Deafen", PillStyle::Neutral)
    };
    let deaf_resp = ui.pill(wid("discord.deaf"), deaf_btn_r, deaf_label, Some(Icon::Headphones), deaf_style);
    if deaf_resp.clicked {
        crate::shell::send_discord_shortcut_seamless(true, true, 0x44); // Ctrl+Shift+D
    }
}
