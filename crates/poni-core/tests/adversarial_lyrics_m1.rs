//! Empirical Adversarial Stress Verification Suite for Milestone M1
//! Lyrics Detection & Timing Engine
//!
//! Stress-tests:
//! 1. TTML Parser (`ttml.rs`): Malformed XML, unclosed tags, deep spans, odd/sub-ms times, entity decoding.
//! 2. LRCLIB Normalization (`lrclib.rs`): Complex Unicode, emojis, Asian noise parts, feat markers, match scoring.
//! 3. LRC Parser (`lrc.rs`): Out-of-order stamps, corrupted offset tags, sub-millisecond precision, word timings.
//! 4. LyricClock & Timeline (`timeline.rs` & `nowplaying.rs`): Anti-jitter filter, monotonicity, leaps, NaN/Inf resilience.

use poni_core::lrc::parse_lrc;
use poni_core::lrclib::{best_match, match_score, normalize_query, LrclibRecord, TrackQuery};
use poni_core::nowplaying::{is_apple_music, lyrics_lead_s};
use poni_core::timeline::{LyricClock, PendingSeek, Timeline};
use poni_core::ttml::{parse_time, parse_ttml};

// =========================================================================
// 1. TTML PARSER ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_ttml_empty_and_corrupted_roots() {
    // Completely empty and whitespace inputs
    assert!(parse_ttml("").is_empty());
    assert!(!parse_ttml("").synced);
    assert!(parse_ttml("   \t\r\n  ").is_empty());

    // Root without body or closing tags
    assert!(parse_ttml("<tt>").is_empty());
    assert!(parse_ttml("<tt></tt>").is_empty());
    assert!(parse_ttml("<tt><head></head><body></body></tt>").is_empty());

    // Tag soup / broken XML without <p>
    assert!(parse_ttml("<<<<>>>>").is_empty());
    // Tag without closing `>`
    assert!(parse_ttml("<p begin=\"1\" no closing bracket at all").is_empty());

    // Tag with closing `>` but no closing `</p>`: lenient scanner recovers the line
    let no_close_p = parse_ttml("<p begin=\"1.0\">no closing p tag");
    assert_eq!(no_close_p.lines.len(), 1);
    assert_eq!(no_close_p.lines[0].text, "no closing p tag");
    assert!(no_close_p.synced);

    // In a lenient scanner, `<<<p>>` has `<p>` followed by text `>`
    let soup_p = parse_ttml("<<<p>>");
    assert_eq!(soup_p.lines.len(), 1);
    assert_eq!(soup_p.lines[0].text, ">");
    assert!(!soup_p.synced);

    // <body appearing inside an XML comment before real body
    let commented_body = "<!-- <body> <p begin=\"1.0\">Ignored</p> </body> -->\
        <tt><body><p begin=\"2.0\">Real Line</p></body></tt>";
    let lyr = parse_ttml(commented_body);
    // Real line should still be parsed
    assert!(!lyr.is_empty());
    assert_eq!(lyr.lines.last().map(|l| l.text.as_str()), Some("Real Line"));
}

#[test]
fn test_ttml_pathological_p_tags() {
    // Missing closing </p> tag: lenient parser should not panic or loop infinitely
    let unclosed_p = "<tt><body><p begin=\"1.0\">Line 1<p begin=\"2.0\">Line 2</p></body></tt>";
    let lyr = parse_ttml(unclosed_p);
    // Lenient parser should recover without crashing
    assert!(!lyr.lines.is_empty());

    // Self-closing <p/> variations
    let self_closing_1 = "<tt><body><p begin=\"1.0\"/><p begin=\"2.0\">Second</p></body></tt>";
    let lyr1 = parse_ttml(self_closing_1);
    assert_eq!(lyr1.lines.len(), 1);
    assert_eq!(lyr1.lines[0].text, "Second");

    let self_closing_space = "<tt><body><p begin=\"1.0\" /><p begin=\"2.0\">Second</p></body></tt>";
    let lyr2 = parse_ttml(self_closing_space);
    // Must not crash or loop
    assert!(!lyr2.lines.is_empty());

    // Empty <p></p> tags
    let empty_p = "<tt><body><p begin=\"1.0\"></p><p begin=\"2.0\">Text</p></body></tt>";
    let lyr3 = parse_ttml(empty_p);
    assert_eq!(lyr3.lines.len(), 1);
    assert_eq!(lyr3.lines[0].text, "Text");

    // Multiple consecutive closing tags
    let multi_close = "<tt><body><p begin=\"1.0\">Text</p></p></p><p begin=\"2.0\">Next</p></body></tt>";
    let lyr4 = parse_ttml(multi_close);
    assert_eq!(lyr4.lines.len(), 2);
    assert_eq!(lyr4.lines[0].text, "Text");
    assert_eq!(lyr4.lines[1].text, "Next");
}

#[test]
fn test_ttml_nested_spans_and_roles() {
    // Deeply nested spans (50 levels)
    let mut deep = String::from("<tt><body><p begin=\"1.0\">");
    for _ in 0..50 {
        deep.push_str("<span>");
    }
    deep.push_str("Deep Words");
    for _ in 0..50 {
        deep.push_str("</span>");
    }
    deep.push_str("</p></body></tt>");
    let lyr = parse_ttml(&deep);
    assert_eq!(lyr.lines.len(), 1);
    assert_eq!(lyr.lines[0].text, "Deep Words");

    // Unbalanced spans (extra closing </span>)
    let extra_close = "<tt><body><p begin=\"1.0\"><span>Word</span></span></span> more</p></body></tt>";
    let lyr = parse_ttml(extra_close);
    assert_eq!(lyr.lines[0].text, "Word more");

    // Unclosed spans
    let unclosed_span = "<tt><body><p begin=\"1.0\"><span>Word <span>nested</p></body></tt>";
    let lyr = parse_ttml(unclosed_span);
    assert_eq!(lyr.lines[0].text, "Word nested");

    // Background vocal with nested translation (x-translation dropped, x-bg kept in parens)
    let roles = "<tt><body><p begin=\"1.0\">Main <span ttm:role=\"x-bg\">backing <span ttm:role=\"x-translation\">terjemahan</span> vocals</span></p></body></tt>";
    let lyr = parse_ttml(roles);
    assert_eq!(lyr.lines[0].text, "Main (backing vocals)");

    // Background vocal starting with open paren
    let bg_paren = "<tt><body><p begin=\"1.0\">Main <span ttm:role=\"x-bg\">(backing)</span></p></body></tt>";
    let lyr = parse_ttml(bg_paren);
    // Should format as "Main (backing)", not double parens "Main ((backing))"
    assert_eq!(lyr.lines[0].text, "Main (backing)");
}

#[test]
fn test_ttml_time_expressions_adversarial() {
    // Valid standard and sub-millisecond precision
    assert_eq!(parse_time("0:00:00.000"), Some(0.0));
    assert_eq!(parse_time("00:00:00.000"), Some(0.0));
    assert_eq!(parse_time("00:00:00"), Some(0.0));
    assert_eq!(parse_time("0s"), Some(0.0));
    assert_eq!(parse_time("0ms"), Some(0.0));
    assert_eq!(parse_time("0.000s"), Some(0.0));
    assert_eq!(parse_time("1:02.5"), Some(62.5));
    assert_eq!(parse_time("12.75s"), Some(12.75));
    assert_eq!(parse_time("750ms"), Some(0.75));

    // High precision
    let subms = parse_time("01:23.456789").unwrap();
    assert!((subms - 83.456789).abs() < 1e-6);

    // Large valid time
    assert_eq!(parse_time("100:00:00.000"), Some(360000.0));

    // Malformed & hostile times (must return None, NEVER panic)
    assert_eq!(parse_time("-5.0s"), None);
    assert_eq!(parse_time("-01:00.000"), None);
    assert_eq!(parse_time("NaN"), None);
    assert_eq!(parse_time("Infinity"), None);
    assert_eq!(parse_time("-Infinity"), None);
    assert_eq!(parse_time("::"), None);
    assert_eq!(parse_time("1:2:3:4"), None);
    assert_eq!(parse_time("1.5:00"), None);
    assert_eq!(parse_time("0:05.123.456"), None);
    assert_eq!(parse_time("abc"), None);
    assert_eq!(parse_time(""), None);
    assert_eq!(parse_time("   "), None);

    // Document with unsorted lines: TTML parser must sort lines monotonically
    let unsorted = "<tt><body>\
        <p begin=\"00:20.000\" end=\"00:25.000\">Line 2</p>\
        <p begin=\"00:10.000\" end=\"00:15.000\">Line 1</p>\
        </body></tt>";
    let lyr = parse_ttml(unsorted);
    assert!(lyr.synced);
    assert_eq!(lyr.lines[0].time_s, 10.0);
    assert_eq!(lyr.lines[0].text, "Line 1");
    // Long gap (20 - 15 = 5 >= 4s): gap blank line inserted at 15.0s
    assert_eq!(lyr.lines[1].time_s, 15.0);
    assert_eq!(lyr.lines[1].text, "");
    assert_eq!(lyr.lines[2].time_s, 20.0);
    assert_eq!(lyr.lines[2].text, "Line 2");
}

#[test]
fn test_ttml_entity_decoding_stress() {
    // Full suite of XML predefined entities
    let xml = "<tt><body><p begin=\"1.0\">&amp; &lt; &gt; &quot; &apos;</p></body></tt>";
    assert_eq!(parse_ttml(xml).lines[0].text, "& < > \" '");

    // Hex and decimal character references
    let num_xml = "<tt><body><p begin=\"1.0\">&#65; &#x42; &#X43; &#x1F600;</p></body></tt>";
    assert_eq!(parse_ttml(num_xml).lines[0].text, "A B C \u{1F600}");

    // Corrupted and edge-case entities (must not panic)
    let bad_entities = "<tt><body><p begin=\"1.0\">&amp &lt &#xZZ; &#xD800; &#x110000; &#999999999; &</p></body></tt>";
    let lyr = parse_ttml(bad_entities);
    assert!(!lyr.lines.is_empty());
}

#[test]
fn test_ttml_scale_and_throughput_stress() {
    // 1,000 paragraphs in a single TTML document
    let mut large_doc = String::from("<tt><body>");
    for i in 0..1000 {
        large_doc.push_str(&format!(
            "<p begin=\"{}:{:02}.000\" end=\"{}:{:02}.800\">Line number {} of the song</p>",
            i / 60,
            i % 60,
            i / 60,
            i % 60,
            i
        ));
    }
    large_doc.push_str("</body></tt>");

    let start = std::time::Instant::now();
    let lyrics = parse_ttml(&large_doc);
    let elapsed = start.elapsed();

    assert!(lyrics.synced);
    assert_eq!(lyrics.lines.len(), 1000);
    assert!(
        elapsed.as_millis() < 500,
        "Parsing 1,000 TTML lines took too long: {:?}",
        elapsed
    );
}

// =========================================================================
// 2. LRCLIB NORMALIZATION ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_lrclib_unicode_and_emojis() {
    let q = |title: &str, artist: &str| TrackQuery {
        title: title.to_string(),
        artist: artist.to_string(),
        album: String::new(),
        duration_s: 200,
    };

    // Emojis and combining characters
    let res = normalize_query(&q("🔥 Fire & Ice ❄️ (Official Video) [4K]", "Band 🎸"));
    assert_eq!(res.title, "🔥 Fire & Ice ❄️");
    assert_eq!(res.artist, "Band 🎸");

    // Turkish dotted/dotless I case folding safety
    let res = normalize_query(&q("İSTANBUL - Çiçek (Official Audio)", "Artist"));
    assert_eq!(res.title, "İSTANBUL - Çiçek");

    // Kelvin sign and micro sign
    let res = normalize_query(&q("\u{212A}elvin feat. Someone", "Artist"));
    assert_eq!(res.title, "\u{212A}elvin");

    // RTL Arabic / Hebrew scripts
    let res = normalize_query(&q("مرحبا - أغنية (Official Video)", "فنان"));
    assert_eq!(res.title, "مرحبا - أغنية");

    // CJK Ideographs and Kana
    let res = normalize_query(&q("夜に駆ける (Official Music Video)", "YOASOBI"));
    assert_eq!(res.title, "夜に駆ける");
}

#[test]
fn test_lrclib_asian_noise_and_brackets() {
    let q = |title: &str, artist: &str| TrackQuery {
        title: title.to_string(),
        artist: artist.to_string(),
        album: String::new(),
        duration_s: 0,
    };

    // Thai noise
    assert_eq!(normalize_query(&q("Song (ซับไทย)", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song [แปลไทย]", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song (เนื้อเพลง)", "Artist")).title, "Song");

    // Korean noise
    assert_eq!(normalize_query(&q("Song [가사/해석]", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song [자막]", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song (한글자막)", "Artist")).title, "Song");

    // Chinese and Japanese noise with full-width brackets
    assert_eq!(normalize_query(&q("Song【中字】", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song【歌詞】", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song【和訳】", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song (字幕)", "Artist")).title, "Song");

    // Vietnamese noise
    assert_eq!(normalize_query(&q("Song (Lời bài hát)", "Artist")).title, "Song");
    assert_eq!(normalize_query(&q("Song (Lyrics + Vietsub)", "Artist")).title, "Song");

    // Legitimate brackets preserved
    assert_eq!(
        normalize_query(&q("(I Can't Get No) Satisfaction", "The Rolling Stones")).title,
        "(I Can't Get No) Satisfaction"
    );
    assert_eq!(normalize_query(&q("Song (Live at Wembley)", "Artist")).title, "Song (Live at Wembley)");
}

#[test]
fn test_lrclib_feat_markers_adversarial() {
    let q = |title: &str| TrackQuery {
        title: title.to_string(),
        artist: "Main Artist".to_string(),
        album: String::new(),
        duration_s: 0,
    };

    // Various feat markers stripped
    assert_eq!(normalize_query(&q("Song (feat. Guest)")).title, "Song");
    assert_eq!(normalize_query(&q("Song [feat. Guest]")).title, "Song");
    assert_eq!(normalize_query(&q("Song feat. Guest")).title, "Song");
    assert_eq!(normalize_query(&q("Song (ft. Guest)")).title, "Song");
    assert_eq!(normalize_query(&q("Song [ft. Guest]")).title, "Song");
    assert_eq!(normalize_query(&q("Song ft. Guest")).title, "Song");
    assert_eq!(normalize_query(&q("Song (with Guest)")).title, "Song");

    // Case insensitivity
    assert_eq!(normalize_query(&q("Song FEAT. Guest")).title, "Song");
    assert_eq!(normalize_query(&q("Song [FT. Guest]")).title, "Song");
    assert_eq!(normalize_query(&q("Song (WITH Guest)")).title, "Song");

    // Real words that contain "feat" or "ft" must NOT be stripped
    assert_eq!(normalize_query(&q("Accept Defeat. Today")).title, "Accept Defeat. Today");
    assert_eq!(normalize_query(&q("Turn Left. Now")).title, "Turn Left. Now");
    assert_eq!(normalize_query(&q("Graft. Work")).title, "Graft. Work");

    // Title that STARTS with "feat." should not be stripped to empty
    assert_eq!(normalize_query(&q("feat. Only")).title, "feat. Only");
}

#[test]
fn test_lrclib_match_score_and_stubs() {
    let q = TrackQuery {
        title: "Paper Boats".into(),
        artist: "The Lanterns".into(),
        album: "Harbor".into(),
        duration_s: 200,
    };

    // Stubs with fewer than 4 lines of text must NOT qualify as full synced lyrics
    let stub_record = LrclibRecord {
        id: 1,
        track_name: "Paper Boats".into(),
        artist_name: "The Lanterns".into(),
        duration: Some(200.0),
        synced_lyrics: Some("[00:01.00]One\n[00:02.00]Two\n[00:03.00]Three\n".into()),
        ..Default::default()
    };
    assert!(!stub_record.has_full_synced(), "3-line record must be classified as stub (<4 lines)");

    let full_record = LrclibRecord {
        id: 2,
        track_name: "Paper Boats".into(),
        artist_name: "The Lanterns".into(),
        duration: Some(200.0),
        synced_lyrics: Some("[00:01.00]One\n[00:02.00]Two\n[00:03.00]Three\n[00:04.00]Four\n".into()),
        ..Default::default()
    };
    assert!(full_record.has_full_synced());

    // Best match selection
    let records = vec![stub_record, full_record];
    let matched = best_match(&records, &q);
    assert_eq!(matched.map(|r| r.id), Some(2), "Stub must never win over full lyrics");

    // Score computation
    let score = match_score(&q, "Paper Boats", "The Lanterns", Some(200.0), true);
    assert!(score.unwrap() >= 100);

    // Completely mismatched title must yield None
    assert_eq!(match_score(&q, "Totally Different", "The Lanterns", Some(200.0), true), None);
}

#[test]
fn test_lrclib_pathological_string_lengths() {
    // 50,000-character malicious string
    let huge_noise = " [Official Video]".repeat(3000);
    let title = format!("Song{}", huge_noise);
    let q = TrackQuery {
        title,
        artist: "Artist".into(),
        album: "".into(),
        duration_s: 180,
    };

    let start = std::time::Instant::now();
    let norm = normalize_query(&q);
    let elapsed = start.elapsed();

    assert_eq!(norm.title, "Song");
    assert!(
        elapsed.as_millis() < 500,
        "Normalizing 50,000-char repetitive string took too long: {:?}",
        elapsed
    );
}

// =========================================================================
// 3. LRC PARSER ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_lrc_out_of_order_and_duplicates() {
    let text = "\
[00:30.00]Third line
[00:10.00]First line
[00:20.00]Second line
";
    let l = parse_lrc(text);
    assert!(l.synced);
    assert_eq!(l.lines.len(), 3);
    assert_eq!(l.lines[0].time_s, 10.0);
    assert_eq!(l.lines[0].text, "First line");
    assert_eq!(l.lines[1].time_s, 20.0);
    assert_eq!(l.lines[1].text, "Second line");
    assert_eq!(l.lines[2].time_s, 30.0);
    assert_eq!(l.lines[2].text, "Third line");

    // Multiple timestamps on a single line
    let multi_stamp = "[00:05.00][00:15.00][00:25.00]Repeated refrain";
    let l_multi = parse_lrc(multi_stamp);
    assert_eq!(l_multi.lines.len(), 3);
    assert_eq!(l_multi.lines[0].time_s, 5.0);
    assert_eq!(l_multi.lines[1].time_s, 15.0);
    assert_eq!(l_multi.lines[2].time_s, 25.0);
}

#[test]
fn test_lrc_corrupted_offset_tags() {
    // Normal positive offset: shifts timestamps earlier
    let l = parse_lrc("[offset:+1000]\n[00:05.00]Line");
    assert_eq!(l.lines[0].time_s, 4.0);

    // Negative offset: shifts timestamps later
    let l = parse_lrc("[offset:-1000]\n[00:05.00]Line");
    assert_eq!(l.lines[0].time_s, 6.0);

    // Huge positive offset: must clamp to 0.0, never negative
    let l = parse_lrc("[offset:+9999999]\n[00:05.00]Line");
    assert_eq!(l.lines[0].time_s, 0.0);

    // Non-numeric and corrupted offsets (must not panic)
    let l = parse_lrc("[offset:corrupted]\n[00:05.00]Line");
    assert_eq!(l.lines[0].time_s, 5.0);

    let l = parse_lrc("[offset:]\n[00:05.00]Line");
    assert_eq!(l.lines[0].time_s, 5.0);

    // Special float strings (NaN, Infinity): must not produce NaN or panic
    let l = parse_lrc("[offset:NaN]\n[00:05.00]Line");
    assert!(!l.lines[0].time_s.is_nan());
}

#[test]
fn test_lrc_submillisecond_and_extreme_fractions() {
    let lrc = "\
[00:01.1]1 digit
[00:02.12]2 digits
[00:03.123]3 digits
[00:04.123456]6 digits
[00:05:45]colon fraction
";
    let l = parse_lrc(lrc);
    assert_eq!(l.lines.len(), 5);
    assert!((l.lines[0].time_s - 1.1).abs() < 1e-9);
    assert!((l.lines[1].time_s - 2.12).abs() < 1e-9);
    assert!((l.lines[2].time_s - 3.123).abs() < 1e-9);
    assert!((l.lines[3].time_s - 4.123456).abs() < 1e-6);
    assert!((l.lines[4].time_s - 5.45).abs() < 1e-9);

    // Enhanced LRC word timings stripping
    let enhanced = "[00:10.00]Word <00:10.20>one <00:10.50>two <00:10.80>three";
    let l_enh = parse_lrc(enhanced);
    assert_eq!(l_enh.lines[0].text, "Word one two three");

    // Non-timing angle brackets preserved
    let tags = "[00:10.00]This is <b>bold</b> and <3 hearts";
    let l_tags = parse_lrc(tags);
    assert_eq!(l_tags.lines[0].text, "This is <b>bold</b> and <3 hearts");
}

#[test]
fn test_lrc_scale_and_line_index_stress() {
    // 5,000 lines in an LRC file
    let mut large_lrc = String::new();
    for i in 0..5000 {
        large_lrc.push_str(&format!("[{:02}:{:02}.50]Line {}\n", i / 60, i % 60, i));
    }

    let lyrics = parse_lrc(&large_lrc);
    assert!(lyrics.synced);
    assert_eq!(lyrics.lines.len(), 5000);

    // Stress test binary search partition point lookup across 10,000 random queries
    let start = std::time::Instant::now();
    for q in 0..10000 {
        let t = (q as f64) * 0.45;
        let idx = lyrics.line_index_at(t);
        if t < 0.50 {
            assert_eq!(idx, None);
        } else {
            assert!(idx.is_some());
        }
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 50,
        "10,000 binary search index lookups took too long: {:?}",
        elapsed
    );
}

// =========================================================================
// 4. LYRIC CLOCK & TIMELINE ENGINE ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_lyric_clock_anti_jitter_and_monotonicity() {
    let mut clock = LyricClock::default();
    let mut prev_t = f64::NEG_INFINITY;
    let mut max_drift = 0.0f64;

    // Simulate 600 frames (10 seconds at 60 FPS) with violent jitter (+-0.45s)
    // representing unbuffered Apple Music / GSMTC position reports.
    for frame in 0..600 {
        let now = frame as f64 / 60.0;
        let ground_truth = 10.0 + now;
        // Hostile jitter alternating every 10 frames
        let jitter = if (frame / 10) % 2 == 0 { 0.45 } else { -0.45 };
        let reported = ground_truth + jitter;

        let clock_t = clock.tick(now, reported, true, 1.0);

        // Invariant 1: Clock must NEVER run backwards while playing
        assert!(
            clock_t >= prev_t,
            "LyricClock violated monotonicity at frame {}: clock_t={} < prev_t={}",
            frame, clock_t, prev_t
        );
        prev_t = clock_t;

        // Invariant 2: After initial lock-in (60 frames = 1s), drift must stay bounded
        if frame > 60 {
            let drift = (clock_t - ground_truth).abs();
            max_drift = max_drift.max(drift);
        }
    }

    assert!(max_drift < 0.50, "Max jitter drift exceeded tolerance: {}s", max_drift);
}

#[test]
fn test_lyric_clock_leap_and_boundary_conditions() {
    let mut clock = LyricClock::default();

    // 1. User Seek forward (> 1.0s JUMP threshold): must snap immediately
    clock.tick(0.0, 10.0, true, 1.0);
    clock.tick(0.016, 10.016, true, 1.0);
    let seek_fwd = clock.tick(0.033, 50.0, true, 1.0);
    assert_eq!(seek_fwd, 50.0, "Seek forward must snap immediately");

    // 2. User Seek backward (> 1.0s JUMP threshold): must snap immediately
    let seek_back = clock.tick(0.050, 20.0, true, 1.0);
    assert_eq!(seek_back, 20.0, "Seek backward must snap immediately");

    // 3. Paused playback: clock must match reported position exactly
    let paused_pos = clock.tick(0.066, 15.0, false, 1.0);
    assert_eq!(paused_pos, 15.0, "Paused clock must track reported position");

    // 4. Large frame gap (dt > GAP_S = 0.5s): must snap to reported position
    let gap_snap = clock.tick(2.0, 35.0, true, 1.0);
    assert_eq!(gap_snap, 35.0, "Frame gap > 0.5s must snap to report");

    // 5. Negative dt (system clock jump backwards): must not crash or hang
    let neg_dt = clock.tick(1.0, 36.0, true, 1.0);
    assert_eq!(neg_dt, 36.0, "Negative dt must safely snap to report");

    // 6. Non-finite values (NaN / Inf): clock must reject/reset safely
    let nan_pos = clock.tick(2.1, f64::NAN, true, 1.0);
    assert!(nan_pos.is_nan());
    // Recovery after NaN
    let recovered = clock.tick(2.2, 40.0, true, 1.0);
    assert_eq!(recovered, 40.0, "Clock must recover immediately from NaN");
}

#[test]
fn test_apple_music_lyrics_lead_time_invariants() {
    // Apple Music for Windows Store app ID and executable
    let apple_music_store = "AppleInc.AppleMusicWin_nzyj5cx40ttqa!App";
    let apple_music_exe = "AppleMusic.exe";
    let spotify = "Spotify.exe";

    // Verification of is_apple_music matcher
    assert!(is_apple_music(apple_music_store));
    assert!(is_apple_music(apple_music_exe));
    assert!(!is_apple_music(spotify));

    // Apple Music Windows gets 0.45s lead time due to GSMTC lag
    assert_eq!(lyrics_lead_s(apple_music_store), 0.45);
    assert_eq!(lyrics_lead_s(apple_music_exe), 0.45);

    // Other players default to 0.0s lead (no artificial offset)
    assert_eq!(lyrics_lead_s(spotify), 0.0);
    assert_eq!(lyrics_lead_s("zen.exe"), 0.0);
    assert_eq!(lyrics_lead_s("chrome.exe"), 0.0);
}

#[test]
fn test_timeline_pending_seek_concurrency_and_expiration() {
    let seek = PendingSeek::new(120.0, 100.0);
    assert_eq!(seek.target_s(), 120.0);

    // Stale report before seek target: held
    let stale = Timeline::new(30.0, 240.0, 100.2, true);
    assert!(seek.holds(&stale, 100.5));

    // Fresh report matching seek target within tolerance: passed
    let fresh = Timeline::new(120.8, 240.0, 101.0, true);
    assert!(!seek.holds(&fresh, 101.0));

    // Expiration after PendingSeek::CONFIRM_S (3.0s): player word stands
    assert!(!seek.expired(102.9));
    assert!(seek.expired(103.0));
    assert!(!seek.holds(&stale, 103.1), "Expired seek must never hold reports");
}
