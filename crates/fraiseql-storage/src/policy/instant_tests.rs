//! A policy's `not_before` / `not_after`: the strict RFC 3339 text a rule is read from, and
//! the text it is written back as.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)] // Reason: test code, panics acceptable

use super::{PolicyRuleSpec, parse_policy, policy_to_specs};

fn rule(not_before: &str) -> PolicyRuleSpec {
    PolicyRuleSpec {
        methods:           vec!["read".to_string()],
        principal:         "authenticated".to_string(),
        key_prefix:        None,
        not_before:        Some(not_before.to_string()),
        not_after:         None,
        require_unexpired: false,
        require_claims:    std::collections::BTreeMap::new(),
        require_metadata:  std::collections::BTreeMap::new(),
    }
}

/// The text `not_before` is written back as, or the error reading it.
fn round_trip(not_before: &str) -> Result<String, String> {
    let policy = parse_policy(&[rule(not_before)]).map_err(|e| e.to_string())?;
    Ok(policy_to_specs(&policy)[0].not_before.clone().unwrap())
}

/// A bound is written in whole seconds of UTC with a `Z`; a fraction is dropped toward
/// the earlier second, before 1970 as after.
#[test]
fn a_bound_is_written_in_whole_utc_seconds() {
    for (read, written) in [
        ("2026-01-31T00:00:00Z", "2026-01-31T00:00:00Z"),
        ("2026-01-31t00:00:00z", "2026-01-31T00:00:00Z"),
        ("2026-01-31 00:00:00.999+02:00", "2026-01-30T22:00:00Z"),
        ("2026-01-31T00:00:00−05:30", "2026-01-31T05:30:00Z"),
        ("1969-12-31T23:59:59.5Z", "1969-12-31T23:59:59Z"),
        ("0000-01-01T00:00:00Z", "0000-01-01T00:00:00Z"),
    ] {
        assert_eq!(round_trip(read).as_deref(), Ok(written), "{read}");
    }
}

/// Text outside the strict grammar, a leap second, and an instant out of range are
/// refused, naming the field and the text.
#[test]
fn a_bound_outside_the_strict_grammar_is_refused() {
    for refused in [
        "2026-01-31T00:00:00 UTC",
        "2026-01-31T00:00:00UTC",
        "2026-01-31T00:00:00+0200",
        " 2026-01-31T00:00:00Z",
        "2026-01-31T00:00:00Z ",
        "+2026-01-31T00:00:00Z",
        "2026-1-31T00:00:00Z",
        "2026-01-31",
        "2026-01-31T00:00Z",
        "2026-01-31T23:59:60Z",
        "9999-12-31T23:59:59Z",
    ] {
        let error = round_trip(refused).expect_err(refused);
        assert!(
            error.starts_with(&format!("policy rule 0: not_before = {refused:?} is not ")),
            "{error}"
        );
    }
}
