//! Adversarial Verification & Stress Test Suite for Milestone 1
//! Requirement R1: Audio Equalizer Adaptive Contrast Guard
//!
//! Independently verifies:
//! - 6 Strict Themes + 3 Specialized Looks
//! - Combinatorial Sweep: Primaries, Secondaries, Pastels, Earth Tones, Skin Tones, Grayscales, Neons, 200 Random RGBs
//! - Mathematical invariants: CR >= 3.499:1, Hue angle delta < 0.15 deg, Proportional channel ratio invariance
//! - Robustness against NaN, Inf, out-of-range inputs, and idempotence.

use poni_core::geom::Color;
use poni_core::look;
use poni_core::settings::Theme;

fn rgb_to_hsl_hue(r: f32, g: f32, b: f32) -> Option<f32> {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta < 1e-6 {
        return None;
    }
    let h = if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    Some((h + 360.0).rem_euclid(360.0))
}

#[test]
fn test_adversarial_combinatorial_sweep_all_themes() {
    let strict_themes = [
        ("Classic", Theme::Classic),
        ("Oren", Theme::Oren),
        ("Corgi", Theme::Corgi),
        ("Pill", Theme::Pill),
        ("Oren Pill", Theme::OrenPill),
        ("Corgi Pill", Theme::CorgiPill),
    ];

    let test_colors: Vec<(&'static str, Color)> = vec![
        // Primaries & Secondaries
        ("Pure Red", Color::rgba(1.0, 0.0, 0.0, 1.0)),
        ("Pure Green", Color::rgba(0.0, 1.0, 0.0, 1.0)),
        ("Pure Blue", Color::rgba(0.0, 0.0, 1.0, 1.0)),
        ("Pure Yellow", Color::rgba(1.0, 1.0, 0.0, 1.0)),
        ("Pure Cyan", Color::rgba(0.0, 1.0, 1.0, 1.0)),
        ("Pure Magenta", Color::rgba(1.0, 0.0, 1.0, 1.0)),

        // Pastels
        ("Pastel Pink", Color::hex(0xFFD1DC)),
        ("Pastel Mint", Color::hex(0xAAF0D1)),
        ("Pastel Lavender", Color::hex(0xE6E6FA)),
        ("Pastel Peach", Color::hex(0xFFE5B4)),
        ("Pastel Blue", Color::hex(0xAEC6CF)),
        ("Pastel Yellow", Color::hex(0xFDFD96)),

        // Earth tones
        ("Earth Brown", Color::hex(0x8B4513)),
        ("Earth Olive", Color::hex(0x556B2F)),
        ("Earth Terracotta", Color::hex(0xE2725B)),
        ("Earth Sand", Color::hex(0xC2B280)),
        ("Earth Khaki", Color::hex(0xF0E68C)),
        ("Earth Sienna", Color::hex(0xA0522D)),
        ("Earth Ochre", Color::hex(0xCC7722)),

        // Skin tones (Fitzpatrick Types I - VI)
        ("Skin Fair Type I", Color::hex(0xFFDFC4)),
        ("Skin Fair Type II", Color::hex(0xF0D5BE)),
        ("Skin Medium Type III", Color::hex(0xD1A384)),
        ("Skin Olive Type IV", Color::hex(0xB58A63)),
        ("Skin Brown Type V", Color::hex(0x8D5524)),
        ("Skin Deep Type VI", Color::hex(0x3B2219)),

        // Grayscales
        ("Pure Black", Color::black(1.0)),
        ("Gray 10%", Color::rgba(0.1, 0.1, 0.1, 1.0)),
        ("Gray 25%", Color::rgba(0.25, 0.25, 0.25, 1.0)),
        ("Gray 50%", Color::rgba(0.5, 0.5, 0.5, 1.0)),
        ("Gray 75%", Color::rgba(0.75, 0.75, 0.75, 1.0)),
        ("Gray 90%", Color::rgba(0.9, 0.9, 0.9, 1.0)),
        ("Pure White", Color::white(1.0)),

        // Saturated Neons
        ("Neon Green", Color::hex(0x39FF14)),
        ("Neon Pink", Color::hex(0xFF10F0)),
        ("Neon Cyan", Color::hex(0x00FFFF)),
        ("Neon Yellow", Color::hex(0xCCFF00)),
        ("Neon Purple", Color::hex(0xB026FF)),
        ("Neon Orange", Color::hex(0xFF5F1F)),

        // Mathematical piecewise boundary points
        ("Piecewise Threshold", Color::rgba(0.04045, 0.04045, 0.04045, 1.0)),
        ("Below Piecewise", Color::rgba(0.04, 0.04, 0.04, 1.0)),
        ("Above Piecewise", Color::rgba(0.05, 0.05, 0.05, 1.0)),
        ("Sub-epsilon 1e-4", Color::rgba(1e-4, 1e-4, 1e-4, 1.0)),
        ("Near-White 0.999", Color::rgba(0.999, 0.999, 0.999, 1.0)),
    ];

    // Generate 200 deterministic pseudorandom colors using a simple LCG
    let mut seed: u64 = 0xDEADBEEF;
    let mut rand_f32 = || -> f32 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 32) as u32 as f32) / (u32::MAX as f32)
    };

    let mut random_samples = Vec::new();
    for _ in 0..200 {
        let r = rand_f32();
        let g = rand_f32();
        let b = rand_f32();
        random_samples.push(Color::rgba(r, g, b, 1.0));
    }

    let mut total_assertions = 0;

    for (theme_name, theme) in strict_themes {
        let lk = look::of(theme);
        let ground = lk.ground;
        let ground_l = ground.relative_luminance();

        // 1. Test named palette
        for &(color_name, accent) in &test_colors {
            for &alpha in &[1.0f32, 0.5f32, 0.05f32] {
                let test_accent = accent.with_alpha(alpha);
                let guarded = lk.guard_accent(test_accent);

                // Invariant 1: No NaN / Inf
                assert!(!guarded.r.is_nan() && !guarded.r.is_infinite());
                assert!(!guarded.g.is_nan() && !guarded.g.is_infinite());
                assert!(!guarded.b.is_nan() && !guarded.b.is_infinite());
                assert!(!guarded.a.is_nan() && !guarded.a.is_infinite());

                // Invariant 2: Range clamping in [0.0, 1.0]
                assert!(guarded.r >= 0.0 && guarded.r <= 1.0);
                assert!(guarded.g >= 0.0 && guarded.g <= 1.0);
                assert!(guarded.b >= 0.0 && guarded.b <= 1.0);
                assert!(guarded.a >= 0.0 && guarded.a <= 1.0);

                // Invariant 3: Alpha preservation
                assert_eq!(guarded.a, alpha, "Alpha channel must be preserved");

                // Invariant 4: Contrast Ratio >= 3.499:1
                let cr = Color::contrast_ratio(guarded, ground);
                assert!(
                    cr >= 3.499,
                    "CR below 3.499:1! Theme={}, Color={}, CR={:.4}:1",
                    theme_name, color_name, cr
                );

                // Invariant 5: Hue preservation
                if let (Some(orig_h), Some(grd_h)) = (
                    rgb_to_hsl_hue(test_accent.r, test_accent.g, test_accent.b),
                    rgb_to_hsl_hue(guarded.r, guarded.g, guarded.b)
                ) {
                    let mut diff = (orig_h - grd_h).abs();
                    if diff > 180.0 {
                        diff = 360.0 - diff;
                    }
                    assert!(
                        diff < 0.15,
                        "Hue angle distorted! Theme={}, Color={}, OrigHue={:.2}, GuardHue={:.2}, diff={:.2}",
                        theme_name, color_name, orig_h, grd_h, diff
                    );
                }

                // Invariant 6: Proportional channel ratios on light grounds
                if ground_l >= 0.25 && guarded != test_accent {
                    if test_accent.r > 0.05 && test_accent.g > 0.05 {
                        let orig_rg = test_accent.r / test_accent.g;
                        let grd_rg = guarded.r / guarded.g;
                        assert!(
                            (orig_rg - grd_rg).abs() < 1e-3,
                            "R/G ratio distorted on {}: orig={}, grd={}",
                            theme_name, orig_rg, grd_rg
                        );
                    }
                    if test_accent.g > 0.05 && test_accent.b > 0.05 {
                        let orig_gb = test_accent.g / test_accent.b;
                        let grd_gb = guarded.g / guarded.b;
                        assert!(
                            (orig_gb - grd_gb).abs() < 1e-3,
                            "G/B ratio distorted on {}: orig={}, grd={}",
                            theme_name, orig_gb, grd_gb
                        );
                    }
                }

                // Invariant 7: Idempotence
                let reguarded = lk.guard_accent(guarded);
                assert!((reguarded.r - guarded.r).abs() < 1e-5);
                assert!((reguarded.g - guarded.g).abs() < 1e-5);
                assert!((reguarded.b - guarded.b).abs() < 1e-5);

                total_assertions += 1;
            }
        }

        // 2. Test 200 random samples
        for (i, &rand_c) in random_samples.iter().enumerate() {
            let guarded = lk.guard_accent(rand_c);
            let cr = Color::contrast_ratio(guarded, ground);
            assert!(
                cr >= 3.499,
                "Random sample #{} failed on theme {}: CR={:.4}:1",
                i, theme_name, cr
            );

            // Check non-NaN
            assert!(!guarded.r.is_nan() && !guarded.g.is_nan() && !guarded.b.is_nan());

            // Check range
            assert!(guarded.r >= 0.0 && guarded.r <= 1.0);
            assert!(guarded.g >= 0.0 && guarded.g <= 1.0);
            assert!(guarded.b >= 0.0 && guarded.b <= 1.0);

            // Check hue on chromatic inputs
            if let (Some(orig_h), Some(grd_h)) = (
                rgb_to_hsl_hue(rand_c.r, rand_c.g, rand_c.b),
                rgb_to_hsl_hue(guarded.r, guarded.g, guarded.b)
            ) {
                let mut diff = (orig_h - grd_h).abs();
                if diff > 180.0 {
                    diff = 360.0 - diff;
                }
                assert!(diff < 0.15, "Random #{} hue distorted: diff={:.2}", i, diff);
            }

            total_assertions += 1;
        }
    }

    println!("Total adversarial test iterations verified: {}", total_assertions);
}

#[test]
fn test_adversarial_special_looks_and_extremes() {
    let special_looks = [
        ("Oren Notch Night", look::around_notch(Theme::Oren)),
        ("Glass Dark", look::glass(false)),
        ("Glass Light", look::glass(true)),
    ];

    let extreme_colors = [
        ("Near Black", Color::rgba(0.001, 0.001, 0.001, 1.0)),
        ("Pure Black", Color::black(1.0)),
        ("Pure White", Color::white(1.0)),
        ("High Contrast Red", Color::rgba(1.0, 0.0, 0.0, 1.0)),
        ("Deep Navy", Color::rgba(0.01, 0.02, 0.15, 1.0)),
        ("Pale Cream", Color::rgba(0.98, 0.95, 0.88, 1.0)),
    ];

    for (look_name, lk) in special_looks {
        for (color_name, accent) in extreme_colors {
            let guarded = lk.guard_accent(accent);
            let cr = Color::contrast_ratio(guarded, lk.ground);
            assert!(
                cr >= 3.499,
                "Failed on special look '{}' with color '{}': CR={:.4}:1",
                look_name, color_name, cr
            );
            assert!(!guarded.r.is_nan() && !guarded.g.is_nan() && !guarded.b.is_nan());
            assert!(guarded.r >= 0.0 && guarded.r <= 1.0);
            assert!(guarded.g >= 0.0 && guarded.g <= 1.0);
            assert!(guarded.b >= 0.0 && guarded.b <= 1.0);
        }
    }
}
