//! The production default, asserted directly.
//!
//! `fraiseql-server`'s `production_safety_test.rs` asserted one answer for an
//! unset `FRAISEQL_ENV` and `fraiseql-observers`' `insecure_guard` asserted the
//! opposite. Both were "tested behaviour"; only one can be right. These are the
//! assertions the whole workspace now shares.

use super::{
    declares_development, declares_production, env_opt_in, insecure_bypass_allowed, is_production,
};

/// Every variable this module reads, cleared, then overlaid with `overrides`.
fn with_env(overrides: &[(&str, Option<&str>)], f: impl FnOnce() + std::panic::UnwindSafe) {
    let mut vars: Vec<(&str, Option<&str>)> = vec![
        ("FRAISEQL_ENV", None),
        ("FRAISEQL_PROFILE", None),
        ("KUBERNETES_SERVICE_HOST", None),
    ];
    for (key, value) in overrides {
        vars.retain(|(k, _)| k != key);
        vars.push((key, *value));
    }
    temp_env::with_vars(vars, f);
}

#[test]
fn unset_means_production() {
    with_env(&[], || {
        assert!(
            is_production(),
            "an unset FRAISEQL_ENV must mean production — an operator should never \
             have to set anything to be treated as production"
        );
    });
}

#[test]
fn only_an_explicit_development_declaration_turns_production_off() {
    for value in ["development", "dev", "DEVELOPMENT", "Dev"] {
        with_env(&[("FRAISEQL_ENV", Some(value))], || {
            assert!(!is_production(), "FRAISEQL_ENV={value} declares development");
        });
    }
}

#[test]
fn unrecognised_values_stay_production() {
    // A typo must not silently downgrade the posture.
    for value in ["", "developement", "staging", "test", "local", "prod"] {
        with_env(&[("FRAISEQL_ENV", Some(value))], || {
            assert!(is_production(), "FRAISEQL_ENV={value:?} is not a development declaration");
        });
    }
}

#[test]
fn kubernetes_and_profile_can_only_force_production_on() {
    with_env(
        &[
            ("FRAISEQL_ENV", Some("development")),
            ("KUBERNETES_SERVICE_HOST", Some("10.0.0.1")),
        ],
        || {
            assert!(
                is_production(),
                "a Kubernetes pod is production even if FRAISEQL_ENV says otherwise"
            );
        },
    );
    with_env(
        &[
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", Some("production")),
        ],
        || {
            assert!(is_production(), "an explicit production profile wins over a development env");
        },
    );
}

#[test]
fn a_bypass_is_inert_unless_development_is_declared() {
    with_env(&[], || {
        assert!(!insecure_bypass_allowed(true), "unset environment must refuse the bypass");
    });
    with_env(&[("FRAISEQL_ENV", Some("production"))], || {
        assert!(!insecure_bypass_allowed(true));
    });
    with_env(&[("FRAISEQL_ENV", Some("development"))], || {
        assert!(insecure_bypass_allowed(true), "the escape hatch must still work when asked for");
        assert!(!insecure_bypass_allowed(false), "and must stay off when not requested");
    });
}

#[test]
fn opt_in_parsing_accepts_only_one_and_true() {
    temp_env::with_var("FRAISEQL_TEST_OPT_IN", Some("1"), || {
        assert!(env_opt_in("FRAISEQL_TEST_OPT_IN"));
    });
    temp_env::with_var("FRAISEQL_TEST_OPT_IN", Some("TRUE"), || {
        assert!(env_opt_in("FRAISEQL_TEST_OPT_IN"));
    });
    for value in ["yes", "0", "false", "on", ""] {
        temp_env::with_var("FRAISEQL_TEST_OPT_IN", Some(value), || {
            assert!(!env_opt_in("FRAISEQL_TEST_OPT_IN"), "{value:?} must not read as an opt-in");
        });
    }
    temp_env::with_var_unset("FRAISEQL_TEST_OPT_IN", || {
        assert!(!env_opt_in("FRAISEQL_TEST_OPT_IN"));
    });
}

#[test]
fn the_two_declaration_predicates_are_disjoint() {
    for value in ["development", "dev"] {
        assert!(declares_development(value) && !declares_production(value));
    }
    for value in ["production", "prod"] {
        assert!(declares_production(value) && !declares_development(value));
    }
}
