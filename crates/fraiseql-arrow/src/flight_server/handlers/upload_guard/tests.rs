//! Unit tests for the shared Flight upload guard (#953, #1028).
#![allow(clippy::unwrap_used)] // Reason: test code — panics are acceptable

use super::*;

fn allow(tables: &[&str]) -> HashSet<String> {
    tables.iter().map(|t| (*t).to_string()).collect()
}

#[test]
fn no_allow_list_refuses_everything() {
    // The default. Upload is off until an operator turns it on, per #953.
    let err = authorize_upload(None, "u1", "tb_user").expect_err("must refuse");
    assert!(err.contains("Upload is disabled"), "{err}");
}

#[test]
fn an_empty_allow_list_refuses_everything() {
    let err = authorize_upload(Some(&allow(&[])), "u1", "tb_user").expect_err("must refuse");
    assert!(err.contains("not permitted"), "{err}");
}

#[test]
fn a_listed_table_is_permitted() {
    authorize_upload(Some(&allow(&["ta_metrics"])), "u1", "ta_metrics").expect("permitted");
}

#[test]
fn an_unlisted_table_is_refused_even_when_others_are_listed() {
    let err =
        authorize_upload(Some(&allow(&["ta_metrics"])), "u1", "tb_user").expect_err("must refuse");
    assert!(err.contains("tb_user"), "the refusal names the table it refused: {err}");
}

#[test]
fn matching_is_exact_not_prefix_or_suffix() {
    // A prefix match would let `tb_user_secret` through on a `tb_user` allow-list.
    let allowed = allow(&["tb_user"]);
    for probe in [
        "tb_user_secret",
        "tb_users",
        "TB_USER",
        "core.tb_user",
        " tb_user",
    ] {
        authorize_upload(Some(&allowed), "u1", probe)
            .expect_err(&format!("'{probe}' must not satisfy a 'tb_user' allow-list"));
    }
}
