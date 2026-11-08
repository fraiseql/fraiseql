use super::*;

#[test]
fn the_claim_checks_for_and_writes_the_marker() {
    assert_eq!(
        CLAIM_SQL.matches(MARKER).count(),
        2,
        "the claim must compare the schema's comment with MARKER and write MARKER"
    );
}

#[test]
fn every_auth_script_starts_by_claiming_the_schema() {
    #[cfg(feature = "auth-saml")]
    let saml = SAML_SCRIPTS;
    #[cfg(not(feature = "auth-saml"))]
    let saml: &[&str] = &[];
    for (index, script) in SCRIPTS.iter().chain(saml).enumerate() {
        assert!(script.starts_with(CLAIM_SQL), "auth script #{index} must start with CLAIM_SQL");
    }
}
