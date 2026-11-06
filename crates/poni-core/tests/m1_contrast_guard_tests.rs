//! Milestone 1 Unit Tests: Requirement R1 Audio Equalizer Adaptive Contrast Guard
//! Verifies WCAG relative luminance, contrast ratio calculation, and adaptive contrast guarding
//! across all 6 strict themes and specialized looks.

use poni_core::geom::Color;
use poni_core::look;
use poni_core::settings::Theme;

#[test]
fn test_srgb_to_linear_and_wcag_luminance_properties() {
    // Exact black and white properties
    assert_eq!(Color::srgb_to_linear(0.0), 0.0);
    assert!((Color::srgb_to_linear(1.0) - 1.0).abs() < 1e-6);

    let black = Color::black(1.0);
    let white = Color::white(1.0);
    assert_eq!(black.relative_luminance(), 0.0);
    assert!((white.relative_luminance() - 1.0).abs() < 1e-6);

    // WCAG contrast ratio extremes
    let max_cr = Color::contrast_ratio(white, black);
    assert!((max_cr - 21.0).abs() < 1e-4, "White on black must be 21:1, got {}", max_cr);

    let same_cr = Color::contrast_ratio(white, white);
    assert!((same_cr - 1.0).abs() < 1e-6, "Color against itself must be 1:1, got {}", same_cr);

    // Monotonicity of sRGB to linear
    let mut prev = 0.0;
    for i in 1..=100 {
        let c = i as f32 / 100.0;
        let lin = Color::srgb_to_linear(c);
        assert!(lin >= prev, "Linearization must be monotonic");
        prev = lin;
    }
}

#[test]
fn test_adaptive_contrast_guard_across_all_themes() {
    let test_accents = [
        ("Vibrant Orange", Color::rgba(1.0, 0.55, 0.0, 1.0)),
        ("Bright Pink", Color::rgba(1.0, 0.2, 0.6, 1.0)),
        ("Yellow", Color::rgba(1.0, 0.85, 0.0, 1.0)),
        ("Cyan", Color::rgba(0.1, 0.8, 0.9, 1.0)),
        ("White", Color::white(1.0)),
        ("Pastel Green", Color::rgba(0.6, 0.9, 0.6, 1.0)),
        ("Deep Blue", Color::rgba(0.1, 0.2, 0.8, 1.0)),
    ];

    for theme in Theme::ALL {
        let look = look::of(theme);
        let ground = look.ground;

        for (name, accent) in test_accents {
            let guarded = look.guard_accent(accent);
            let cr = Color::contrast_ratio(guarded, ground);

            assert!(
                cr >= 3.499,
                "Failed contrast guard for theme {:?} ({}) with accent '{}' ({:?}): CR was {:.3}:1 (guarded: {:?}, ground: {:?})",
                theme,
                if ground.relative_luminance() >= 0.25 { "light" } else { "dark" },
                name,
                accent,
                cr,
                guarded,
                ground
            );

            // On light grounds, verify strict Hue preservation via proportional RGB scaling
            if ground.relative_luminance() >= 0.25 {
                // If the color was darkened, verify each channel was scaled by the same factor k
                if guarded != accent {
                    // Find a non-zero reference channel to deduce k
                    let k = if accent.r > 0.05 {
                        guarded.r / accent.r
                    } else if accent.g > 0.05 {
                        guarded.g / accent.g
                    } else {
                        guarded.b / accent.b
                    };

                    assert!(k > 0.0 && k <= 1.0, "Scale factor k must be in (0, 1], got {}", k);

                    // Check proportional channels
                    assert!((guarded.r - accent.r * k).abs() < 1e-4, "R channel distorted");
                    assert!((guarded.g - accent.g * k).abs() < 1e-4, "G channel distorted");
                    assert!((guarded.b - accent.b * k).abs() < 1e-4, "B channel distorted");

                    // Check hue ratio invariance between channels
                    if accent.r > 0.05 && accent.g > 0.05 {
                        let orig_rg = accent.r / accent.g;
                        let guard_rg = guarded.r / guarded.g;
                        assert!(
                            (orig_rg - guard_rg).abs() < 1e-4,
                            "Hue ratio R/G distorted for '{}' on theme {:?}: orig {}, guarded {}",
                            name,
                            theme,
                            orig_rg,
                            guard_rg
                        );
                    }
                    if accent.g > 0.05 && accent.b > 0.05 {
                        let orig_gb = accent.g / accent.b;
                        let guard_gb = guarded.g / guarded.b;
                        assert!(
                            (orig_gb - guard_gb).abs() < 1e-4,
                            "Hue ratio G/B distorted for '{}' on theme {:?}: orig {}, guarded {}",
                            name,
                            theme,
                            orig_gb,
                            guard_gb
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_alpha_channel_preservation() {
    let ground_light = look::of(Theme::Oren).ground;
    let ground_dark = look::of(Theme::Classic).ground;

    let transparent_orange = Color::rgba(1.0, 0.55, 0.0, 0.45);
    let guarded_light = transparent_orange.adapt_contrast(ground_light, 3.5);
    assert_eq!(guarded_light.a, 0.45, "Alpha channel must be preserved on light ground");

    let dark_navy = Color::rgba(0.02, 0.02, 0.08, 0.75);
    let guarded_dark = dark_navy.adapt_contrast(ground_dark, 3.5);
    assert_eq!(guarded_dark.a, 0.75, "Alpha channel must be preserved on dark ground");
}

#[test]
fn test_special_looks_notch_and_glass() {
    let test_accents = [
        ("Vibrant Orange", Color::rgba(1.0, 0.55, 0.0, 1.0)),
        ("Bright Pink", Color::rgba(1.0, 0.2, 0.6, 1.0)),
        ("Yellow", Color::rgba(1.0, 0.85, 0.0, 1.0)),
        ("Cyan", Color::rgba(0.1, 0.8, 0.9, 1.0)),
        ("White", Color::white(1.0)),
        ("Pastel Green", Color::rgba(0.6, 0.9, 0.6, 1.0)),
        ("Deep Blue", Color::rgba(0.1, 0.2, 0.8, 1.0)),
    ];

    let special_looks = [
        ("Oren Notch Night", look::around_notch(Theme::Oren)),
        ("Glass Light", look::glass(true)),
        ("Glass Dark", look::glass(false)),
    ];

    for (look_name, look) in special_looks {
        for (name, accent) in test_accents {
            let guarded = look.guard_accent(accent);
            let cr = Color::contrast_ratio(guarded, look.ground);
            assert!(
                cr >= 3.499,
                "Failed contrast guard for look '{}' with accent '{}': CR was {:.3}:1",
                look_name,
                name,
                cr
            );
        }
    }
}
