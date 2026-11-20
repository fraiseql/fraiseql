use std::collections::HashMap;

use super::{configured_proxy, direct_connection_warning};

fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    let vars: HashMap<&str, &str> = vars.iter().copied().collect();
    move |name| vars.get(name).map(ToString::to_string)
}

#[test]
fn each_proxy_variable_in_either_case_is_found() {
    for name in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        assert_eq!(configured_proxy(env(&[(name, "http://proxy:3128")])), Some(name));
    }
}

#[test]
fn no_proxy_variable_or_a_blank_one_is_none() {
    assert_eq!(configured_proxy(env(&[])), None);
    assert_eq!(configured_proxy(env(&[("HTTP_PROXY", "  ")])), None);
    assert_eq!(configured_proxy(env(&[("NO_PROXY", "localhost")])), None);
}

#[test]
fn the_warning_names_the_variable_and_says_deliveries_connect_directly() {
    let warning = direct_connection_warning("HTTPS_PROXY");

    assert!(warning.contains("HTTPS_PROXY"), "{warning}");
    assert!(warning.contains("connect directly"), "{warning}");
}
