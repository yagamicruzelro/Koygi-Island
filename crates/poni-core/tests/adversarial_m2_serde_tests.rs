//! Adversarial Serde Stress Test Suite for Milestone 2
//! Requirement R2: Complete Rebranding and Purge of Oyenza to koygi
//!
//! Independently verifies:
//! 1. Deserialization of legacy settings.json payloads containing `oyenza_show_amounts: true/false`.
//! 2. Deserialization of legacy data.json payloads containing `oyenza_noticed: ["v1", "v2"]`.
//! 3. Preservation of values into `koygi_show_amounts` and `koygi_noticed`.
//! 4. Resilience against missing keys, empty payloads `{}`, extra unexpected keys.
//! 5. Serialization roundtrips asserting output emits modern `koygi_*` keys, never `oyenza_*`.

use poni_core::settings::Settings;
use poni_core::store::Store;

#[test]
fn test_adversarial_legacy_settings_show_amounts_resilience() {
    // Case 1: Legacy payload with "oyenza_show_amounts": true
    let legacy_true = r#"{
        "theme": "oren",
        "oyenza_show_amounts": true,
        "media_theme_tint": true
    }"#;
    let s1: Settings = serde_json::from_str(legacy_true).expect("Deserialization of legacy_true must succeed");
    assert!(s1.koygi_show_amounts, "Legacy oyenza_show_amounts: true must map to koygi_show_amounts: true");

    // Case 2: Legacy payload with "oyenza_show_amounts": false
    let legacy_false = r#"{
        "theme": "classic",
        "oyenza_show_amounts": false
    }"#;
    let s2: Settings = serde_json::from_str(legacy_false).expect("Deserialization of legacy_false must succeed");
    assert!(!s2.koygi_show_amounts, "Legacy oyenza_show_amounts: false must map to koygi_show_amounts: false");

    // Case 3: Modern payload with "koygi_show_amounts": true
    let modern_true = r#"{
        "theme": "corgi",
        "koygi_show_amounts": true
    }"#;
    let s3: Settings = serde_json::from_str(modern_true).expect("Deserialization of modern_true must succeed");
    assert!(s3.koygi_show_amounts, "Modern koygi_show_amounts: true must deserialize correctly");

    // Case 4: Missing show_amounts key entirely (empty json)
    let empty_payload = r#"{}"#;
    let s4: Settings = serde_json::from_str(empty_payload).expect("Deserialization of empty payload must succeed");
    assert!(!s4.koygi_show_amounts, "Default koygi_show_amounts must be false when missing");

    // Case 5: Payload with unknown/deprecated keys and legacy page names
    let complex_legacy = r#"{
        "version": 1,
        "theme": "pill",
        "oyenza_show_amounts": true,
        "start_page": "oyenza",
        "hidden_tabs": ["oyenza", "ai", "script", "notes", "media"],
        "unexpected_future_key": 42,
        "another_legacy_blob": {"nested": [1, 2, 3]}
    }"#;
    let s5: Settings = serde_json::from_str(complex_legacy).expect("Deserialization of complex legacy payload must succeed");
    assert!(s5.koygi_show_amounts, "Must preserve koygi_show_amounts: true even with unknown keys");
    assert_eq!(s5.start_page, "oyenza");
    assert_eq!(s5.hidden_tabs, vec!["oyenza", "ai", "script", "notes", "media"]);

    // Case 6: Modern serialization must emit "koygi_show_amounts", NEVER "oyenza_show_amounts"
    let serialized = serde_json::to_string(&s1).expect("Serialization must succeed");
    assert!(serialized.contains(r#""koygi_show_amounts":true"#), "Must emit koygi_show_amounts");
    assert!(!serialized.contains("oyenza_show_amounts"), "Must NOT emit oyenza_show_amounts");
}

#[test]
fn test_adversarial_legacy_store_noticed_resilience() {
    // Case 1: Legacy data.json with "oyenza_noticed": ["bill_1", "bill_2"]
    let legacy_noticed = r#"{
        "version": 1,
        "oyenza_noticed": ["rent_2026_11", "spotify_2026_11"],
        "last_page": "today"
    }"#;
    let store1: Store = serde_json::from_str(legacy_noticed).expect("Deserialization of legacy data.json must succeed");
    assert_eq!(
        store1.koygi_noticed,
        vec!["rent_2026_11", "spotify_2026_11"],
        "Legacy oyenza_noticed array must be preserved in koygi_noticed"
    );

    // Case 2: Modern data.json with "koygi_noticed"
    let modern_noticed = r#"{
        "version": 1,
        "koygi_noticed": ["electric_2026_11"],
        "last_page": "koygi"
    }"#;
    let store2: Store = serde_json::from_str(modern_noticed).expect("Deserialization of modern data.json must succeed");
    assert_eq!(store2.koygi_noticed, vec!["electric_2026_11"]);

    // Case 3: Empty payload `{}`
    let empty_store: Store = serde_json::from_str(r#"{}"#).expect("Deserialization of empty store must succeed");
    assert!(empty_store.koygi_noticed.is_empty(), "Empty payload must result in empty koygi_noticed");

    // Case 4: Missing koygi_noticed / oyenza_noticed field amidst other store fields
    let partial_store = r#"{
        "version": 1,
        "last_page": "oyenza",
        "recorder_island": true,
        "tasks": {"items": []}
    }"#;
    let store3: Store = serde_json::from_str(partial_store).expect("Deserialization of partial store must succeed");
    assert!(store3.koygi_noticed.is_empty());
    assert_eq!(store3.last_page, "oyenza");
    assert!(store3.recorder_island);

    // Case 5: Serialized modern store must emit "koygi_noticed", NEVER "oyenza_noticed"
    let serialized_store = serde_json::to_string(&store1).expect("Serialization of store must succeed");
    assert!(serialized_store.contains(r#""koygi_noticed":["rent_2026_11","spotify_2026_11"]"#));
    assert!(!serialized_store.contains("oyenza_noticed"), "Must NOT emit oyenza_noticed");
}

#[test]
fn test_adversarial_serde_stress_combinatorial_payloads() {
    // 1. Payloads that MUST deserialize successfully
    let valid_payloads = [
        (r#"{"oyenza_show_amounts": true}"#, true),
        (r#"{"oyenza_show_amounts": false}"#, false),
        (r#"{"koygi_show_amounts": true}"#, true),
        (r#"{"koygi_show_amounts": false}"#, false),
        (r#"{}"#, false),
    ];

    for (payload, expected) in valid_payloads {
        let res: Result<Settings, _> = serde_json::from_str(payload);
        assert!(res.is_ok(), "Payload '{}' failed deserialization: {:?}", payload, res.err());
        assert_eq!(res.unwrap().koygi_show_amounts, expected);
    }

    // 2. Both keys present in the same JSON payload: Serde alias semantics define this as a duplicate field
    let duplicate_payloads = [
        r#"{"oyenza_show_amounts": true, "koygi_show_amounts": false}"#,
        r#"{"koygi_show_amounts": true, "oyenza_show_amounts": false}"#,
    ];
    for payload in duplicate_payloads {
        let res: Result<Settings, _> = serde_json::from_str(payload);
        assert!(res.is_err(), "Serde must reject conflicting duplicate alias fields in same object: {}", payload);
        let err_msg = format!("{}", res.err().unwrap());
        assert!(err_msg.contains("duplicate field"), "Error should report duplicate field, got: {}", err_msg);
    }
}
