#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design
use super::*;

#[test]
fn test_default_rules_exist() {
    let rules = get_default_rules();
    assert!(!rules.is_empty());
    println!("Total default rules: {}", rules.len());
}

#[test]
fn test_email_domain_rule() {
    let rules = get_default_rules();
    assert!(rules.contains_key("email_domain_eq"));
    assert!(rules.contains_key("email_domain_in"));
}

#[test]
fn test_country_code_rule() {
    let rules = get_default_rules();
    assert!(rules.contains_key("country_code_eq"));
}
