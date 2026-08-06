//! The production default, asserted directly.
//!
//! `fraiseql-server`'s `production_safety_test.rs` asserted one answer for an
//! unset `FRAISEQL_ENV` and `fraiseql-observers`' `insecure_guard` asserted the
//! opposite. Both were "tested behaviour"; only one can be right. These are the
//! assertions the whole workspace now shares.

use super::{
    BypassDecision, declares_development, declares_production, env_opt_in, insecure_bypass,
    insecure_bypass_allowed, is_production,
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

// ── #882: an escape hatch that is refused must say so ─────────────────────────

/// Posture markers cleared, then the bypass requested, so `insecure_bypass`
/// decides on the posture the test sets and nothing else.
fn with_posture<T>(env: Option<&str>, f: impl FnOnce() -> T + std::panic::UnwindSafe) -> T {
    let mut out = None;
    temp_env::with_vars(
        [
            ("FRAISEQL_TEST_HATCH", Some("1")),
            ("FRAISEQL_ENV", env),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || out = Some(f()),
    );
    out.expect("temp_env ran the closure")
}

#[test]
fn a_requested_bypass_is_honoured_only_in_a_declared_development_environment() {
    assert_eq!(
        with_posture(Some("development"), || insecure_bypass("FRAISEQL_TEST_HATCH")),
        BypassDecision::Honoured
    );
    assert_eq!(
        with_posture(Some("production"), || insecure_bypass("FRAISEQL_TEST_HATCH")),
        BypassDecision::RefusedInProduction,
        "#882: a bypass honoured in production is not a bypass, it is a vulnerability"
    );
    assert_eq!(
        with_posture(None, || insecure_bypass("FRAISEQL_TEST_HATCH")),
        BypassDecision::RefusedInProduction,
        "unset FRAISEQL_ENV is production — the hatch must not be honoured by default"
    );
}

#[test]
fn an_unrequested_bypass_is_not_reported_as_refused() {
    temp_env::with_vars(
        [
            ("FRAISEQL_TEST_HATCH", None::<&str>),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            assert_eq!(
                insecure_bypass("FRAISEQL_TEST_HATCH"),
                BypassDecision::NotRequested,
                "nothing was requested, so there is nothing to warn an operator about"
            );
        },
    );
}

#[test]
fn a_refused_bypass_is_logged_at_error_naming_the_variable() {
    let events = capture::install();
    with_posture(Some("production"), || {
        assert!(!insecure_bypass("FRAISEQL_TEST_HATCH").is_honoured());
    });
    let logged = events.take();
    assert!(
        logged
            .iter()
            .any(|(level, msg)| *level == tracing::Level::ERROR
                && msg.contains("FRAISEQL_TEST_HATCH")),
        "#882: a refused bypass must be visible in the log stream — otherwise it \
         reaches the operator as an unexplained connection failure. Captured: {logged:?}"
    );
}

#[test]
fn an_honoured_bypass_is_logged_at_warn_naming_the_variable() {
    let events = capture::install();
    with_posture(Some("development"), || {
        assert!(insecure_bypass("FRAISEQL_TEST_HATCH").is_honoured());
    });
    let logged = events.take();
    assert!(
        logged
            .iter()
            .any(|(level, msg)| *level == tracing::Level::WARN
                && msg.contains("FRAISEQL_TEST_HATCH")),
        "an active bypass must be visible too — guards are off. Captured: {logged:?}"
    );
}

#[test]
fn an_unrequested_bypass_logs_nothing() {
    let events = capture::install();
    temp_env::with_vars(
        [
            ("FRAISEQL_TEST_HATCH", None::<&str>),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            let _ = insecure_bypass("FRAISEQL_TEST_HATCH");
        },
    );
    assert!(
        events.take().is_empty(),
        "the overwhelmingly common case must not add a line to every log stream"
    );
}

/// Captures `tracing` events emitted on the current thread.
///
/// Scoped to the calling thread (`set_default`, not `set_global_default`) so the
/// log-assertion tests do not observe each other's events under the default
/// parallel test harness.
mod capture {
    use std::sync::{Arc, Mutex};

    use tracing::{Level, Subscriber, subscriber::DefaultGuard};
    use tracing_subscriber::{Layer, Registry, layer::Context, prelude::*};

    type Events = Arc<Mutex<Vec<(Level, String)>>>;

    /// Holds the captured events and keeps the subscriber installed until dropped.
    pub struct Captured {
        events: Events,
        _guard: DefaultGuard,
    }

    impl Captured {
        /// Every event recorded so far, as `(level, rendered message)`.
        pub fn take(&self) -> Vec<(Level, String)> {
            self.events.lock().expect("capture mutex").clone()
        }
    }

    struct CapturingLayer {
        events: Events,
    }

    impl<S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>> Layer<S>
        for CapturingLayer
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            struct MessageVisitor<'a>(&'a mut String);
            impl tracing::field::Visit for MessageVisitor<'_> {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        use std::fmt::Write as _;
                        let _ = write!(self.0, "{value:?}");
                    }
                }
            }
            let mut message = String::new();
            event.record(&mut MessageVisitor(&mut message));
            self.events
                .lock()
                .expect("capture mutex")
                .push((*event.metadata().level(), message));
        }
    }

    /// Install a thread-local capturing subscriber for the rest of the test.
    pub fn install() -> Captured {
        let events: Events = Arc::default();
        let guard = tracing::subscriber::set_default(Registry::default().with(CapturingLayer {
            events: Arc::clone(&events),
        }));
        Captured {
            events,
            _guard: guard,
        }
    }
}
