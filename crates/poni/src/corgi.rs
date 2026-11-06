//! Corgi mascot: Playful Corgi puppy companion living in the dynamic island.
//!
//! Crafted in the same clean vector artstyle and palette vibrancy as Oren:
//! - Warm golden-honey coat (#F5A030), white facial blaze & muzzle, cute button nose,
//!   and adorable pink tongue melet without dark masks.
//! - Ear fold: Distinct triangular crease fold (#C77218) when resting/idle, matching Oren's iconic fold.
//! - Music: Super happy smile with pink tongue melet, perky upright ears with ear twitches,
//!   and floating musical notes drifting upward.
//! - Animation: Stable stationary base matching Oren (no erratic up-and-down bobbing).
//! - Idle: Relaxed half-closed eyelids, folded droopy ear, blinking and ear twitches on timer.
//! - Sleep: Curved peaceful smile slits with animated floating Zz.

use crate::gfx::Canvas;
use poni_core::geom::{Color, Point, Rect};
use poni_core::settings::Theme;
use poni_core::svgpath::Seg;

pub const GOLDEN: Color = Color::hex(0xF5A030);      // Warm vibrant Welsh Corgi golden-honey coat
pub const FOLD_GOLDEN: Color = Color::hex(0xC77218); // Darker warm honey for ear fold crease (like Oren's fold)
pub const INNER_EAR: Color = Color::hex(0xFDB7B7);   // Soft pastel pink for inner ear
pub const WHITE: Color = Color::hex(0xFFFBF5);       // Soft warm white for blaze & muzzle
pub const INK: Color = Color::hex(0x221B19);         // Dark ink for eyes, nose, smile
pub const TONGUE: Color = Color::hex(0xFF6B8B);      // Vibrant cute pink tongue

pub fn on(theme: Theme) -> bool {
    theme == Theme::Corgi || theme == Theme::CorgiPill
}

/// App icon tile using Corgi puppy art.
pub fn app_icon(c: &mut Canvas, center: Point, size: f32) {
    let tile = Rect::new(center.x - size / 2.0, center.y - size / 2.0, size, size);
    c.unthemed(|c| {
        c.fill_rrect(tile, size * 0.2237, Color::hex(0x2D2118));
        let cx = center.x;
        let bar_y = center.y + size * 0.40;
        let w = size * 0.72;
        peek(c, cx, bar_y, w, tile.y + 1.0, false, false, None, 0.0, true);
    });
}

/// Draw Corgi peeking over baseline at `bar_y`.
pub fn peek(
    c: &mut Canvas,
    cx: f32,
    bar_y: f32,
    w: f32,
    top: f32,
    is_night: bool,
    is_bored: bool,
    look: Option<f32>,
    ear_wobble: f32,
    is_happy: bool,
) {
    let k = w / 26.0;
    c.clip(Rect::new(cx - w * 1.3, top, 2.6 * w, (bar_y - top + 10.0).max(0.0)));

    // 1. Right Ear (Always upright, iconic pointy Corgi ear)
    let r_tip = Point::new(cx + (8.5 - ear_wobble) * k, bar_y - 20.0 * k);
    let r_b1 = Point::new(cx + 3.5 * k, bar_y - 12.0 * k);
    let r_b2 = Point::new(cx + 10.5 * k, bar_y - 10.0 * k);
    let right_ear_segs = [
        Seg::MoveTo(r_b1.x, r_b1.y),
        Seg::LineTo(r_tip.x, r_tip.y),
        Seg::LineTo(r_b2.x, r_b2.y),
        Seg::Close,
    ];
    c.path(&right_ear_segs, Some(GOLDEN), None);

    let ri_tip = Point::new(cx + (8.3 - ear_wobble * 0.7) * k, bar_y - 18.0 * k);
    let ri_b1 = Point::new(cx + 4.5 * k, bar_y - 12.5 * k);
    let ri_b2 = Point::new(cx + 9.5 * k, bar_y - 11.0 * k);
    let right_inner_segs = [
        Seg::MoveTo(ri_b1.x, ri_b1.y),
        Seg::LineTo(ri_tip.x, ri_tip.y),
        Seg::LineTo(ri_b2.x, ri_b2.y),
        Seg::Close,
    ];
    c.path(&right_inner_segs, Some(INNER_EAR), None);

    // 2. Left Ear:
    // If happy (music): upright perky ear with soft pink inner ear.
    // If idle/sleep: folded down ear matching Oren's iconic fold geometry!
    if is_happy {
        let l_tip = Point::new(cx - (8.5 + ear_wobble) * k, bar_y - 20.0 * k);
        let l_b1 = Point::new(cx - 10.5 * k, bar_y - 10.0 * k);
        let l_b2 = Point::new(cx - 3.5 * k, bar_y - 12.0 * k);
        let left_ear_segs = [
            Seg::MoveTo(l_b1.x, l_b1.y),
            Seg::LineTo(l_tip.x, l_tip.y),
            Seg::LineTo(l_b2.x, l_b2.y),
            Seg::Close,
        ];
        c.path(&left_ear_segs, Some(GOLDEN), None);

        let li_tip = Point::new(cx - (8.3 + ear_wobble * 0.7) * k, bar_y - 18.0 * k);
        let li_b1 = Point::new(cx - 9.5 * k, bar_y - 11.0 * k);
        let li_b2 = Point::new(cx - 4.5 * k, bar_y - 12.5 * k);
        let left_inner_segs = [
            Seg::MoveTo(li_b1.x, li_b1.y),
            Seg::LineTo(li_tip.x, li_tip.y),
            Seg::LineTo(li_b2.x, li_b2.y),
            Seg::Close,
        ];
        c.path(&left_inner_segs, Some(INNER_EAR), None);
    } else {
        // Base left ear stub
        let ear_stub = [
            Seg::MoveTo(cx - 10.5 * k, bar_y - 10.0 * k),
            Seg::LineTo(cx - 6.5 * k, bar_y - 14.5 * k),
            Seg::LineTo(cx - 3.5 * k, bar_y - 12.0 * k),
            Seg::Close,
        ];
        c.path(&ear_stub, Some(GOLDEN), None);

        // Folded tip flap pointing down-left with crisp darker shade (matching Oren's fold)
        let fold_tip = [
            Seg::MoveTo(cx - 4.5 * k, bar_y - 12.0 * k),
            Seg::LineTo(cx - 13.0 * k, bar_y - 7.5 * k),
            Seg::LineTo(cx - 8.5 * k, bar_y - 15.0 * k),
            Seg::Close,
        ];
        c.path(&fold_tip, Some(FOLD_GOLDEN), None);
    }

    // 3. Head main body (warm vibrant Welsh Corgi golden-honey)
    let head_r = Rect::from_center(Point::new(cx, bar_y - 7.5 * k), 22.0 * k, 15.0 * k);
    c.fill_rrect(head_r, 7.5 * k, GOLDEN);

    // 4. White markings: Forehead blaze & clean cute muzzle (NO black mask!)
    let blaze_r = Rect::from_center(Point::new(cx, bar_y - 8.5 * k), 3.0 * k, 6.0 * k);
    c.fill_rrect(blaze_r, 1.5 * k, WHITE);

    let muzzle_r = Rect::from_center(Point::new(cx, bar_y - 4.8 * k), 8.5 * k, 6.5 * k);
    c.fill_rrect(muzzle_r, 3.5 * k, WHITE);

    // 5. Button nose (clean small ink dot on white muzzle)
    let nose_p = Point::new(cx, bar_y - 6.2 * k);
    c.fill_circle(nose_p, 1.1 * k, INK);

    // 6. Mouth & Tongue (Melet)
    if is_happy {
        // Cheerful open smile with cute pink tongue hanging out!
        let smile_segs = [
            Seg::MoveTo(cx - 3.2 * k, bar_y - 4.6 * k),
            Seg::QuadTo(cx, bar_y - 3.5 * k, cx + 3.2 * k, bar_y - 4.6 * k),
        ];
        c.path(&smile_segs, None, Some((INK, 0.9 * k)));

        // Cute rounded pink tongue (melet) peeking out
        let tongue_r = Rect::new(cx - 1.8 * k, bar_y - 4.2 * k, 3.6 * k, 4.4 * k);
        c.fill_rrect(tongue_r, 1.8 * k, TONGUE);
        c.fill_rrect(Rect::new(cx - 0.25 * k, bar_y - 4.0 * k, 0.5 * k, 2.5 * k), 0.25 * k, Color::hex(0xDE4F72));
    } else {
        // Subtle cute puppy smile line
        let smile_segs = [
            Seg::MoveTo(cx - 2.6 * k, bar_y - 4.4 * k),
            Seg::QuadTo(cx, bar_y - 3.7 * k, cx + 2.6 * k, bar_y - 4.4 * k),
        ];
        c.path(&smile_segs, None, Some((INK, 0.8 * k)));
    }

    // 7. Eyes
    let look_x = look.unwrap_or(0.0).clamp(-1.0, 1.0) * 0.8 * k;
    if is_night {
        // Sleeping curved peaceful smile slits (u u)
        let eye_y = bar_y - 8.8 * k;
        let left_eye = [
            Seg::MoveTo(cx - 6.5 * k, eye_y),
            Seg::QuadTo(cx - 4.8 * k, eye_y + 1.2 * k, cx - 3.1 * k, eye_y),
        ];
        let right_eye = [
            Seg::MoveTo(cx + 3.1 * k, eye_y),
            Seg::QuadTo(cx + 4.8 * k, eye_y + 1.2 * k, cx + 6.5 * k, eye_y),
        ];
        c.path(&left_eye, None, Some((INK, 1.1 * k)));
        c.path(&right_eye, None, Some((INK, 1.1 * k)));
    } else if is_bored {
        // Relaxed, sleepy, droopy half-closed eyelids
        let eye_l = Point::new(cx - 5.0 * k + look_x, bar_y - 8.8 * k);
        let eye_r = Point::new(cx + 5.0 * k + look_x, bar_y - 8.8 * k);
        c.fill_circle(eye_l, 1.7 * k, INK);
        c.fill_circle(eye_r, 1.7 * k, INK);

        // Droopy golden eyelid covers top 45%
        c.fill_rect(Rect::new(eye_l.x - 2.0 * k, eye_l.y - 2.0 * k, 4.0 * k, 1.8 * k), GOLDEN);
        c.fill_rect(Rect::new(eye_r.x - 2.0 * k, eye_r.y - 2.0 * k, 4.0 * k, 1.8 * k), GOLDEN);

        c.fill_circle(Point::new(eye_l.x + 0.4 * k, eye_l.y + 0.3 * k), 0.5 * k, WHITE);
        c.fill_circle(Point::new(eye_r.x + 0.4 * k, eye_r.y + 0.3 * k), 0.5 * k, WHITE);
    } else {
        // Alert round eyes with cute specular glint
        let eye_l = Point::new(cx - 5.0 * k + look_x, bar_y - 9.0 * k);
        let eye_r = Point::new(cx + 5.0 * k + look_x, bar_y - 9.0 * k);
        c.fill_circle(eye_l, 1.7 * k, INK);
        c.fill_circle(eye_r, 1.7 * k, INK);

        c.fill_circle(Point::new(eye_l.x + 0.5 * k, eye_l.y - 0.5 * k), 0.6 * k, WHITE);
        c.fill_circle(Point::new(eye_r.x + 0.5 * k, eye_r.y - 0.5 * k), 0.6 * k, WHITE);
    }

    c.unclip();
}

/// Music peek: Corgi is super happy with perky ears, melet tongue, gentle ear twitches,
/// and floating music notes! Base position is stable and calm (matching Oren).
pub fn music_peek(
    c: &mut Canvas,
    cx: f32,
    bar_y: f32,
    w: f32,
    top: f32,
    now: f64,
    playing: bool,
) {
    let ear_wobble = if playing {
        (now * 5.0).sin() as f32 * 1.5
    } else {
        0.0
    };
    let k = w / 26.0;
    // Stable baseline (no bobbing up and down)
    peek(c, cx, bar_y, w, top, false, false, None, ear_wobble, playing);

    // Floating music notes near the ears while playing
    if playing {
        let note_color = c.look().ink.with_alpha(0.80);
        for (idx, (ox, phase)) in [(-11.0 * k, 0.0), (11.0 * k, 1.8)].iter().enumerate() {
            let t = (now * 1.4 + phase).rem_euclid(2.0);
            let float_y = (t as f32) * 15.0 * k;
            let note_alpha = (1.0 - (t as f32 / 2.0)).clamp(0.0, 1.0) * 0.85;
            let note_x = cx + ox + ((now * 2.2 + phase).sin() as f32) * 2.0 * k;
            let note_y = bar_y - 20.0 * k - float_y;
            if note_y > top - 5.0 {
                let col = note_color.with_alpha(note_alpha);
                c.fill_circle(Point::new(note_x, note_y), 1.5 * k, col);
                c.fill_rrect(Rect::new(note_x + 0.9 * k, note_y - 4.5 * k, 0.8 * k, 4.5 * k), 0.4 * k, col);
                if idx == 1 {
                    c.fill_rrect(Rect::new(note_x + 0.9 * k, note_y - 4.5 * k, 2.2 * k, 1.1 * k), 0.5 * k, col);
                }
            }
        }
    }
}

/// Rest peek: Corgi while resting with droopy ear down.
/// Stationary base matching Oren's calm presence.
pub fn rest_peek(
    c: &mut Canvas,
    cx: f32,
    bar_y: f32,
    w: f32,
    top: f32,
    now: f64,
    is_night: bool,
    look: Option<f32>,
) {
    let k = w / 26.0;
    peek(c, cx, bar_y, w, top, is_night, !is_night, look, 0.0, false);

    // Floating Zz when sleeping at night
    if is_night {
        let z_phase = (now * 0.85).rem_euclid(2.0);
        let z_y = bar_y - 12.0 * k - (z_phase as f32 * 14.0 * k);
        let z_x = cx + 8.0 * k + ((now * 1.2).sin() as f32) * 1.8 * k;
        let z_alpha = (1.0 - (z_phase as f32 / 2.0)).clamp(0.0, 1.0) * 0.8;
        if z_y > top {
            c.text("z", Rect::new(z_x, z_y, 10.0 * k, 10.0 * k), &crate::gfx::TextStyle::caption().color(c.look().ink.with_alpha(z_alpha)));
        }
    }
}
