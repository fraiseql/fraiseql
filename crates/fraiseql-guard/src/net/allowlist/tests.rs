//! Host patterns, as every guest's outbound call reads them.

use super::allows;

fn patterns(list: &[&str]) -> Vec<String> {
    list.iter().map(|p| (*p).to_owned()).collect()
}

#[test]
fn an_empty_list_permits_nothing() {
    assert!(!allows("api.example.com", &[]));
}

#[test]
fn a_star_permits_any_host() {
    assert!(allows("api.example.com", &patterns(&["*"])));
}

#[test]
fn an_exact_pattern_permits_that_host_only() {
    let list = patterns(&["api.example.com"]);
    assert!(allows("api.example.com", &list));
    assert!(!allows("www.example.com", &list));
    assert!(!allows("api.example.com.evil.test", &list));
}

#[test]
fn a_subdomain_pattern_permits_subdomains_but_not_the_domain() {
    let list = patterns(&["*.example.com"]);
    assert!(allows("api.example.com", &list));
    assert!(allows("a.b.example.com", &list));
    assert!(!allows("example.com", &list));
    assert!(!allows("badexample.com", &list));
}

#[test]
fn host_names_compare_without_regard_to_case() {
    assert!(allows("api.example.com", &patterns(&["API.Example.com"])));
    assert!(allows("API.example.COM", &patterns(&["*.Example.com"])));
}

#[test]
fn an_ip_literal_matches_exactly() {
    let list = patterns(&["203.0.113.7"]);
    assert!(allows("203.0.113.7", &list));
    assert!(!allows("203.0.113.70", &list));
}
