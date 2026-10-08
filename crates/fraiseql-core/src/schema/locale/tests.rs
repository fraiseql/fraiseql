#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::BTreeMap;

use super::*;

fn config(default: &str, allowed: &[&str], fallback: &[(&str, &str)]) -> Result<LocaleConfig> {
    LocaleConfig::new(
        default,
        allowed.iter().map(ToString::to_string).collect(),
        fallback.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect(),
        default_resolve(),
    )
}

fn plan() -> LocaleConfig {
    config("en-US", &["en-US", "en-GB", "fr", "fr-FR", "de-DE"], &[("fr-CA", "fr-FR")]).unwrap()
}

fn refusal(result: Result<LocaleConfig>) -> String {
    result.expect_err("refused").to_string()
}

#[test]
fn each_allowed_locale_has_its_chain() {
    let c = config("en-US", &["en-US", "fr", "fr-CA", "fr-FR"], &[("fr-CA", "fr-FR")]).unwrap();
    assert_eq!(c.chain("fr-CA").unwrap(), ["fr-CA", "fr-FR", "fr", "en-US"]);
    assert_eq!(c.chain("fr-FR").unwrap(), ["fr-FR", "fr", "en-US"]);
    assert_eq!(c.chain("en-US").unwrap(), ["en-US"]);
    assert!(c.chain("de-DE").is_none(), "only allowed tags have a chain");
}

#[test]
fn a_chain_follows_fallbacks_transitively_and_never_repeats() {
    let c =
        config("en", &["en", "pt", "pt-BR", "pt-PT"], &[("pt-AO", "pt-PT"), ("pt-PT", "pt-BR")])
            .unwrap();
    assert_eq!(c.chain("pt-PT").unwrap(), ["pt-PT", "pt-BR", "pt", "en"]);
}

#[test]
fn a_requested_tag_matches_exactly_then_by_fallback_then_by_truncation() {
    let c = plan();
    assert_eq!(c.match_tag("fr-FR"), Some("fr-FR"));
    assert_eq!(c.match_tag("FR-fr"), Some("fr-FR"), "case-insensitively");
    assert_eq!(
        c.match_tag("fr-CA"),
        Some("fr-FR"),
        "the explicit fallback wins over truncation"
    );
    assert_eq!(c.match_tag("fr-BE"), Some("fr"), "truncation");
    assert_eq!(c.match_tag("de-AT"), None, "`de` is not allowed");
    assert_eq!(c.match_tag("zh-Hant-x-a"), None);
    for junk in ["", "*", "fr'; --", "x".repeat(40).as_str()] {
        assert_eq!(c.match_tag(junk), None, "{junk:?}");
    }
}

fn resolve(c: &LocaleConfig, argument: Option<&str>, header: Option<&str>) -> String {
    let arg = |_: &str| argument.map(ToString::to_string);
    let hdr = |name: &str| header.filter(|_| name == "Accept-Language").map(ToString::to_string);
    c.resolve(&LocaleInputs {
        argument:   Some(&arg),
        header:     Some(&hdr),
        enrichment: None,
    })
    .to_string()
}

#[test]
fn sources_are_tried_in_order_and_fall_through() {
    let c = plan();
    assert_eq!(resolve(&c, Some("de-DE"), Some("fr")), "de-DE", "argument before header");
    assert_eq!(resolve(&c, Some("xx"), Some("fr")), "fr", "an unknown argument falls through");
    assert_eq!(resolve(&c, None, Some("fr-CA,fr;q=0.9")), "fr-FR");
    assert_eq!(resolve(&c, None, Some("fr-BE")), "fr");
    assert_eq!(resolve(&c, None, Some("da, de-DE;q=0.5, en-GB;q=0.8")), "en-GB", "by q-value");
    assert_eq!(resolve(&c, None, Some("*")), "en-US");
    assert_eq!(resolve(&c, None, Some("fr;q=0")), "en-US", "q=0 is a refusal");
    assert_eq!(
        resolve(&c, None, Some("fr;q=banana, de-DE")),
        "de-DE",
        "a malformed entry is skipped"
    );
    assert_eq!(resolve(&c, None, None), "en-US");
    let long = format!("{}de-DE", "x-y;q=0.1,".repeat(200));
    assert_eq!(resolve(&c, None, Some(&long)), "en-US", "an over-long header is not parsed");
}

#[test]
fn qvalues_parse_per_rfc_9110() {
    assert_eq!(parse_qvalue("1"), Some(1000));
    assert_eq!(parse_qvalue("1.000"), Some(1000));
    assert_eq!(parse_qvalue("0.5"), Some(500));
    assert_eq!(parse_qvalue("0.123"), Some(123));
    for bad in ["1.5", "2", "0.1234", "-1", "", "a"] {
        assert_eq!(parse_qvalue(bad), None, "{bad}");
    }
}

#[test]
fn a_tag_is_well_formed_and_canonical_or_refused() {
    for good in [
        "fr",
        "fr-FR",
        "zh-Hant-TW",
        "es-419",
        "sl-rozaj",
        "de-CH-1996",
    ] {
        config(good, &[good], &[]).unwrap();
    }
    assert!(refusal(config("fr-fr", &["fr-fr"], &[])).contains("`fr-FR`"));
    for bad in [
        "f",
        "fr_FR",
        "en-US-x-twain",
        "fr-FR'",
        "fr-",
        "-fr",
        "en-US-u-ca-gregory",
        "",
    ] {
        assert!(
            refusal(config("en", &["en", bad], &[])).contains("well-formed"),
            "{bad:?} must be refused"
        );
    }
}

#[test]
fn the_declaration_is_refused_when_inconsistent() {
    assert!(refusal(config("en", &[], &[])).contains("at least one"));
    assert!(refusal(config("de", &["en"], &[])).contains("default `de` is not in allowed"));
    assert!(refusal(config("en", &["en", "en"], &[])).contains("twice"));
    assert!(
        refusal(config("en", &["en"], &[("fr-CA", "fr-FR")])).contains("target is not in allowed")
    );
    assert!(refusal(config("en", &["en", "fr"], &[("fr", "fr")])).contains("names itself"));
    assert!(
        refusal(config("en", &["en", "fr", "de"], &[("fr", "de"), ("de", "fr")])).contains("cycle")
    );
    let mut dup = plan();
    dup.resolve.push(LocaleSource::Argument {
        argument: "locale".to_string(),
    });
    assert!(dup.validate().unwrap_err().to_string().contains("twice"));
    let mut bad = plan();
    bad.resolve = vec![LocaleSource::Header {
        header: "x y".to_string(),
    }];
    assert!(bad.validate().is_err());
}

#[test]
fn the_chains_are_not_read_from_a_compiled_file() {
    let json = serde_json::json!({
        "default": "en", "allowed": ["en"], "chains": {"en": ["en'); DROP TABLE x; --"]}
    });
    assert!(serde_json::from_value::<LocaleConfig>(json).is_err(), "unknown field refused");
    let mut loaded: LocaleConfig =
        serde_json::from_value(serde_json::json!({"default": "en", "allowed": ["en"]})).unwrap();
    assert!(loaded.chain("en").is_none(), "no chain until validated");
    loaded.validate().unwrap();
    assert_eq!(loaded.chain("en").unwrap(), ["en"]);
}

#[test]
fn the_toml_shape_of_a_source_deserializes() {
    let sources: Vec<LocaleSource> = serde_json::from_value(serde_json::json!([
        {"argument": "locale"}, {"header": "Accept-Language"}, {"enrichment": "user_locale"}
    ]))
    .unwrap();
    assert_eq!(sources.len(), 3);
    assert!(serde_json::from_value::<LocaleSource>(serde_json::json!({"cookie": "x"})).is_err());
    let _ = BTreeMap::<String, String>::new();
}
