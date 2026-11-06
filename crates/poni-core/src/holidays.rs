//! Indonesian National Holidays (Hari Libur Nasional & Tanggal Merah).

use crate::agenda::Event;
use crate::datetime::civil_from_days;

/// Returns the national holiday name for Indonesian public holidays (Tanggal Merah).
pub fn holiday_for(year: i32, month: u8, day: u8) -> Option<&'static str> {
    // Fixed-date national holidays
    match (month, day) {
        (1, 1) => return Some("Tahun Baru Masehi"),
        (5, 1) => return Some("Hari Buruh Internasional"),
        (6, 1) => return Some("Hari Lahir Pancasila"),
        (8, 17) => return Some("Hari Kemerdekaan RI"),
        (12, 25) => return Some("Hari Raya Natal"),
        _ => {}
    }

    // Year-specific holidays (Lunar, Islamic, and Christian holidays)
    match year {
        2025 => match (month, day) {
            (1, 27) => Some("Isra Mi'raj Nabi Muhammad SAW"),
            (1, 29) => Some("Tahun Baru Imlek 2576 Kongzili"),
            (3, 29) => Some("Hari Suci Nyepi Tahun Baru Saka 1947"),
            (3, 31) | (4, 1) => Some("Hari Raya Idul Fitri 1446 H"),
            (4, 18) => Some("Wafat Yesus Kristus (Jumat Agung)"),
            (4, 20) => Some("Hari Paskah"),
            (5, 12) => Some("Hari Raya Waisak 2569 BE"),
            (5, 29) => Some("Kenaikan Yesus Kristus"),
            (6, 6) => Some("Hari Raya Idul Adha 1446 H"),
            (6, 27) => Some("Tahun Baru Islam 1447 H"),
            (9, 5) => Some("Maulid Nabi Muhammad SAW"),
            _ => None,
        },
        2026 => match (month, day) {
            (1, 16) => Some("Isra Mi'raj Nabi Muhammad SAW"),
            (2, 17) => Some("Tahun Baru Imlek 2577 Kongzili"),
            (3, 19) => Some("Hari Suci Nyepi Tahun Baru Saka 1948"),
            (3, 20) | (3, 21) => Some("Hari Raya Idul Fitri 1447 H"),
            (4, 3) => Some("Wafat Yesus Kristus (Jumat Agung)"),
            (4, 5) => Some("Hari Paskah"),
            (5, 14) => Some("Kenaikan Yesus Kristus"),
            (5, 27) => Some("Hari Raya Idul Adha 1447 H"),
            (5, 31) => Some("Hari Raya Waisak 2570 BE"),
            (6, 16) => Some("Tahun Baru Islam 1448 H"),
            (8, 25) => Some("Maulid Nabi Muhammad SAW"),
            _ => None,
        },
        2027 => match (month, day) {
            (2, 5) => Some("Isra Mi'raj Nabi Muhammad SAW"),
            (2, 6) => Some("Tahun Baru Imlek 2578 Kongzili"),
            (3, 9) => Some("Hari Suci Nyepi Tahun Baru Saka 1949"),
            (3, 10) | (3, 11) => Some("Hari Raya Idul Fitri 1448 H"),
            (3, 26) => Some("Wafat Yesus Kristus (Jumat Agung)"),
            (3, 28) => Some("Hari Paskah"),
            (5, 6) => Some("Kenaikan Yesus Kristus"),
            (5, 17) => Some("Hari Raya Idul Adha 1448 H"),
            (5, 20) => Some("Hari Raya Waisak 2571 BE"),
            (6, 6) => Some("Tahun Baru Islam 1449 H"),
            (8, 15) => Some("Maulid Nabi Muhammad SAW"),
            _ => None,
        },
        _ => None,
    }
}

/// Generates calendar events for all national holidays within the given day range.
pub fn holidays_in_range(start_day: i64, end_day: i64) -> Vec<Event> {
    let mut events = Vec::new();
    for d in start_day..=end_day {
        let (y, m, day) = civil_from_days(d);
        if let Some(name) = holiday_for(y, m, day) {
            let next_day = d + 1;
            let (ny, nm, nday) = civil_from_days(next_day);
            events.push(Event {
                title: name.to_string(),
                start: format!("{y:04}-{m:02}-{day:02}"),
                end: format!("{ny:04}-{nm:02}-{nday:02}"),
                all_day: true,
                place: Some("Hari Libur Nasional".to_string()),
                join: None,
                link: None,
            });
        }
    }
    events
}
