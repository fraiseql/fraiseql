use super::{
    super::{set_runtime_acronyms, to_snake_case},
    Acronyms, to_snake_case_with,
};

/// The project term `utils::tests` installs process-wide. Installing the same list here
/// keeps the first-call-wins global the same whichever test runs first.
fn installed() -> Vec<String> {
    vec!["acmewidget7".to_string()]
}

const NAMES: &[&str] = &[
    "id",
    "firstName",
    "HTTPResponse",
    "phone1",
    "dns1Id",
    "s3Bucket",
    "hostIpv4",
    "emissionEc2Kg",
    "Ec2Instance",
    "acmewidget7",
    "acmewidget7Count",
    "already_snake",
    "phone_1",
];

#[test]
fn a_value_set_snake_cases_as_the_installed_one_does() {
    set_runtime_acronyms(&installed());
    let project = Acronyms::with_extra(&installed());
    for name in NAMES {
        assert_eq!(to_snake_case_with(name, &project), to_snake_case(name), "{name}");
    }
}

#[test]
fn a_project_acronym_absent_from_the_value_set_splits() {
    let defaults = Acronyms::with_extra(&[]);
    let project = Acronyms::with_extra(&installed());
    assert_eq!(to_snake_case_with("acmewidget7Count", &defaults), "acmewidget_7_count");
    assert_eq!(to_snake_case_with("acmewidget7Count", &project), "acmewidget7_count");
}

#[test]
fn extra_terms_are_trimmed_and_lowercased_and_empties_ignored() {
    let set = Acronyms::with_extra(&[" Zeta9 ".to_string(), String::new(), "  ".to_string()]);
    assert!(set.contains("zeta9"));
    assert!(set.contains("s3"), "the built-in defaults stay");
    assert!(!set.contains(""));
}
