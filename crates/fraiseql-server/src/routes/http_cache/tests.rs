//! Tests for the HTTP cache decision.

use axum::http::{HeaderMap, HeaderValue};
use fraiseql_core::schema::{CompiledSchema, SessionVariableMapping, SessionVariableSource};
use serde_json::json;

use super::{Answered, Caching, apply, decide};

fn answered(body: &serde_json::Value) -> Answered<'_> {
    Answered {
        body,
        ttl: Some(60),
        principal: false,
        tenant: false,
        s_maxage: Some(600),
    }
}

fn cache_control(caching: Caching, negotiated: &[&str]) -> (String, String) {
    let mut headers = HeaderMap::new();
    apply(&mut headers, caching, Some(&CompiledSchema::default()), negotiated);
    let header = |name| {
        headers
            .get(name)
            .map_or_else(String::new, |v: &HeaderValue| v.to_str().unwrap_or_default().to_string())
    };
    (header("cache-control"), header("vary"))
}

#[test]
fn an_anonymous_untenanted_answer_is_public_with_its_s_maxage() {
    let body = json!({ "data": {} });
    let caching = decide(&answered(&body), &HeaderMap::new(), &CompiledSchema::default());
    assert_eq!(
        caching,
        Caching::Public {
            max_age:  60,
            s_maxage: Some(600),
        }
    );
    assert_eq!(cache_control(caching, &[]).0, "public, max-age=60, s-maxage=600");
}

/// The tenant decides on its own: a tenant resolved from the `Host`, with no
/// header naming it, is still that tenant's answer.
#[test]
fn a_tenant_with_no_header_naming_it_is_private() {
    let body = json!({ "data": {} });
    let mut facts = answered(&body);
    facts.tenant = true;
    let caching = decide(&facts, &HeaderMap::new(), &CompiledSchema::default());
    assert_eq!(caching, Caching::Private { max_age: 60 });
    assert_eq!(cache_control(caching, &[]).0, "private, max-age=60", "no s-maxage");
}

#[test]
fn a_principal_is_private() {
    let body = json!({ "data": {} });
    let mut facts = answered(&body);
    facts.principal = true;
    let caching = decide(&facts, &HeaderMap::new(), &CompiledSchema::default());
    assert_eq!(caching, Caching::Private { max_age: 60 });
}

#[test]
fn a_header_session_variable_makes_every_answer_private() {
    let body = json!({ "data": {} });
    let mut schema = CompiledSchema::default();
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.org".to_string(),
        source: SessionVariableSource::Header {
            header: "x-org".to_string(),
        },
    });
    let caching = decide(&answered(&body), &HeaderMap::new(), &schema);
    assert_eq!(caching, Caching::Private { max_age: 60 });
}

#[test]
fn a_zero_or_absent_ttl_stores_nothing() {
    let body = json!({ "data": {} });
    for ttl in [None, Some(0)] {
        let mut facts = answered(&body);
        facts.ttl = ttl;
        assert_eq!(
            decide(&facts, &HeaderMap::new(), &CompiledSchema::default()),
            Caching::NoStore,
            "{ttl:?}"
        );
    }
}

#[test]
fn an_answer_with_errors_stores_nothing() {
    let body = json!({ "data": null, "errors": [] });
    assert_eq!(
        decide(&answered(&body), &HeaderMap::new(), &CompiledSchema::default()),
        Caching::NoStore
    );
}

#[test]
fn vary_names_the_transports_negotiation_headers_once() {
    let (_, vary) = cache_control(Caching::Private { max_age: 1 }, &["Accept", "accept"]);
    assert_eq!(vary, "Authorization, Cookie, X-API-Key, X-Tenant-ID, Accept");
}

#[test]
fn no_store_carries_no_vary() {
    assert_eq!(cache_control(Caching::NoStore, &["Accept"]), ("no-store".into(), String::new()));
}
