//! `FRAISEQL_REQUIRE_REDIS` reads as a boolean in clap's grammar, as the server's other
//! boolean variables do: `0` or `false` leaves the requirement off, and a value that is
//! neither true nor false refuses boot instead of switching the requirement on.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::initialization::{SharedStateBackends, redis_requirement};

const PER_PROCESS: SharedStateBackends = SharedStateBackends {
    pkce_in_memory:         true,
    rate_limiter_in_memory: false,
    revocation_in_memory:   false,
    saml_replay_in_memory:  false,
};

const DISTRIBUTED: SharedStateBackends = SharedStateBackends {
    pkce_in_memory:         false,
    rate_limiter_in_memory: false,
    revocation_in_memory:   false,
    saml_replay_in_memory:  false,
};

const TRUE: [&str; 7] = ["1", "true", "TRUE", "yes", "y", "on", " t "];
const FALSE: [&str; 7] = ["0", "false", "False", "no", "n", "off", " f "];

#[test]
fn unset_does_not_require_redis() {
    redis_requirement(None, &PER_PROCESS).unwrap();
}

#[test]
fn a_false_value_does_not_require_redis() {
    for raw in FALSE {
        assert!(redis_requirement(Some(raw), &PER_PROCESS).is_ok(), "{raw:?}");
    }
}

#[test]
fn a_true_value_requires_redis() {
    for raw in TRUE {
        let refusal = redis_requirement(Some(raw), &PER_PROCESS).unwrap_err().to_string();
        assert!(refusal.contains("per-process state"), "{raw:?}: {refusal}");
        redis_requirement(Some(raw), &DISTRIBUTED).unwrap();
    }
}

#[test]
fn a_value_that_is_neither_true_nor_false_refuses_boot_naming_the_variable() {
    for raw in ["", "enabled", "2", "redis"] {
        for backends in [&PER_PROCESS, &DISTRIBUTED] {
            let refusal = redis_requirement(Some(raw), backends).unwrap_err().to_string();
            assert!(refusal.contains("FRAISEQL_REQUIRE_REDIS"), "{raw:?}: {refusal}");
            assert!(refusal.contains("not a boolean"), "{raw:?}: {refusal}");
        }
    }
}

/// The grammar is clap's `BoolishValueParser`, the one the server's flags use.
#[cfg(feature = "cli")]
#[test]
fn the_grammar_is_clap_boolish() {
    use clap::builder::TypedValueParser as _;

    let command = clap::Command::new("fraiseql-server");
    let parser = clap::builder::BoolishValueParser::new();
    for raw in TRUE.iter().chain(FALSE.iter()).chain(["", "enabled", "2"].iter()) {
        let clap = parser.parse_ref(&command, None, std::ffi::OsStr::new(raw.trim())).ok();
        let ours = super::env_bool::boolish(raw);
        assert_eq!(ours, clap, "{raw:?}");
    }
}
