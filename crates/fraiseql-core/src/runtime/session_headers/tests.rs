#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::schema::SessionVariableMapping;

fn config(headers: &[&str]) -> SessionVariablesConfig {
    SessionVariablesConfig {
        variables:         headers
            .iter()
            .enumerate()
            .map(|(i, h)| SessionVariableMapping {
                name:   format!("app.v{i}"),
                source: SessionVariableSource::Header {
                    header: (*h).to_string(),
                },
            })
            .chain(std::iter::once(SessionVariableMapping {
                name:   "app.lit".to_string(),
                source: SessionVariableSource::Literal {
                    value: "x".to_string(),
                },
            }))
            .collect(),
        inject_started_at: false,
    }
}

fn sent(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Vec<Option<String>> {
    move |name| {
        pairs
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| Some((*v).to_string()))
            .collect()
    }
}

#[test]
fn only_the_named_headers_are_read_case_insensitively() {
    let h = SessionHeaders::resolve(
        &config(&["X-Region"]),
        &sent(&[("x-region", "eu"), ("x-other", "no")]),
    )
    .unwrap();
    assert_eq!(h.get("x-region"), Some("eu"), "the named header, lower-cased");
    assert_eq!(h.get("X-REGION"), Some("eu"), "looked up in any case");
    assert_eq!(h.get("x-other"), None, "a header no mapping names is not read");
}

#[test]
fn an_unsent_header_is_absent() {
    let h = SessionHeaders::resolve(&config(&["x-region"]), &sent(&[])).unwrap();
    assert!(h.is_empty(), "nothing sent, nothing resolved");
}

#[test]
fn a_header_sent_twice_is_refused() {
    let err = SessionHeaders::resolve(
        &config(&["x-region"]),
        &sent(&[("x-region", "eu"), ("x-region", "us")]),
    )
    .unwrap_err();
    assert!(err.to_string().contains("more than once"), "{err}");
}

#[test]
fn a_non_utf8_header_is_refused() {
    let err = SessionHeaders::resolve(&config(&["x-region"]), &|_| vec![None]).unwrap_err();
    assert!(err.to_string().contains("UTF-8"), "{err}");
}

#[test]
fn an_oversized_header_is_refused_not_truncated() {
    let at_bound = "a".repeat(MAX_SESSION_HEADER_BYTES);
    let over = "a".repeat(MAX_SESSION_HEADER_BYTES + 1);
    let ok =
        SessionHeaders::resolve(&config(&["x-region"]), &|_| vec![Some(at_bound.clone())]).unwrap();
    assert_eq!(ok.get("x-region").map(str::len), Some(MAX_SESSION_HEADER_BYTES), "at the bound");
    let err =
        SessionHeaders::resolve(&config(&["x-region"]), &|_| vec![Some(over.clone())]).unwrap_err();
    assert!(err.to_string().contains("longer than"), "{err}");
}

#[test]
fn the_scoped_value_is_read_inside_the_scope_only() {
    let h = SessionHeaders::resolve(&config(&["x-region"]), &sent(&[("x-region", "eu")])).unwrap();
    assert_eq!(scoped_session_header("x-region"), None, "outside any scope");
    with_session_headers_sync(h, || {
        assert_eq!(scoped_session_header("X-Region").as_deref(), Some("eu"), "inside the scope");
    });
}
