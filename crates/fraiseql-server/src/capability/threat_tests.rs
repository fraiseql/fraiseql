//! The definition-of-shipped gate on threat models: every registration says what it guards,
//! and each input it must refuse is shown refused by a hostile fixture.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::path::Path;

use super::{REGISTRY, Refusal, ThreatModel};

/// Registrations with no hostile fixture yet, each with the issue that tracks it.
/// Shrink-only: the gate fails when one of these gains a refusal or a hostile fixture, so
/// the entry goes with it.
const NO_HOSTILE_FIXTURE_YET: &[(&str, &str)] = &[
    ("functions", "#1845"),
    ("sources", "#1845"),
    ("grpc", "#1845"),
    ("federation", "#1845"),
    ("observers", "#1845"),
    ("inbound_email", "#1845"),
    ("cdc_outbound", "#1845"),
    ("subscription_kafka", "#1845"),
];

/// What the gate reads of one fixture's `fixture.toml`.
#[derive(Debug)]
struct FixtureFacts {
    /// Its directory name under `tests/capabilities/`.
    dir:        String,
    capability: String,
    hostile:    bool,
    /// The key of its `[effect]`.
    effect:     String,
    /// Whether that effect is a `row` expecting no rows.
    empty_row:  bool,
}

/// The facts of every fixture under `tests/capabilities/`.
fn fixture_facts() -> Vec<FixtureFacts> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/capabilities");
    let mut facts: Vec<FixtureFacts> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("fixture.toml").is_file())
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(path.join("fixture.toml")).unwrap();
            facts_of(&name, &text)
        })
        .collect();
    facts.sort_by(|a, b| a.dir.cmp(&b.dir));
    facts
}

/// The facts of the fixture `dir`, whose `fixture.toml` reads `text`.
fn facts_of(dir: &str, text: &str) -> FixtureFacts {
    let value: toml::Value = toml::from_str(text).unwrap_or_else(|e| panic!("fixture {dir}: {e}"));
    let effect = value
        .get("effect")
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("fixture {dir} has no [effect]"));
    let (kind, body) = effect
        .iter()
        .next()
        .unwrap_or_else(|| panic!("fixture {dir}: [effect] is empty"));
    FixtureFacts {
        dir:        dir.to_owned(),
        capability: value
            .get("capability")
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("fixture {dir} names no capability"))
            .to_owned(),
        hostile:    value.get("role").and_then(toml::Value::as_str) == Some("hostile"),
        effect:     kind.clone(),
        empty_row:  kind == "row"
            && body.get("expect").and_then(toml::Value::as_array).is_some_and(Vec::is_empty),
    }
}

/// Every way `registrations`, `fixtures` and `not_yet` fall short of the bar.
fn shortfalls(
    registrations: &[(&str, ThreatModel)],
    fixtures: &[FixtureFacts],
    not_yet: &[(&str, &str)],
) -> Vec<String> {
    let mut failures = Vec::new();
    let blank = |list: &[&str]| list.is_empty() || list.iter().any(|item| item.trim().is_empty());
    for (name, threat) in registrations {
        if blank(threat.assets) {
            failures.push(format!("{name}: its threat model names no asset"));
        }
        if blank(threat.crosses) {
            failures.push(format!("{name}: its threat model names no trust boundary"));
        }
        let listed = not_yet.iter().any(|(listed, _)| listed == name);
        let hostile: Vec<&FixtureFacts> =
            fixtures.iter().filter(|f| f.hostile && f.capability == *name).collect();
        if listed {
            if !threat.refuses.is_empty() || !hostile.is_empty() {
                failures.push(format!(
                    "{name} has a refusal or a hostile fixture now: remove its \
                     NO_HOSTILE_FIXTURE_YET entry"
                ));
            }
        } else if threat.refuses.is_empty() {
            failures.push(format!(
                "{name} refuses nothing: add a refusal and the hostile fixture that sends it, \
                 or list it in NO_HOSTILE_FIXTURE_YET with its issue"
            ));
        }
        for Refusal { input, fixture } in threat.refuses {
            if input.trim().is_empty() {
                failures.push(format!("{name}: a refusal names no input"));
            }
            match fixtures.iter().find(|f| f.dir == *fixture) {
                None => {
                    failures
                        .push(format!("{name}: no fixture {fixture} under tests/capabilities/"));
                },
                Some(f) if f.capability != *name || !f.hostile => failures.push(format!(
                    "{name}: fixture {fixture} is not a hostile fixture of {name}: give it \
                     `capability = \"{name}\"` and `role = \"hostile\"`"
                )),
                Some(_) => {},
            }
        }
        for f in hostile {
            let naming = threat.refuses.iter().filter(|r| r.fixture == f.dir).count();
            if naming != 1 {
                failures.push(format!(
                    "fixture {}: {naming} refusals of {name} name it, not one",
                    f.dir
                ));
            }
        }
    }
    for f in fixtures.iter().filter(|f| f.hostile) {
        if !registrations.iter().any(|(name, _)| *name == f.capability) {
            failures.push(format!("fixture {}: no registration is named {}", f.dir, f.capability));
        }
        if !(f.effect == "none" || f.effect == "refused" || f.empty_row) {
            failures.push(format!(
                "fixture {}: a hostile fixture's effect says nothing followed (`none`, \
                 `refused`, or a `row` expecting no rows), not `{}`",
                f.dir, f.effect
            ));
        }
    }
    for (name, issue) in not_yet {
        if !registrations.iter().any(|(registered, _)| registered == name) {
            failures.push(format!("NO_HOSTILE_FIXTURE_YET names {name}, which is not registered"));
        }
        if !issue.starts_with('#') {
            failures.push(format!("NO_HOSTILE_FIXTURE_YET: {name} names no issue"));
        }
    }
    failures
}

/// Every registration says what it guards, and each one either refuses an input a hostile
/// fixture shows refused or is listed, with its issue. Needs no database.
#[test]
fn every_registration_has_a_threat_model_and_a_hostile_fixture() {
    let registrations: Vec<(&str, ThreatModel)> =
        REGISTRY.iter().map(|entry| (entry.meta.name, entry.threat)).collect();
    let failures = shortfalls(&registrations, &fixture_facts(), NO_HOSTILE_FIXTURE_YET);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The rules [`shortfalls`] applies, on inputs written here.
mod rules {
    use super::*;

    const GUARDED: ThreatModel = ThreatModel {
        assets:  &["the notes table"],
        crosses: &["a client's request into the executor"],
        refuses: &[Refusal {
            input:   "a request with no bearer",
            fixture: "rest_refuses",
        }],
    };

    fn hostile(dir: &str, capability: &str, effect: &str) -> FixtureFacts {
        let effect = match effect {
            "none" => "none = { of = { row = { query = \"SELECT 1\", expect = [] } } }",
            "empty_row" => "row = { query = \"SELECT 1\", expect = [] }",
            "row" => "row = { query = \"SELECT 1\", expect = [[\"1\"]] }",
            other => panic!("{other}"),
        };
        facts_of(
            dir,
            &format!("capability = \"{capability}\"\nrole = \"hostile\"\n[effect]\n{effect}"),
        )
    }

    #[test]
    fn a_guarded_registration_with_its_hostile_fixture_passes() {
        for effect in ["none", "empty_row"] {
            let fixtures = [hostile("rest_refuses", "rest", effect)];
            assert_eq!(shortfalls(&[("rest", GUARDED)], &fixtures, &[]), Vec::<String>::new());
        }
    }

    #[test]
    fn a_threat_model_with_no_asset_or_no_boundary_fails() {
        let fixtures = [hostile("rest_refuses", "rest", "none")];
        for threat in [
            ThreatModel {
                assets: &[],
                ..GUARDED
            },
            ThreatModel {
                assets: &[" "],
                ..GUARDED
            },
            ThreatModel {
                crosses: &[],
                ..GUARDED
            },
        ] {
            assert_eq!(shortfalls(&[("rest", threat)], &fixtures, &[]).len(), 1, "{threat:?}");
        }
    }

    #[test]
    fn a_registration_that_refuses_nothing_fails_unless_listed() {
        let open = ThreatModel {
            refuses: &[],
            ..GUARDED
        };
        assert_eq!(shortfalls(&[("rest", open)], &[], &[]).len(), 1);
        let listed = shortfalls(&[("rest", open)], &[], &[("rest", "#1")]);
        assert!(listed.is_empty(), "{listed:?}");
    }

    #[test]
    fn a_listed_registration_that_gained_a_refusal_or_a_fixture_fails() {
        let fixtures = [hostile("rest_refuses", "rest", "none")];
        let gained_a_refusal = shortfalls(&[("rest", GUARDED)], &fixtures, &[("rest", "#1")]);
        assert!(
            !gained_a_refusal.is_empty(),
            "expected a shortfall for a listed guarded registration"
        );
        let open = ThreatModel {
            refuses: &[],
            ..GUARDED
        };
        let gained_a_fixture = shortfalls(&[("rest", open)], &fixtures, &[("rest", "#1")]);
        assert!(
            !gained_a_fixture.is_empty(),
            "expected a shortfall for a listed registration with a fixture"
        );
    }

    #[test]
    fn a_refusal_names_an_existing_hostile_fixture_of_its_own_capability() {
        let elsewhere = [hostile("rest_refuses", "mcp", "none")];
        let positive = [facts_of(
            "rest_refuses",
            "capability = \"rest\"\n[effect]\nrow = { query = \"SELECT 1\", expect = [] }",
        )];
        for fixtures in [&[][..], &elsewhere[..], &positive[..]] {
            let regs = [
                ("rest", GUARDED),
                (
                    "mcp",
                    ThreatModel {
                        refuses: &[],
                        ..GUARDED
                    },
                ),
            ];
            let failures = shortfalls(&regs, fixtures, &[("mcp", "#1")]);
            assert!(failures.iter().any(|f| f.starts_with("rest:")), "{fixtures:?}: {failures:?}");
        }
    }

    #[test]
    fn a_hostile_fixture_no_refusal_names_fails() {
        let fixtures = [
            hostile("rest_refuses", "rest", "none"),
            hostile("rest_refuses_too", "rest", "none"),
        ];
        let failures = shortfalls(&[("rest", GUARDED)], &fixtures, &[]);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("rest_refuses_too"), "{failures:?}");
    }

    #[test]
    fn a_hostile_fixture_whose_effect_is_something_happening_fails() {
        let fixtures = [hostile("rest_refuses", "rest", "row")];
        let failures = shortfalls(&[("rest", GUARDED)], &fixtures, &[]);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("says nothing followed"), "{failures:?}");
    }

    #[test]
    fn a_gap_list_entry_names_a_registration_and_an_issue() {
        let open = ThreatModel {
            refuses: &[],
            ..GUARDED
        };
        assert_eq!(shortfalls(&[("rest", open)], &[], &[("rest", "#1"), ("gone", "#1")]).len(), 1);
        assert_eq!(shortfalls(&[("rest", open)], &[], &[("rest", "soon")]).len(), 1);
    }
}
