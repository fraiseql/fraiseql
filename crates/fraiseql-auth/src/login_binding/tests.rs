#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

/// The `name=value` part of a `Set-Cookie` value.
fn pair(cookie: &HeaderValue) -> String {
    cookie.to_str().unwrap().split(';').next().unwrap().to_string()
}

fn presenting(cookie: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, HeaderValue::from_str(cookie).unwrap());
    headers
}

#[test]
fn the_browser_that_started_the_login_matches() {
    let started = start("state-1", ReturnsBy::Redirect, 600);
    let headers = presenting(&format!("other=1; {}", pair(&started.cookie)));
    assert!(matches(&headers, "state-1", &started.digest));
}

#[test]
fn no_cookie_another_value_or_another_states_cookie_does_not_match() {
    let started = start("state-1", ReturnsBy::Redirect, 600);
    let other = start("state-2", ReturnsBy::Redirect, 600);
    assert!(!matches(&HeaderMap::new(), "state-1", &started.digest));
    let (name, _) = pair(&started.cookie)
        .split_once('=')
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .unwrap();
    assert!(!matches(&presenting(&format!("{name}=guess")), "state-1", &started.digest));
    assert!(!matches(&presenting(&format!("{name}=")), "state-1", &started.digest));
    assert!(!matches(&presenting(&pair(&other.cookie)), "state-1", &started.digest));
}

#[test]
fn the_cookie_is_host_only_http_only_secure_and_follows_how_the_provider_returns() {
    let lax = start("s", ReturnsBy::Redirect, 600).cookie.to_str().unwrap().to_string();
    let none = start("s", ReturnsBy::FormPost, 600).cookie.to_str().unwrap().to_string();
    for cookie in [&lax, &none] {
        assert!(cookie.starts_with("__Host-fraiseql_login_"), "{cookie}");
        for attribute in ["; Path=/;", "; HttpOnly;", "; Secure;", "; Max-Age=600"] {
            assert!(cookie.contains(attribute), "{cookie}");
        }
        assert!(!cookie.contains("Domain"), "{cookie}");
    }
    assert!(lax.contains("SameSite=Lax"), "{lax}");
    assert!(none.contains("SameSite=None"), "{none}");
}

#[test]
fn clearing_empties_the_cookie_of_that_state() {
    let started = start("state-1", ReturnsBy::FormPost, 600);
    let cleared = clear("state-1", ReturnsBy::FormPost).to_str().unwrap().to_string();
    let name = pair(&started.cookie).split_once('=').unwrap().0.to_string();
    assert!(cleared.starts_with(&format!("{name}=;")), "{cleared}");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    assert!(cleared.contains("SameSite=None"), "{cleared}");
}

#[test]
fn the_stored_digest_is_not_the_cookie_value() {
    let started = start("state-1", ReturnsBy::Redirect, 600);
    let value = pair(&started.cookie).split_once('=').unwrap().1.to_string();
    assert_ne!(value, started.digest);
    assert_eq!(started.digest.len(), 64);
}
