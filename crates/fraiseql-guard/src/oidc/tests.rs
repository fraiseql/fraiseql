use super::{IssuerMatch, check_discovered_issuer};

const URL: &str = "https://idp.example/.well-known/openid-configuration";

fn accepts(configured: &str, discovered: &str, rule: IssuerMatch) -> bool {
    check_discovered_issuer(URL, configured, discovered, rule).is_ok()
}

#[test]
fn identical_issuers_pass_under_either_rule() {
    for rule in [IssuerMatch::Exact, IssuerMatch::IgnoringTrailingSlash] {
        assert!(accepts("https://idp.example", "https://idp.example", rule), "{rule:?}");
        assert!(accepts("https://idp.example/", "https://idp.example/", rule), "{rule:?}");
    }
}

#[test]
fn a_trailing_slash_difference_is_refused_exactly_and_passes_otherwise() {
    for (configured, discovered) in [
        ("https://idp.example", "https://idp.example/"),
        ("https://idp.example/", "https://idp.example"),
    ] {
        assert!(
            !accepts(configured, discovered, IssuerMatch::Exact),
            "{configured} {discovered}"
        );
        assert!(
            accepts(configured, discovered, IssuerMatch::IgnoringTrailingSlash),
            "{configured} {discovered}"
        );
    }
}

#[test]
fn only_one_trailing_slash_is_ignored() {
    assert!(!accepts(
        "https://idp.example",
        "https://idp.example//",
        IssuerMatch::IgnoringTrailingSlash
    ));
}

#[test]
fn any_other_difference_is_refused_under_either_rule() {
    for discovered in [
        "https://other.example",
        "https://idp.example/realms/other",
        "http://idp.example",
        "https://idp.example:8443",
        "https://IDP.example",
        "",
    ] {
        for rule in [IssuerMatch::Exact, IssuerMatch::IgnoringTrailingSlash] {
            assert!(!accepts("https://idp.example", discovered, rule), "{discovered:?} {rule:?}");
        }
    }
}

#[test]
fn the_refusal_names_the_url_and_both_issuers() {
    let error = check_discovered_issuer(
        URL,
        "https://idp.example",
        "https://other.example",
        IssuerMatch::IgnoringTrailingSlash,
    )
    .expect_err("another issuer was accepted");
    assert!(error.contains(URL), "{error}");
    assert!(error.contains("\"https://idp.example\""), "{error}");
    assert!(error.contains("\"https://other.example\""), "{error}");
}
