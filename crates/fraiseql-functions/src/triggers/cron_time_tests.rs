//! What a cron schedule reads and writes about time, stated in RFC 3339 text so the
//! assertions do not depend on the date-time library the schedule is built on.
//!
//! Each firing time below was produced by the schedule on IANA 2025b, except where a case
//! says the zone's rules changed since.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics acceptable

use super::{CronDecision, CronExecutionState, CronTrigger, Schedule};

type Instant = fraiseql_time::Timestamp;

fn instant(text: &str) -> Instant {
    text.parse().expect("an RFC 3339 instant")
}

fn show(at: Instant) -> String {
    let utc = at.as_jiff().to_zoned(fraiseql_time::jiff::tz::TimeZone::UTC);
    utc.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn minute_after(at: Instant) -> Instant {
    at + fraiseql_time::jiff::SignedDuration::from_mins(1)
}

/// The minutes in `[from, to)` at which `expression` in `zone` fires, recording each fire.
fn fires(expression: &str, zone: &str, from: &str, to: &str) -> Vec<String> {
    let schedule = Schedule::parse(expression, Some(zone)).expect("the schedule parses");
    let mut state = CronExecutionState::new();
    let mut fired = Vec::new();
    let (mut now, to) = (instant(from), instant(to));
    while now < to {
        if schedule.decide(&state, &now) == CronDecision::Fire {
            state.record_execution(now);
            fired.push(show(now));
        }
        now = minute_after(now);
    }
    fired
}

/// Every zone name the schedule accepted on IANA 2025b, links such as `US/Eastern`,
/// `Asia/Calcutta` and `Europe/Kiev` included.
#[test]
fn every_zone_name_accepted_on_iana_2025b_is_accepted() {
    let names: Vec<&str> = include_str!("cron_zone_names.txt").lines().collect();
    assert_eq!(names.len(), 597);
    for name in names {
        let schedule =
            Schedule::parse("0 2 * * *", Some(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(schedule.zone_name(), name);
    }
}

/// A zone is named exactly: another case, surrounding space, nothing, an unknown name and
/// the `Factory` placeholder are refused, naming the value.
#[test]
fn a_zone_name_that_is_not_exactly_an_iana_name_is_refused() {
    for name in [
        "europe/paris",
        "utc",
        "Europe/Paris ",
        "",
        "Mars/Olympus",
        "Factory",
    ] {
        let error = Schedule::parse("0 2 * * *", Some(name)).expect_err(name);
        assert_eq!(
            error,
            format!("unknown timezone {name:?}: expected an IANA name such as \"Europe/Paris\"")
        );
    }
    assert_eq!(Schedule::parse("0 2 * * *", None).unwrap().zone_name(), "UTC");
}

/// 2026-10-04: Adelaide goes from 02:00 ACST (+09:30) to 03:00 ACDT (+10:30), a
/// half-hour zone in the southern hemisphere. A fixed 02:30 fires at the first minute
/// after the gap; a wildcard hour keeps its wall clock.
#[test]
fn adelaide_skips_an_hour_in_october() {
    assert_eq!(
        fires(
            "30 2 * * *",
            "Australia/Adelaide",
            "2026-10-03T12:00:00Z",
            "2026-10-04T12:00:00Z"
        ),
        ["2026-10-03T16:30:00Z"]
    );
    assert_eq!(
        fires(
            "*/30 * * * *",
            "Australia/Adelaide",
            "2026-10-03T15:00:00Z",
            "2026-10-03T18:00:00Z"
        ),
        [
            "2026-10-03T15:00:00Z",
            "2026-10-03T15:30:00Z",
            "2026-10-03T16:00:00Z",
            "2026-10-03T16:30:00Z",
            "2026-10-03T17:00:00Z",
            "2026-10-03T17:30:00Z",
        ]
    );
}

/// 2026-04-05: Adelaide goes back from 03:00 ACDT to 02:00 ACST, so 02:30 happens twice
/// and fires on its first occurrence.
#[test]
fn adelaide_repeats_an_hour_in_april() {
    assert_eq!(
        fires(
            "30 2 * * *",
            "Australia/Adelaide",
            "2026-04-04T12:00:00Z",
            "2026-04-05T12:00:00Z"
        ),
        ["2026-04-04T16:00:00Z"]
    );
}

/// Santiago changes its clocks at midnight: on 2026-09-06 local midnight does not exist,
/// so a midnight job fires at 01:00 (-03); on 2026-04-04, 23:30 happens twice.
#[test]
fn santiago_changes_its_clocks_at_midnight() {
    assert_eq!(
        fires("0 0 * * *", "America/Santiago", "2026-09-05T12:00:00Z", "2026-09-07T12:00:00Z"),
        ["2026-09-06T04:00:00Z", "2026-09-07T03:00:00Z"]
    );
    assert_eq!(
        fires(
            "30 23 * * *",
            "America/Santiago",
            "2026-04-04T12:00:00Z",
            "2026-04-05T12:00:00Z"
        ),
        ["2026-04-05T02:30:00Z"]
    );
}

/// Kathmandu is UTC+05:45.
#[test]
fn kathmandu_is_a_quarter_hour_off() {
    assert_eq!(
        fires("0 9 * * *", "Asia/Kathmandu", "2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z"),
        ["2026-03-01T03:15:00Z"]
    );
}

/// The payload's `executed_at` is the tick's whole second, in UTC with a `Z`: a fraction
/// is dropped, not rounded, before 1970 as after.
#[test]
fn the_payload_time_is_the_whole_second_in_utc() {
    let trigger = CronTrigger {
        function_name: "nightly".to_string(),
        schedule:      "* * * * *".to_string(),
        timezone:      "UTC".to_string(),
    };
    for (at, written) in [
        ("2024-03-15T02:00:59.999Z", "2024-03-15T02:00:59Z"),
        ("2024-03-15T02:00:00Z", "2024-03-15T02:00:00Z"),
        ("1969-12-31T23:59:30.5Z", "1969-12-31T23:59:30Z"),
    ] {
        assert_eq!(trigger.build_payload(&instant(at)).data["executed_at"], written, "{at}");
    }
}

/// The state's stored form: the instant in UTC, its fraction in 3, 6 or 9 digits or none.
#[test]
fn the_state_is_stored_with_the_instant_as_written() {
    for (state, written) in [
        (CronExecutionState::new(), r#"{"last_executed":null}"#),
        (
            CronExecutionState::resuming_from(instant("2024-03-15T02:00:07.312Z")),
            r#"{"last_executed":"2024-03-15T02:00:07.312Z"}"#,
        ),
        (
            CronExecutionState::resuming_from(instant("2024-03-15T02:00:00Z")),
            r#"{"last_executed":"2024-03-15T02:00:00Z"}"#,
        ),
        (
            CronExecutionState::resuming_from(instant("2024-03-15T02:00:00.000001Z")),
            r#"{"last_executed":"2024-03-15T02:00:00.000001Z"}"#,
        ),
    ] {
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, written);
        let read: CronExecutionState = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&read).unwrap(), written);
    }
}

/// A window starts at its minute, before 1970 as after.
#[test]
fn a_window_before_1970_starts_at_its_minute() {
    let schedule = Schedule::parse("* * * * *", None).unwrap();
    let state = CronExecutionState::resuming_from(instant("1969-12-31T23:59:40Z"));
    match schedule.decide(&state, &instant("1969-12-31T23:59:30.5Z")) {
        CronDecision::AlreadyFired {
            window_start,
            last_executed,
        } => {
            assert_eq!(show(window_start), "1969-12-31T23:59:00Z");
            assert_eq!(show(last_executed), "1969-12-31T23:59:40Z");
        },
        other => panic!("{other:?}"),
    }
}

/// Zones whose rules changed between IANA 2025b and 2026c: British Columbia keeps
/// daylight time from 2026-11-01, and Morocco keeps UTC+00:00 from 2026-09-20. A 09:00
/// job fires on the rules in force, an hour earlier and an hour later than on 2025b
/// (17:00Z and 08:00Z).
#[test]
fn a_zone_follows_the_rules_in_force() {
    assert_eq!(
        fires("0 9 * * *", "America/Vancouver", "2026-11-02T00:00:00Z", "2026-11-03T00:00:00Z"),
        ["2026-11-02T16:00:00Z"]
    );
    assert_eq!(
        fires("0 9 * * *", "Africa/Casablanca", "2026-10-01T00:00:00Z", "2026-10-02T00:00:00Z"),
        ["2026-10-01T09:00:00Z"]
    );
}
