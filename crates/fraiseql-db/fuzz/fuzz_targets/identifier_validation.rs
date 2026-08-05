#![no_main]

//! Property target for #794/#795 and #833: no identifier the validator accepts
//! can alter the structure of the SQL it is interpolated into.
//!
//! Three audit findings share one root cause — a client-supplied name reaching a
//! `format!` that builds SQL:
//!
//! * **#794** — window aliases and dimension paths were interpolated raw, and the
//!   `WindowAllowlist` was never consulted on the live path.
//! * **#795** — the `table` field of an aggregate/window request went into `FROM`
//!   verbatim, which substitutes an arbitrary relation and walks around RLS.
//! * **#833** — a `where` field name could close the quote of the JSON-path
//!   literal it was interpolated into. Its MySQL half left with the dialect, but
//!   the boundary protects PostgreSQL too and is now enforced by
//!   `validate_graphql_identifier`.
//!
//! The fix in each case was to validate the name at the boundary against
//! `[_A-Za-z][_0-9A-Za-z]*`. That grammar is the whole defence, so this target
//! asserts the validator cannot be talked out of it: **every** accepted string
//! must match the grammar exactly, and therefore cannot carry a quote, a
//! backslash, a comment marker, a space, a parenthesis or a semicolon.
//!
//! Fuzzing the validator rather than the SQL builders is deliberate — it is the
//! single boundary all three defects now funnel through, so a hole here reopens
//! all of them at once.

use fraiseql_db::utils::validate_graphql_identifier;
use libfuzzer_sys::fuzz_target;

/// The grammar the validator claims to enforce, written independently of it.
fn matches_grammar(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fuzz_target!(|data: &str| {
    if validate_graphql_identifier(data, "where").is_err() {
        return;
    }

    // Accepted. It must match the grammar…
    assert!(
        matches_grammar(data),
        "validator accepted an identifier outside [_A-Za-z][_0-9A-Za-z]*: {data:?}"
    );

    // …and therefore cannot carry anything that changes SQL structure. This is
    // implied by the grammar, but stated separately so the failure names the
    // consequence rather than the rule.
    for bad in [
        '\'', '"', '\\', ';', '(', ')', ' ', '\t', '\n', '\r', '-', '/', '*', '.', ',', '%',
    ] {
        assert!(
            !data.contains(bad),
            "accepted identifier can break out of interpolated SQL via {bad:?}: {data:?}"
        );
    }

    // Consistency across call sites: the same string must be judged the same way
    // whatever argument it was supplied for. #794 and #833 were the same defect
    // reached through two different arguments.
    assert!(
        validate_graphql_identifier(data, "orderBy").is_ok(),
        "identifier accepted for `where` but rejected for `orderBy`: {data:?}"
    );
});
