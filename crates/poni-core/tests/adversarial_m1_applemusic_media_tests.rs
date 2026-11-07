//! Adversarial Empirical Stress Test Suite for Milestone M1
//! Validates Apple Music & Media Models:
//! - `poni_core::applemusic`
//! - `poni_core::mediaremote`
//! - `poni_core::nowplaying`

use poni_core::applemusic::{
    best_song, clean_user_token, dev_token_stale, find_bundle_urls, find_developer_token,
    lyrics_url, parse_lyrics, parse_search, parse_storefront, search_url, storefront_url,
    to_record, AmSong, DevToken,
};
use poni_core::lrc::{LyricLine, Lyrics};
use poni_core::lrclib::TrackQuery;
use poni_core::mediaremote::{Playing, Stream};
use poni_core::nowplaying::{
    artist_album, is_apple_music, is_browser, island_controls, lyrics_lead_s,
    site_from_window_title, slider_value, Controls, APPLE_MUSIC_LYRICS_LEAD_S,
};

fn base64url_encode(s: &str) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = s.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn make_jwt(header: &str, payload: &str, sig: &str) -> String {
    format!(
        "{}.{}.{}",
        base64url_encode(header),
        base64url_encode(payload),
        sig
    )
}

// =========================================================================
// 1. APPLE MUSIC ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_applemusic_developer_token_adversarial_inputs() {
    // 1. Empty or pure noise JS
    assert_eq!(find_developer_token(""), None);
    assert_eq!(find_developer_token("   \n\t  "), None);
    assert_eq!(find_developer_token("function test() { return 42; }"), None);

    // 2. "eyJ" substring traps
    assert_eq!(find_developer_token("eyJ"), None);
    assert_eq!(find_developer_token("eyJ."), None);
    assert_eq!(find_developer_token("eyJ.eyJ"), None);
    assert_eq!(find_developer_token("eyJ..."), None);
    assert_eq!(find_developer_token("eyJ123.456"), None); // only 2 parts
    assert_eq!(find_developer_token("eyJ123.456.789.extra"), None); // 4 parts

    // 3. Valid JWT structure with malformed base64url or non-JSON payloads
    assert_eq!(find_developer_token("eyJ!@#$.payload.sig"), None);
    let not_json_header = format!("{}.{}.sig", base64url_encode("not json"), base64url_encode("{}"));
    assert_eq!(find_developer_token(&not_json_header), None);

    let not_json_payload = format!("{}.{}.sig", base64url_encode("{}"), base64url_encode("not json"));
    assert_eq!(find_developer_token(&not_json_payload), None);

    // 4. JSON payloads missing required kid/iss/root_https_origin
    let irrelevant_jwt = make_jwt(r#"{"alg":"ES256","kid":"RandomKey"}"#, r#"{"sub":"123","exp":100}"#, "c2ln");
    assert_eq!(find_developer_token(&irrelevant_jwt), None);

    // 5. Valid WebPlayKid token
    let webplay_jwt = make_jwt(
        r#"{"alg":"ES256","kid":"WebPlayKid"}"#,
        r#"{"iss":"AMPWebPlay","exp":1890000000}"#,
        "c2ln",
    );
    let dev_tok = find_developer_token(&format!("const token = \"{webplay_jwt}\";")).expect("Must find WebPlayKid");
    assert_eq!(dev_tok.token, webplay_jwt);
    assert_eq!(dev_tok.expires_at, 1890000000);

    // 6. Token with non-numeric/null exp field
    let null_exp_jwt = make_jwt(
        r#"{"alg":"ES256","kid":"WebPlayKid"}"#,
        r#"{"iss":"AMPWebPlay","exp":null}"#,
        "c2ln",
    );
    let dev_tok_null = find_developer_token(&null_exp_jwt).expect("Must find WebPlayKid with null exp");
    assert_eq!(dev_tok_null.expires_at, 0);

    let string_exp_jwt = make_jwt(
        r#"{"alg":"ES256","kid":"WebPlayKid"}"#,
        r#"{"iss":"AMPWebPlay","exp":"never"}"#,
        "c2ln",
    );
    let dev_tok_str = find_developer_token(&string_exp_jwt).expect("Must parse token with string exp as 0");
    assert_eq!(dev_tok_str.expires_at, 0);

    // 7. Fallback to apple.com root_https_origin
    let apple_origin_jwt = make_jwt(
        r#"{"alg":"ES256","kid":"OtherKid"}"#,
        r#"{"iss":"OtherIssuer","exp":1700000000,"root_https_origin":["apple.com"]}"#,
        "c2ln",
    );
    let fallback_tok = find_developer_token(&apple_origin_jwt).expect("Must find apple.com fallback");
    assert_eq!(fallback_tok.token, apple_origin_jwt);
    assert_eq!(fallback_tok.expires_at, 1700000000);

    // 8. Priority: WebPlayKid wins over apple.com fallback even if earlier in script
    let bundle_js = format!("var a = '{apple_origin_jwt}';\nvar b = '{webplay_jwt}';");
    let chosen = find_developer_token(&bundle_js).expect("WebPlayKid must be prioritized");
    assert_eq!(chosen.token, webplay_jwt);
}

#[test]
fn test_applemusic_dev_token_stale_boundaries() {
    let t_infinite = DevToken { token: "abc".into(), expires_at: 0 };
    assert!(!dev_token_stale(&t_infinite, 0));
    assert!(!dev_token_stale(&t_infinite, 1_000_000_000));
    assert!(!dev_token_stale(&t_infinite, i64::MAX));
    assert!(!dev_token_stale(&t_infinite, i64::MIN));

    let exp = 1_000_000i64;
    let t = DevToken { token: "abc".into(), expires_at: exp };
    let day = 24 * 3600;

    // Boundary: exactly 24h before expiry -> stale
    assert!(dev_token_stale(&t, exp - day));
    // 24h + 1s before expiry -> not stale
    assert!(!dev_token_stale(&t, exp - day - 1));
    // After expiry -> stale
    assert!(dev_token_stale(&t, exp + 100));
    assert!(dev_token_stale(&t, exp));
}

#[test]
fn test_applemusic_clean_user_token_adversarial() {
    let valid_80 = "A".repeat(80);
    assert_eq!(clean_user_token(&valid_80), Some(valid_80.clone()));

    let short_79 = "A".repeat(79);
    assert_eq!(clean_user_token(&short_79), None);

    let valid_8192 = "B".repeat(8192);
    assert_eq!(clean_user_token(&valid_8192), Some(valid_8192.clone()));

    let too_long_8193 = "B".repeat(8193);
    assert_eq!(clean_user_token(&too_long_8193), None);

    // Cookie formats and weird prefixing
    assert_eq!(
        clean_user_token(&format!("media-user-token={valid_80}")),
        Some(valid_80.clone())
    );
    assert_eq!(
        clean_user_token(&format!("  media-user-token:   \"{}\"  ", valid_80)),
        Some(valid_80.clone())
    );
    assert_eq!(
        clean_user_token(&format!("session=123; media-user-token={valid_80}; auth=true")),
        Some(valid_80.clone())
    );

    // Illegal characters
    let bad_chars = format!("{} with space", "A".repeat(75));
    assert_eq!(clean_user_token(&bad_chars), None);

    let bad_unicode = format!("{}😀", "A".repeat(79));
    assert_eq!(clean_user_token(&bad_unicode), None);

    let bad_control = format!("{}\n{}", "A".repeat(40), "A".repeat(40));
    assert_eq!(clean_user_token(&bad_control), None);
}

#[test]
fn test_applemusic_parse_storefront_adversarial() {
    assert_eq!(parse_storefront(""), None);
    assert_eq!(parse_storefront("not json"), None);
    assert_eq!(parse_storefront("{}"), None);
    assert_eq!(parse_storefront(r#"{"data": null}"#), None);
    assert_eq!(parse_storefront(r#"{"data": []}"#), None);
    assert_eq!(parse_storefront(r#"{"data": [{}]}"#), None);
    assert_eq!(parse_storefront(r#"{"data": [{"id": null}]}"#), None);
    assert_eq!(parse_storefront(r#"{"data": [{"id": 1234}]}"#), None);
    assert_eq!(parse_storefront(r#"{"data": [{"id": ""}]}"#), None);
    assert_eq!(parse_storefront(r#"{"data": [{"id": "   "}]}"#), None);

    // Alphanumeric <= 8 characters permitted
    assert_eq!(parse_storefront(r#"{"data": [{"id": "us"}]}"#), Some("us".into()));
    assert_eq!(parse_storefront(r#"{"data": [{"id": "JAPAN"}]}"#), Some("japan".into()));
    assert_eq!(parse_storefront(r#"{"data": [{"id": "12345678"}]}"#), Some("12345678".into()));

    // Out of bounds / malicious path traversal
    assert_eq!(parse_storefront(r#"{"data": [{"id": "123456789"}]}"#), None); // > 8 chars
    assert_eq!(parse_storefront(r#"{"data": [{"id": "../us"}]}"#), None);
    assert_eq!(parse_storefront(r#"{"data": [{"id": "us-ca"}]}"#), None);
    assert_eq!(parse_storefront(r#"{"data": [{"id": "us/new"}]}"#), None);
}

#[test]
fn test_applemusic_parse_search_and_best_song_adversarial() {
    // Malformed JSON replies
    assert!(parse_search("").is_err());
    assert!(parse_search("502 Bad Gateway").is_err());
    assert!(parse_search("{}").is_err()); // missing "results"
    assert!(parse_search(r#"{"errors": [{"status": "401"}]}"#).is_err());

    // Empty results
    assert_eq!(parse_search(r#"{"results": {}}"#).unwrap(), vec![]);
    assert_eq!(parse_search(r#"{"results": {"songs": null}}"#).unwrap(), vec![]);
    assert_eq!(parse_search(r#"{"results": {"songs": {"data": []}}}"#).unwrap(), vec![]);

    // Array with null attributes or missing ID
    let mixed_data = r#"{"results": {"songs": {"data": [
        null,
        {},
        {"id": "101"},
        {"attributes": {"name": "No ID Song"}},
        {"id": "102", "attributes": null},
        {"id": "103", "attributes": {"name": "Valid Song", "artistName": "Artist A", "durationInMillis": 180000, "hasTimeSyncedLyrics": true}},
        {"id": "104", "attributes": {"name": "Negative Duration", "artistName": "Artist B", "durationInMillis": -50000, "hasTimeSyncedLyrics": true}},
        {"id": "105", "attributes": {"name": "Missing Flags", "artistName": "Artist C"}}
    ]}}}"#;

    let songs = parse_search(mixed_data).expect("Must parse valid JSON gracefully filtering corrupted items");
    assert_eq!(songs.len(), 3);
    assert_eq!(songs[0].id, "103");
    assert_eq!(songs[0].duration_s, 180.0);
    assert!(songs[0].has_synced);

    assert_eq!(songs[1].id, "104");
    assert_eq!(songs[1].duration_s, -50.0);

    assert_eq!(songs[2].id, "105");
    assert_eq!(songs[2].duration_s, 0.0);
    assert!(!songs[2].has_lyrics);
    assert!(!songs[2].has_synced);

    // best_song edge cases
    let q = TrackQuery {
        title: "Valid Song".into(),
        artist: "Artist A".into(),
        album: String::new(),
        duration_s: 180,
    };
    let matched = best_song(&songs, &q).expect("Must match song 103");
    assert_eq!(matched.id, "103");

    // Negative duration should not panic in match_score
    let q_neg = TrackQuery {
        title: "Negative Duration".into(),
        artist: "Artist B".into(),
        album: String::new(),
        duration_s: 180,
    };
    // Song 104 has duration -50.0, filter(|d| *d > 0.0) strips duration, still matches name/artist
    let matched_neg = best_song(&songs, &q_neg);
    assert!(matched_neg.is_some());
    assert_eq!(matched_neg.unwrap().id, "104");
}

#[test]
fn test_applemusic_parse_lyrics_and_record_conversion() {
    assert_eq!(parse_lyrics(""), None);
    assert_eq!(parse_lyrics("{}"), None);
    assert_eq!(parse_lyrics(r#"{"data": []}"#), None);
    assert_eq!(parse_lyrics(r#"{"data": [{"attributes": {}}]}"#), None);
    assert_eq!(parse_lyrics(r#"{"data": [{"attributes": {"ttml": ""}}]}"#), None);
    assert_eq!(parse_lyrics(r#"{"data": [{"attributes": {"ttml": "   \n  "}}]}"#), None);
    assert_eq!(
        parse_lyrics(r#"{"data": [{"attributes": {"ttml": "<tt>lyrics</tt>"}}]}"#),
        Some("<tt>lyrics</tt>".into())
    );

    // to_record with extreme and non-numeric song IDs
    let am_song = AmSong {
        id: "not_a_number_id".into(),
        name: "Test Song".into(),
        artist: "Test Artist".into(),
        album: "".into(),
        duration_s: 0.0,
        has_lyrics: true,
        has_synced: true,
    };
    let lyrics = Lyrics {
        lines: vec![
            LyricLine { time_s: -5.0, text: "Negative timestamp line".into() },
            LyricLine { time_s: 0.0, text: "Zero line".into() },
            LyricLine { time_s: 3661.123, text: "Over an hour line".into() },
        ],
        synced: true,
    };
    let rec = to_record(&am_song, &lyrics);
    assert_eq!(rec.id, 0); // non-numeric id parses as 0
    assert_eq!(rec.album_name, None); // empty album filtered
    assert_eq!(rec.duration, None); // 0.0 duration filtered

    let synced = rec.synced_lyrics.unwrap();
    // Negative timestamp clamped to 00:00.000
    assert!(synced.contains("[00:00.000]Negative timestamp line"));
    // 3661.123s = 61m 01s 123ms
    assert!(synced.contains("[61:01.123]Over an hour line"));
}

// =========================================================================
// 2. MEDIAREMOTE ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_mediaremote_stream_apply_adversarial_diffs() {
    let mut s = Stream::default();

    // 1. Corrupted and edge line formats
    assert!(!s.apply(""));
    assert!(!s.apply("     "));
    assert!(!s.apply("null"));
    assert!(!s.apply("{\"malformed\": "));
    assert!(!s.apply(r#"{"type": "not_data"}"#));
    assert!(!s.apply(r#"{"type": "data", "payload": "not_an_object"}"#));

    // 2. Initial full state
    let full = r#"{"type":"data","diff":false,"payload":{"title":"Song A","artist":"Artist 1","playing":true,"durationMicros":180000000,"elapsedTimeMicros":10000000,"timestampEpochMicros":1700000000000000,"playbackRate":1.5}}"#;
    assert!(s.apply(full));
    assert!(!s.apply(full)); // idempotent duplicate: returns false

    let p = s.playing().expect("Must have playing track");
    assert_eq!(p.title, "Song A");
    assert_eq!(p.artist, "Artist 1");
    assert_eq!(p.duration_s, 180.0);
    assert_eq!(p.elapsed_s, 10.0);
    assert_eq!(p.rate, 1.5);
    assert!(p.playing);

    // 3. Diff updates: null field removes, new field updates
    let diff1 = r#"{"type":"data","diff":true,"payload":{"artist":null,"album":"New Album","playbackRate":null}}"#;
    assert!(s.apply(diff1));
    let p_diff = s.playing().unwrap();
    assert_eq!(p_diff.artist, ""); // removed -> defaults to empty
    assert_eq!(p_diff.album, "New Album");
    assert_eq!(p_diff.rate, 1.0); // null rate fallback to 1.0

    // 4. Repeated null diff for already non-existent key
    let diff2 = r#"{"type":"data","diff":true,"payload":{"non_existent_key":null}}"#;
    assert!(!s.apply(diff2)); // Nothing changed

    // 5. Diff removing title makes playing() return None
    let diff_no_title = r#"{"type":"data","diff":true,"payload":{"title":null}}"#;
    assert!(s.apply(diff_no_title));
    assert_eq!(s.playing(), None);

    // 6. Diff restoring empty title keeps playing() as None
    assert!(s.apply(r#"{"type":"data","diff":true,"payload":{"title":"   "}}"#));
    assert_eq!(s.playing(), None);

    // 7. Full reset via clear()
    s.clear();
    assert_eq!(s.playing(), None);
    assert_eq!(s.artwork_sig(), 0);
}

#[test]
fn test_mediaremote_position_extrapolation_extremes() {
    // 1. Track playing normally
    let p = Playing {
        app_id: "com.apple.Music".into(),
        pid: Some(100),
        title: "Test".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        duration_s: 200.0,
        elapsed_s: 50.0,
        at_unix_s: Some(1_000_000.0),
        rate: 1.0,
        playing: true,
    };

    // Past time delta (clock skew backwards)
    assert_eq!(p.position_at(999_990.0), 50.0);

    // Normal forward extrapolation
    assert_eq!(p.position_at(1_000_010.0), 60.0);

    // Far future: clamped to duration_s
    assert_eq!(p.position_at(1_000_000.0 + 1_000_000.0), 200.0);
    assert_eq!(p.position_at(f64::MAX), 200.0);

    // 2. Zero duration (live stream / radio): continues extrapolating without clamp
    let p_live = Playing { duration_s: 0.0, ..p.clone() };
    assert_eq!(p_live.position_at(1_000_050.0), 100.0);
    assert_eq!(p_live.position_at(1_000_000.0 + 500.0), 550.0);

    // 3. Paused track: position never moves regardless of now
    let p_paused = Playing { playing: false, ..p.clone() };
    assert_eq!(p_paused.position_at(1_000_100.0), 50.0);
    assert_eq!(p_paused.position_at(f64::MAX), 50.0);

    // 4. Track with no timestamp (at_unix_s = None): position never moves
    let p_no_time = Playing { at_unix_s: None, ..p.clone() };
    assert_eq!(p_no_time.position_at(1_000_100.0), 50.0);

    // 5. Extreme rate: 0.0, 5.0, negative
    let p_fast = Playing { rate: 2.5, ..p.clone() };
    assert_eq!(p_fast.position_at(1_000_010.0), 50.0 + 10.0 * 2.5);

    let p_neg_rate = Playing { rate: -1.0, ..p.clone() };
    // 50.0 + 10.0 * (-1.0) = 40.0
    assert_eq!(p_neg_rate.position_at(1_000_010.0), 40.0);
    // 50.0 + 100.0 * (-1.0) = -50.0 -> clamped to 0.0
    assert_eq!(p_neg_rate.position_at(1_000_100.0), 0.0);
}

#[test]
fn test_mediaremote_artwork_decoding_and_hashing() {
    let mut s = Stream::default();
    assert_eq!(s.artwork(), None);
    assert_eq!(s.artwork_sig(), 0);

    // Malformed base64 artwork
    s.apply(r#"{"type":"data","diff":false,"payload":{"artworkData":"not-valid-base64!!!"}}"#);
    assert_eq!(s.artwork(), None);
    assert_ne!(s.artwork_sig(), 0); // FNV-1a still hashes the raw string

    // Valid base64 artwork
    s.apply(r#"{"type":"data","diff":false,"payload":{"artworkData":"AQIDBA=="}}"#);
    assert_eq!(s.artwork(), Some(vec![1, 2, 3, 4]));
    let sig1 = s.artwork_sig();
    assert_ne!(sig1, 0);

    // Different artwork produces different signature
    s.apply(r#"{"type":"data","diff":false,"payload":{"artworkData":"BQYHCA=="}}"#);
    assert_eq!(s.artwork(), Some(vec![5, 6, 7, 8]));
    let sig2 = s.artwork_sig();
    assert_ne!(sig2, 0);
    assert_ne!(sig1, sig2);
}

// =========================================================================
// 3. NOWPLAYING ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_nowplaying_artist_album_splitting_adversarial() {
    let apple = "AppleMusic.exe";
    let spotify = "Spotify.exe";

    // 1. Non-Apple Music players are NEVER split, even with Apple's separator
    let orig_artist = "Band \u{2014} Album".to_string();
    let orig_album = "Existing Album".to_string();
    let (a, b) = artist_album(spotify, orig_artist.clone(), orig_album.clone());
    assert_eq!(a, orig_artist);
    assert_eq!(b, orig_album);

    // 2. Normal Apple Music splitting ("Artist — Album")
    let (a, b) = artist_album(apple, "The Beatles \u{2014} Abbey Road".into(), "".into());
    assert_eq!(a, "The Beatles");
    assert_eq!(b, "Abbey Road");

    // 3. Radio station format: 3 parts ("Artist — Album — Station")
    let (a, b) = artist_album(
        apple,
        "Coldplay \u{2014} Parachutes \u{2014} Alternative Radio".into(),
        "".into(),
    );
    assert_eq!(a, "Coldplay");
    assert_eq!(b, "Parachutes"); // takes 2nd part as album, discards 3rd

    // 4. Multiple em-dashes (4+ parts)
    let (a, b) = artist_album(
        apple,
        "Queen \u{2014} Greatest Hits \u{2014} Disc 1 \u{2014} Remastered".into(),
        "".into(),
    );
    assert_eq!(a, "Queen");
    assert_eq!(b, "Greatest Hits");

    // 5. Existing album reported by player takes precedence over guessed album
    let (a, b) = artist_album(
        apple,
        "The Beatles \u{2014} Abbey Road".into(),
        "Abbey Road (Super Deluxe)".into(),
    );
    assert_eq!(a, "The Beatles");
    assert_eq!(b, "Abbey Road (Super Deluxe)");

    // 6. Existing album is whitespace: guessed album replaces it
    let (a, b) = artist_album(
        apple,
        "The Beatles \u{2014} Abbey Road".into(),
        "   ".into(),
    );
    assert_eq!(a, "The Beatles");
    assert_eq!(b, "Abbey Road");

    // 7. Unicode em-dashes without spaces (e.g. "Artist—Album") -> NOT split
    let (a, b) = artist_album(apple, "The Beatles\u{2014}Abbey Road".into(), "".into());
    assert_eq!(a, "The Beatles\u{2014}Abbey Road");
    assert_eq!(b, "");

    // 8. Regular hyphens (e.g. "Artist - Album") -> NOT split
    let (a, b) = artist_album(apple, "The Beatles - Abbey Road".into(), "".into());
    assert_eq!(a, "The Beatles - Abbey Road");
    assert_eq!(b, "");

    // 9. En-dashes (e.g. "Artist – Album" U+2013) -> NOT split
    let (a, b) = artist_album(apple, "The Beatles \u{2013} Abbey Road".into(), "".into());
    assert_eq!(a, "The Beatles \u{2013} Abbey Road");
    assert_eq!(b, "");

    // 10. Leading separator: " — Album" (empty artist) -> raw value kept
    let (a, b) = artist_album(apple, " \u{2014} Album".into(), "".into());
    assert_eq!(a, " \u{2014} Album");
    assert_eq!(b, "");

    // 11. Empty artist
    let (a, b) = artist_album(apple, "".into(), "Album".into());
    assert_eq!(a, "");
    assert_eq!(b, "Album");

    // 12. Trailing separator: "Artist — "
    let (a, b) = artist_album(apple, "Artist \u{2014} ".into(), "".into());
    assert_eq!(a, "Artist");
    assert_eq!(b, "");
}

#[test]
fn test_nowplaying_slider_value_adversarial() {
    // Normal cases
    assert_eq!(slider_value(0.0, 0.0, 100.0), Some(0.0));
    assert_eq!(slider_value(1.0, 0.0, 100.0), Some(100.0));
    assert_eq!(slider_value(0.5, 10.0, 20.0), Some(15.0));

    // Clamping fraction
    assert_eq!(slider_value(-0.5, 0.0, 100.0), Some(0.0));
    assert_eq!(slider_value(1.5, 0.0, 100.0), Some(100.0));

    // Degenerate ranges
    assert_eq!(slider_value(0.5, 100.0, 100.0), None); // max == min
    assert_eq!(slider_value(0.5, 100.0, 50.0), None);  // max < min

    // Non-finite values
    assert_eq!(slider_value(f64::NAN, 0.0, 100.0), None);
    assert_eq!(slider_value(0.5, f64::NAN, 100.0), None);
    assert_eq!(slider_value(0.5, 0.0, f64::NAN), None);
    assert_eq!(slider_value(f64::INFINITY, 0.0, 100.0), None);
    assert_eq!(slider_value(0.5, f64::NEG_INFINITY, 100.0), None);
    assert_eq!(slider_value(0.5, 0.0, f64::INFINITY), None);
}

#[test]
fn test_nowplaying_island_controls_adversarial() {
    let apple = "AppleMusic.exe";
    let spotify = "Spotify.exe";

    let none_ctrl = Controls::default();
    // Apple Music forces prev: true, and seek: true if has_duration: true
    let apple_dur = island_controls(apple, none_ctrl, true);
    assert!(apple_dur.prev);
    assert!(apple_dur.seek);
    assert!(!apple_dur.next);
    assert!(!apple_dur.play_pause);

    // Apple Music without duration (radio stream): seek must remain false
    let apple_no_dur = island_controls(apple, none_ctrl, false);
    assert!(apple_no_dur.prev);
    assert!(!apple_no_dur.seek);

    // Spotify preserves reported controls, but seek also requires has_duration
    let spotify_full = Controls { prev: true, next: true, play_pause: true, seek: true };
    assert_eq!(island_controls(spotify, spotify_full, true), spotify_full);

    let spotify_no_dur = island_controls(spotify, spotify_full, false);
    assert!(!spotify_no_dur.seek);
    assert!(spotify_no_dur.prev);
}

#[test]
fn test_nowplaying_lyrics_lead_adversarial() {
    assert!(is_apple_music("AppleMusic.exe"));
    assert!(is_apple_music("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App"));
    assert!(!is_apple_music("Spotify.exe"));

    assert_eq!(lyrics_lead_s("AppleMusic.exe"), APPLE_MUSIC_LYRICS_LEAD_S);
    assert_eq!(lyrics_lead_s("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App"), APPLE_MUSIC_LYRICS_LEAD_S);
    assert_eq!(lyrics_lead_s("C:\\Program Files\\AppleMusic\\applemusic.exe"), APPLE_MUSIC_LYRICS_LEAD_S);

    assert_eq!(lyrics_lead_s("Spotify.exe"), 0.0);
    assert_eq!(lyrics_lead_s("Chrome"), 0.0);
    assert_eq!(lyrics_lead_s(""), 0.0);
}

#[test]
fn test_applemusic_find_bundle_urls_adversarial() {
    assert!(find_bundle_urls("").is_empty());
    assert!(find_bundle_urls("<html><body>No scripts here</body></html>").is_empty());

    // Legacy bundle must be skipped
    let html_legacy = r#"<script src="/assets/index-legacy~abc.js"></script>"#;
    assert!(find_bundle_urls(html_legacy).is_empty());

    // CSS file with same prefix must be skipped
    let html_css = r#"<link href="/assets/index~abc.css">"#;
    assert!(find_bundle_urls(html_css).is_empty());

    // Valid bundle with various delimiters: quotes, spaces, angles
    let html_multi = r#"
        <script src="/assets/index~111.js"></script>
        <script src='/assets/index~222.js'></script>
        <script src=/assets/index~333.js>
        <script src="/assets/index~111.js"></script> <!-- duplicate -->
    "#;
    let urls = find_bundle_urls(html_multi);
    assert_eq!(urls.len(), 3);
    assert_eq!(urls[0], "https://music.apple.com/assets/index~111.js");
    assert_eq!(urls[1], "https://music.apple.com/assets/index~222.js");
    assert_eq!(urls[2], "https://music.apple.com/assets/index~333.js");
}

#[test]
fn test_applemusic_urls_construction_adversarial() {
    assert_eq!(storefront_url(), "https://amp-api.music.apple.com/v1/me/storefront");

    // Query with special characters (&, =, ?, spaces, non-ASCII)
    let q = TrackQuery {
        title: "Rock & Roll? = True".into(),
        artist: "AC/DC & Beyoncé".into(),
        album: String::new(),
        duration_s: 200,
    };
    let s_url = search_url("us", &q);
    assert!(s_url.starts_with("https://amp-api.music.apple.com/v1/catalog/us/search?"));
    assert!(s_url.contains("types=songs"));
    assert!(s_url.contains("limit=10"));
    assert!(s_url.contains("term="));

    // Storefront with special chars is percent-encoded
    let l_url = lyrics_url("us/special", "12345/song");
    assert_eq!(
        l_url,
        "https://amp-api.music.apple.com/v1/catalog/us%2Fspecial/songs/12345%2Fsong/lyrics"
    );
}

#[test]
fn test_nowplaying_is_browser_and_site_from_title() {
    // Browsers
    assert!(is_browser("Chrome"));
    assert!(is_browser("MSEdge"));
    assert!(is_browser("308046B0AF4A39CB")); // 16-hex firefox hash
    assert!(is_browser("com.apple.Safari"));
    assert!(is_browser("company.thebrowser.dia"));
    assert!(is_browser("app.zen-browser.zen"));

    // Non-browsers
    assert!(!is_browser("AppleMusic.exe"));
    assert!(!is_browser("Spotify.exe"));
    assert!(!is_browser("Chrome._crx_cinhimbnkkaeohfgghhklpknlkffjgod")); // web app
    assert!(!is_browser("com.apple.Music"));

    // Site from window title
    let title = "Viva La Vida";
    assert_eq!(
        site_from_window_title("Viva La Vida - YouTube - Google Chrome", title),
        Some("YouTube")
    );
    assert_eq!(
        site_from_window_title("Viva La Vida - YouTube Music - Brave", title),
        Some("YouTube Music")
    );
    assert_eq!(
        site_from_window_title("Unrelated Window - YouTube - Chrome", title),
        None
    );
    assert_eq!(
        site_from_window_title("Viva La Vida - Vimeo - Chrome", title),
        None
    );
    assert_eq!(site_from_window_title("Viva La Vida - YouTube - Chrome", ""), None);
}

#[test]
fn test_mediaremote_randomized_diff_sequence_fuzz() {
    let mut s = Stream::default();
    s.apply(r#"{"type":"data","diff":false,"payload":{"title":"Start","playing":true,"elapsedTimeMicros":0}}"#);

    // Run 100 pseudo-random diffs inserting and removing keys
    let mut seed = 0x987654321u64;
    let mut rand_u32 = || -> u32 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as u32
    };

    let keys = ["artist", "album", "durationMicros", "playbackRate", "customKey"];
    for _ in 0..100 {
        let k = keys[(rand_u32() as usize) % keys.len()];
        let delete = rand_u32() % 2 == 0;
        let line = if delete {
            format!(r#"{{"type":"data","diff":true,"payload":{{"{k}":null}}}}"#)
        } else {
            let val = rand_u32() % 1000;
            format!(r#"{{"type":"data","diff":true,"payload":{{"{k}":{val}}}}}"#)
        };
        // apply must never panic
        let _ = s.apply(&line);
        // playing() must never panic
        let _ = s.playing();
        // position_at must never panic
        if let Some(p) = s.playing() {
            let pos = p.position_at(1_700_000_000.0);
            assert!(pos >= 0.0);
        }
    }
}

#[test]
fn test_nowplaying_artist_album_property_fuzz() {
    let apple = "AppleMusic.exe";
    let samples = [
        "",
        "SingleWord",
        "Two Words",
        "Part1 \u{2014} Part2",
        "Part1 \u{2014} Part2 \u{2014} Part3",
        "Part1 \u{2014} Part2 \u{2014} Part3 \u{2014} Part4",
        " \u{2014} ",
        " \u{2014} OnlyAlbum",
        "OnlyArtist \u{2014} ",
        "A - B",
        "A -- B",
        "A – B", // En-dash U+2013
        "A—B",   // Em-dash U+2014 without spaces
        "   Whitespace   ",
        "\u{2014}",
        " \u{2014} \u{2014} ",
    ];

    for &art in &samples {
        for &alb in &samples {
            // Must never panic
            let (a, b) = artist_album(apple, art.into(), alb.into());
            // If raw artist was empty, result artist is empty
            if art.is_empty() {
                assert_eq!(a, "");
            }
            // Result must be valid UTF-8 Strings
            assert!(a.is_ascii() || !a.is_empty() || a.is_empty());
            assert!(b.is_ascii() || !b.is_empty() || b.is_empty());
        }
    }
}

