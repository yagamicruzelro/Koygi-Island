use poni_core::look::{around_notch, glass, of, Material, GLASS_DARK, GLASS_LIGHT};
use poni_core::settings::{Settings, Theme};

#[test]
fn test_theme_serde_canonical_roundtrip() {
    let cases = [
        (Theme::Classic, r#""classic""#),
        (Theme::Oren, r#""oren""#),
        (Theme::Corgi, r#""corgi""#),
        (Theme::Pill, r#""pill""#),
        (Theme::OrenPill, r#""oren_pill""#),
        (Theme::CorgiPill, r#""corgi_pill""#),
    ];

    for (theme, expected_json) in cases {
        let serialized = serde_json::to_string(&theme).expect("Theme serialization failed");
        assert_eq!(serialized, expected_json, "Mismatch in canonical serialization of {:?}", theme);

        let deserialized: Theme = serde_json::from_str(&serialized).expect("Theme deserialization failed");
        assert_eq!(deserialized, theme, "Mismatch in round-trip deserialization of {:?}", theme);
    }
}

#[test]
fn test_theme_legacy_aliases_deserialization() {
    let alias_cases = [
        (r#""liquid_glass""#, Theme::Classic),
        (r#""glass""#, Theme::Classic),
        (r#""oyen""#, Theme::Oren),
        (r#""oren""#, Theme::Oren),
        (r#""oyen_pill""#, Theme::OrenPill),
        (r#""oren_pill""#, Theme::OrenPill),
        (r#""corgi""#, Theme::Corgi),
        (r#""corgi_pill""#, Theme::CorgiPill),
        (r#""pill""#, Theme::Pill),
        (r#""classic""#, Theme::Classic),
    ];

    for (json_str, expected_theme) in alias_cases {
        let parsed: Theme = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!("Failed to deserialize alias {json_str}: {e}"));
        assert_eq!(parsed, expected_theme, "Alias {json_str} must deserialize to {expected_theme:?}");
    }
}

#[test]
fn test_legacy_settings_json_configurations() {
    // 1. Settings containing legacy "liquid_glass" -> must fallback to Classic
    let legacy_liquid_glass = r#"{"settings_version":4,"theme":"liquid_glass","expanded_width":700.0}"#;
    let s: Settings = serde_json::from_str(legacy_liquid_glass).expect("Failed to parse liquid_glass in Settings");
    assert_eq!(s.theme, Theme::Classic, "Legacy 'liquid_glass' must deserialize to Theme::Classic in Settings");

    // 2. Settings containing legacy "glass" -> must fallback to Classic
    let legacy_glass = r#"{"settings_version":4,"theme":"glass","expanded_width":700.0}"#;
    let s: Settings = serde_json::from_str(legacy_glass).expect("Failed to parse glass in Settings");
    assert_eq!(s.theme, Theme::Classic, "Legacy 'glass' must deserialize to Theme::Classic in Settings");

    // 3. Settings containing legacy "oyen"
    let legacy_oyen = r#"{"settings_version":4,"theme":"oyen"}"#;
    let s: Settings = serde_json::from_str(legacy_oyen).expect("Failed to parse oyen in Settings");
    assert_eq!(s.theme, Theme::Oren, "Legacy 'oyen' must deserialize to Theme::Oren");

    // 4. Settings containing legacy "oyen_pill"
    let legacy_oyen_pill = r#"{"settings_version":4,"theme":"oyen_pill"}"#;
    let s: Settings = serde_json::from_str(legacy_oyen_pill).expect("Failed to parse oyen_pill in Settings");
    assert_eq!(s.theme, Theme::OrenPill, "Legacy 'oyen_pill' must deserialize to Theme::OrenPill");

    // 5. Settings omitting theme entirely -> must default to Classic
    let no_theme = r#"{"settings_version":4,"expanded_width":600.0}"#;
    let s: Settings = serde_json::from_str(no_theme).expect("Failed to parse json without theme");
    assert_eq!(s.theme, Theme::Classic, "Settings without theme must default to Theme::Classic");

    // 6. Unknown theme variant must fail deserialization without crashing/panicking
    let invalid_themes = [
        r#"{"theme":"unknown_theme"}"#,
        r#"{"theme":"Glass"}"#,
        r#"{"theme":"LIQUID_GLASS"}"#,
        r#"{"theme":123}"#,
        r#"{"theme":null}"#,
        r#"{"theme":[]}"#,
    ];
    for bad_json in invalid_themes {
        let res: Result<Settings, _> = serde_json::from_str(bad_json);
        assert!(res.is_err(), "Invalid theme payload '{bad_json}' must return Err");
    }
}

#[test]
fn test_theme_look_material_oracle() {
    let all_themes = [
        Theme::Classic,
        Theme::Oren,
        Theme::Corgi,
        Theme::Pill,
        Theme::OrenPill,
        Theme::CorgiPill,
    ];

    assert_eq!(Theme::ALL, all_themes);
    assert_eq!(Theme::all(), &all_themes[..]);

    for theme in all_themes {
        let look = of(theme);
        let notch_look = around_notch(theme);

        assert_eq!(
            look.material,
            Material::Solid,
            "of({:?}) must return Material::Solid",
            theme
        );
        assert_eq!(
            notch_look.material,
            Material::Solid,
            "around_notch({:?}) must return Material::Solid",
            theme
        );
    }

    // Dynamic glass light / dark look tokens
    assert_eq!(glass(true).material, Material::Glass);
    assert_eq!(glass(false).material, Material::Glass);
    assert_eq!(glass(true), &GLASS_LIGHT);
    assert_eq!(glass(false), &GLASS_DARK);
}
