//! The readers, case by case against what chrono's parsers returned, then over a sweep
//! against chrono itself while chrono is still a workspace dependency.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::{read_date, read_date_time, read_rfc3339};

// 2024-01-15T10:30:00Z, in seconds since the Unix epoch.
const AT: i64 = 1_705_314_600;

#[test]
fn a_date_is_read_in_chronos_format_grammar() {
    let cases = [
        ("2024-01-15", Some(19_737)),
        (" 2024-01-15", Some(19_737)),
        ("2024- 01- 15", Some(19_737)),
        ("2024-1-5", Some(19_727)),
        ("+2024-01-15", Some(19_737)),
        ("024-01-15", Some(-710_748)),
        ("0-01-01", Some(-719_528)),
        ("-0044-03-15", Some(-735_525)),
        ("-44-03-15", Some(-735_525)),
        ("+10000-01-01", Some(2_932_897)),
        ("+262142-12-31", Some(95_026_236)),
        ("-262143-01-01", Some(-96_465_292)),
        ("2024-02-29", Some(19_782)),
        ("10000-01-01", None),
        ("002024-01-15", None),
        ("2024 -01-15", None),
        ("2024-01-15 ", None),
        ("2024-001-15", None),
        ("2023-02-29", None),
        ("2024-00-15", None),
        ("2024-01-00", None),
        ("+262143-01-01", None),
        ("-262144-12-31", None),
        ("+", None),
        ("", None),
    ];
    for (text, want) in cases {
        assert_eq!(read_date(text), want, "{text:?}");
    }
}

#[test]
fn a_date_time_is_read_in_chronos_format_grammar_as_utc() {
    let cases = [
        ("2024-01-15T10:30:00", Some(AT)),
        (" 2024-01-15T 10: 30: 00", Some(AT)),
        ("2024-01-15T1:3:0", Some(1_705_280_580)),
        ("2024-01-15T10:30:60", Some(AT + 60)),
        ("2024-01-15T23:59:60", Some(1_705_363_200)),
        ("+10000-01-01T00:00:00", Some(253_402_300_800)),
        ("-262143-01-01T00:00:00", Some(-8_334_601_228_800)),
        ("262142-12-31T23:59:59", None),
        ("2024-01-15T10:30:00Z", None),
        ("2024-01-15t10:30:00", None),
        ("2024-01-15 10:30:00", None),
        ("2024-01-15T10:30:00.5", None),
        ("2024-01-15T24:00:00", None),
        ("2024-01-15T10:60:00", None),
        ("2024-01-15T10:30:61", None),
    ];
    for (text, want) in cases {
        assert_eq!(read_date_time(text), want.map(|second| (second, 0)), "{text:?}");
    }
}

#[test]
fn rfc3339_is_read_as_chrono_read_it_through_the_last_day_of_9999() {
    let cases = [
        ("2024-01-15T10:30:00Z", Some((AT, 0))),
        ("2024-01-15t10:30:00z", Some((AT, 0))),
        ("2024-01-15 10:30:00+00:00", Some((AT, 0))),
        ("2024-01-15T12:30:00+02:00", Some((AT, 0))),
        ("2024-01-15T08:30:00\u{2212}02:00", Some((AT, 0))),
        ("2024-01-15T10:30:00.1234567891Z", Some((AT, 123_456_789))),
        ("1969-12-31T23:59:59.999999Z", Some((-1, 999_999_000))),
        ("2024-01-15T10:30:60Z", Some((AT + 60, 0))),
        ("2024-01-15T23:59:60.5Z", Some((1_705_363_200, 500_000_000))),
        ("9999-12-31T23:59:59Z", Some((253_402_300_799, 0))),
        ("9999-12-31T23:59:59-23:59", Some((253_402_387_139, 0))),
        ("0000-01-01T00:00:00+23:59", Some((-62_167_305_540, 0))),
        ("2024-01-15T10:30:00+0200", None),
        ("2024-01-15T10:30:00+02", None),
        ("2024-01-15T10:30:00UTC", None),
        ("2024-01-15T10:30:00+24:00", None),
        ("2024-01-15T10:30:00.Z", None),
        ("2024-01-15T10:30:00", None),
        ("+2024-01-15T10:30:00Z", None),
        ("2024-01-15T10:30:61Z", None),
        ("2024-02-30T10:30:00Z", None),
    ];
    for (text, want) in cases {
        assert_eq!(read_rfc3339(text), want, "{text:?}");
    }
}

mod against_chrono {
    use chrono::{DateTime, NaiveDate, NaiveDateTime};

    use super::{read_date, read_date_time, read_rfc3339};

    const YEARS: [&str; 22] = [
        "2024", "024", "24", "0", "0000", "1969", "2000", "1900", "+2024", "-0044", "-44",
        "+10000", "10000", "+262142", "-262143", "+262143", "-262144", " 2024", "+ 2024", "--5",
        "9999", "",
    ];
    const MONTHS: [&str; 9] = ["01", "1", " 1", "02", "12", "00", "13", "001", "+1"];
    const DAYS: [&str; 10] = ["15", "5", " 5", "28", "29", "30", "31", "00", "32", "1 "];

    fn dates() -> impl Iterator<Item = String> {
        YEARS.iter().flat_map(|y| {
            MONTHS
                .iter()
                .flat_map(move |m| DAYS.iter().map(move |d| format!("{y}-{m}-{d}")))
        })
    }

    fn chrono_days(text: &str) -> Option<i64> {
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        NaiveDate::parse_from_str(text, "%Y-%m-%d").ok().map(|d| (d - epoch).num_days())
    }

    #[test]
    fn read_date_takes_what_chrono_took() {
        for text in dates() {
            assert_eq!(read_date(&text), chrono_days(&text), "{text:?}");
        }
    }

    #[test]
    fn read_date_time_takes_what_chrono_took() {
        let times = [
            "T10:30:00",
            "T0:0:0",
            "T 1: 2: 3",
            "T23:59:60",
            "T12:34:60",
            "T24:00:00",
            "T10:60:00",
            "T10:30:61",
            "t10:30:00",
            " 10:30:00",
            "T10:30:00Z",
            "T10:30:00.5",
            "T10:30",
            "T10:30:00 ",
        ];
        for date in dates().filter(|d| chrono_days(d).is_some()).take(400) {
            for time in times {
                let text = format!("{date}{time}");
                let chrono = NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S")
                    .ok()
                    .map(|n| n.and_utc().timestamp_micros());
                let ours = read_date_time(&text)
                    .map(|(second, nano)| second * 1_000_000 + i64::from(nano / 1_000));
                assert_eq!(ours, chrono, "{text:?}");
            }
        }
    }

    #[test]
    fn read_rfc3339_takes_what_chrono_took() {
        let dates = [
            "2024-01-15",
            "0000-01-01",
            "9999-12-31",
            "9999-12-30",
            "1969-12-31",
            "2024-02-29",
            "2023-02-29",
            "2024-1-15",
            "+2024-01-15",
            " 2024-01-15",
            "99999-01-01",
        ];
        let separators = ["T", "t", " ", "_", ""];
        let times = [
            "10:30:00",
            "23:59:59",
            "23:59:60",
            "00:00:00",
            "24:00:00",
            "10:60:00",
            "10:30:61",
            "1:30:00",
            "10:30:00.5",
            "10:30:00.123456789",
            "10:30:00.1234567891",
            "10:30:00.",
            "23:59:60.999999999",
        ];
        let offsets = [
            "Z",
            "z",
            "+00:00",
            "-00:00",
            "+23:59",
            "-23:59",
            "\u{2212}02:00",
            "+24:00",
            "+02:60",
            "+0200",
            "+02",
            "UTC",
            "",
            " Z",
            "Z ",
            "+2:00",
        ];
        for date in dates {
            for sep in separators {
                for time in times {
                    for offset in offsets {
                        let text = format!("{date}{sep}{time}{offset}");
                        let chrono = DateTime::parse_from_rfc3339(&text)
                            .ok()
                            .map(|d| (d.timestamp(), d.timestamp_subsec_nanos()));
                        let ours = read_rfc3339(&text);
                        // chrono writes a leap second as the 59th with a second's worth of
                        // extra nanoseconds; it is the same instant.
                        let chrono = chrono
                            .map(|(s, n)| (s + i64::from(n / 1_000_000_000), n % 1_000_000_000));
                        assert_eq!(ours, chrono, "{text:?}");
                    }
                }
            }
        }
    }
}
