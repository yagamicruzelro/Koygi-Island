//! Quick Notes & Memos page: direct in-island typing, persistent notes,
//! individual note copying, delete per note, and instant file access.

use crate::commands::Command;
use crate::gfx::TextStyle;
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::{wid, wid_n, PillStyle, Ui, PILL_GAP, PILL_SMALL};
use poni_core::geom::{palette, Color, Point, Rect};

pub fn height(_st: &AppState) -> f32 {
    188.0
}

pub fn next_wake(st: &AppState) -> Option<f64> {
    st.ui.notes_copied.map(|(_, t)| t + 1.5)
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let accent = st.accent;
    let now = st.now;
    let card = super::card::pill_card(st, area);
    let full = Rect::new(area.x, area.y, area.w, area.h);
    super::card::fill_box(ui, card, full, 18.0, palette::SURFACE);

    let top_y = full.y + 12.0;
    // Header
    let head_r = Rect::new(full.x + 16.0, top_y, 160.0, 20.0);
    ui.c.text("Quick Notes & Memos", head_r, &TextStyle::headline().color(accent));

    let open_w = 88.0;
    let clear_w = 64.0;
    let copy_w = 76.0;
    let open_r = Rect::new(full.right() - 16.0 - open_w, top_y, open_w, PILL_SMALL);
    if ui.pill(wid("notes.open"), open_r, "Open File", None, PillStyle::Utility).clicked {
        let p = crate::paths::data_dir().join("scratchpad.txt");
        if !p.exists() {
            let _ = std::fs::write(&p, "");
        }
        st.cmd(Command::OpenPath(p.to_string_lossy().into()));
    }

    let clear_r = Rect::new(open_r.x - PILL_GAP - clear_w, top_y, clear_w, PILL_SMALL);
    if ui.pill(wid("notes.clear"), clear_r, "Clear", None, PillStyle::Utility).clicked {
        st.cmd(Command::ClearScratchpad);
    }

    let copy_r = Rect::new(clear_r.x - PILL_GAP - copy_w, top_y, copy_w, PILL_SMALL);
    if ui.pill(wid("notes.copy_all"), copy_r, "Copy All", None, PillStyle::Utility).clicked {
        st.cmd(Command::CopyText(st.scratchpad_text.clone()));
    }

    // Direct in-island input row
    let in_y = top_y + PILL_SMALL + 10.0;
    let in_h = 28.0;
    let add_w = 56.0;
    let in_r = Rect::new(full.x + 16.0, in_y, full.w - 32.0 - add_w - 8.0, in_h);
    let resp = ui.text_input_row(
        wid("notes.note_in"),
        in_r,
        &mut st.scratchpad_input,
        "Type note or prompt... (Enter to save)",
        200,
        accent,
        Some(Icon::Plus),
    );
    let add_r = Rect::new(in_r.right() + 8.0, in_y, add_w, in_h);
    let add_click = ui.pill(wid("notes.note_add"), add_r, "+ Add", None, PillStyle::UtilityAccent(accent)).clicked;
    if (resp.submitted || add_click) && !st.scratchpad_input.trim().is_empty() {
        st.cmd(Command::AddScratchpadNote(st.scratchpad_input.clone()));
    }

    // Viewer card
    let box_y = in_y + in_h + 8.0;
    let box_h = (full.bottom() - 14.0 - box_y).max(20.0);
    let box_r = Rect::new(full.x + 16.0, box_y, full.w - 32.0, box_h);
    ui.c.fill_rrect(box_r, 10.0, palette::ISLAND);

    let notes: Vec<&str> = st.scratchpad_text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if notes.is_empty() {
        ui.c.text(
            "No notes yet. Type a quick thought, memo, or prompt above and press Enter to save.",
            box_r.inset(12.0, 10.0),
            &TextStyle::footnote().color(palette::TEXT_DIM),
        );
    } else {
        const ITEM_H: f32 = 28.0;
        const GAP: f32 = 4.0;
        let total_content_h = notes.len() as f32 * (ITEM_H + GAP) + 8.0;
        let max_scroll = (total_content_h - box_h).max(0.0);
        let notches = ui.wheel_in(box_r);
        st.ui.notes_scroll = (st.ui.notes_scroll - notches * 26.0).clamp(0.0, max_scroll);

        ui.c.clip(box_r.inset(2.0, 2.0));
        let mut delete_idx = None;
        let mut copy_idx = None;
        for (i, note) in notes.iter().enumerate() {
            let item_y = box_r.y + 4.0 + i as f32 * (ITEM_H + GAP) - st.ui.notes_scroll;
            let item_r = Rect::new(box_r.x + 6.0, item_y, box_r.w - 12.0, ITEM_H);
            if item_r.bottom() < box_r.y || item_r.y > box_r.bottom() {
                continue;
            }
            let hovered = ui.hovered(item_r);
            let bg = if hovered { Color::white(0.06) } else { Color::white(0.02) };
            ui.c.fill_rrect(item_r, 6.0, bg);

            // Accent dot bullet
            let dot_x = item_r.x + 10.0;
            let dot_y = item_r.center().y;
            ui.c.fill_circle(Point::new(dot_x, dot_y), 2.5, accent);

            // Delete button on the far right
            let del_w = 20.0;
            let del_r = Rect::new(item_r.right() - del_w - 4.0, item_r.center().y - 10.0, del_w, 20.0);
            let del_id = wid_n("notes.item.del", i as u64);
            let del_color = if hovered { accent } else { palette::TEXT_FAINT };
            if ui.icon_button(del_id, del_r, Icon::Close, del_color, 9.0).clicked {
                delete_idx = Some(i);
            }

            // Copy button just to the left of Delete button
            let copy_w = 20.0;
            let copy_r = Rect::new(del_r.x - copy_w - 4.0, item_r.center().y - 10.0, copy_w, 20.0);
            let copy_id = wid_n("notes.item.copy", i as u64);
            let is_copied = st.ui.notes_copied.is_some_and(|(idx, t)| idx == i && (now - t) < 1.5);
            let copy_icon = if is_copied { Icon::Check } else { Icon::Copy };
            let copy_color = if is_copied { accent } else if hovered { accent } else { palette::TEXT_FAINT };
            if ui.icon_button(copy_id, copy_r, copy_icon, copy_color, 10.0).clicked {
                copy_idx = Some(i);
            }

            // Note text
            let text_r = Rect::new(dot_x + 8.0, item_r.y, (copy_r.x - dot_x - 8.0).max(10.0), item_r.h);
            ui.c.text(note, text_r, &TextStyle::body().color(palette::TEXT));
        }
        ui.c.unclip();

        if let Some(idx) = copy_idx {
            if let Some(&note) = notes.get(idx) {
                st.ui.notes_copied = Some((idx, now));
                st.cmd(Command::CopyText(note.to_string()));
            }
        }
        if let Some(idx) = delete_idx {
            st.cmd(Command::DeleteScratchpadNote(idx));
        }
    }
}
