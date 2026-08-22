//! Every compile path refuses the same self-contained configuration (#1017).
//!
//! `SchemaMerger::merge_files` — the `--types` workflow — is the one entry point
//! of six that does not call `TomlSchema::validate()`. Its exemption is real but
//! narrow: queries in the TOML may legitimately name types that only exist in
//! `types.json`, so the *type-reference* checks cannot run before the merge.
//! Everything else in `validate()` needs no type at all, and skipping it meant
//! an invalid `[server]`, `[database]`, `[auth]` or `[security]` value was
//! refused by five workflows and compiled by the sixth.
//!
//! The property under test is parity, not any one check: a self-contained rule
//! added to `validate()` later must run on every path by default rather than on
//! whichever path someone remembers.

use std::fs;

use anyhow::Result;
use fraiseql_cli::schema::SchemaMerger;
use tempfile::TempDir;

/// A TOML fragment that is invalid for a reason having nothing to do with types,
/// paired with the substring the error must name.
struct SelfContainedDefect {
    what:     &'static str,
    toml:     &'static str,
    expected: &'static str,
}

const DEFECTS: &[SelfContainedDefect] = &[
    SelfContainedDefect {
        what:     "[server] port = 0",
        toml:     "[server]\nport = 0\n",
        expected: "port must be non-zero",
    },
    SelfContainedDefect {
        what:     "[server.tls] min_version outside {1.2, 1.3}",
        toml:     "[server.tls]\nenabled = true\ncert_file = \"c.pem\"\nkey_file = \"k.pem\"\nmin_version = \"1.1\"\n",
        expected: "min_version",
    },
    SelfContainedDefect {
        what:     "[database] pool_min above pool_max",
        toml:     "[database]\npool_min = 10\npool_max = 2\n",
        expected: "pool_min",
    },
    SelfContainedDefect {
        what:     "[database] connect_timeout_ms = 0",
        toml:     "[database]\nconnect_timeout_ms = 0\n",
        expected: "connect_timeout_ms",
    },
    SelfContainedDefect {
        what:     "[auth] incomplete PKCE client group",
        toml:     "[auth]\nissuer = \"https://idp.example.com\"\nclient_id = \"abc\"\n",
        expected: "incomplete",
    },
    SelfContainedDefect {
        what:     "[federation.circuit_breaker] zero failure_threshold",
        toml:     "[federation.circuit_breaker]\nfailure_threshold = 0\n",
        expected: "failure_threshold",
    },
    SelfContainedDefect {
        what:     "[security.rate_limiting] unparseable trusted_proxy_cidrs",
        toml:     "[security.rate_limiting]\ntrusted_proxy_cidrs = [\"not-a-cidr\"]\n",
        expected: "CIDR",
    },
    SelfContainedDefect {
        what:     "[hierarchies] empty table",
        toml:     "[hierarchies.org]\ntable = \"\"\npath_column = \"path\"\n",
        expected: "hierarchy table",
    },
];

/// Write the fixture pair and return `(types_path, toml_path)`.
fn fixture(temp: &TempDir, toml_body: &str) -> Result<(String, String)> {
    let toml_path = temp.path().join("fraiseql.toml");
    fs::write(&toml_path, toml_body)?;

    // No types at all: the merged type set is irrelevant to every defect here,
    // which is exactly what makes them self-contained.
    let types_path = temp.path().join("schema.json");
    fs::write(
        &types_path,
        serde_json::json!({"types": [], "queries": [], "mutations": []}).to_string(),
    )?;

    Ok((
        types_path.to_string_lossy().into_owned(),
        toml_path.to_string_lossy().into_owned(),
    ))
}

/// The `--types` path must refuse what the TOML-only path refuses.
///
/// Before #1017 every one of these compiled cleanly through `merge_files` while
/// `merge_toml_only` rejected them, so the workflow an operator chose decided
/// whether their configuration was checked.
#[test]
fn every_self_contained_defect_is_refused_on_the_types_path() -> Result<()> {
    let mut compiled_anyway = Vec::new();

    for defect in DEFECTS {
        let temp = TempDir::new()?;
        let (types_path, toml_path) = fixture(&temp, defect.toml)?;

        match SchemaMerger::merge_files(&types_path, &toml_path) {
            Ok(_) => compiled_anyway.push(defect.what),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains(defect.expected),
                    "{}: refused, but not for the stated reason (wanted {:?}): {msg}",
                    defect.what,
                    defect.expected
                );
            },
        }
    }

    assert!(
        compiled_anyway.is_empty(),
        "the --types compile path accepted {} invalid configuration(s) that every other \
         workflow refuses: {compiled_anyway:?}",
        compiled_anyway.len()
    );

    Ok(())
}

/// The control: these same fixtures are refused by a path that does call
/// `validate()`. Without this, the test above could pass because the fixtures
/// are wrong rather than because the paths agree.
#[test]
fn the_same_defects_are_refused_on_the_toml_only_path() -> Result<()> {
    for defect in DEFECTS {
        let temp = TempDir::new()?;
        let (_types_path, toml_path) = fixture(&temp, defect.toml)?;

        let err = SchemaMerger::merge_toml_only(&toml_path)
            .expect_err(&format!("{}: expected the TOML-only path to refuse it", defect.what));

        let msg = err.to_string();
        assert!(
            msg.contains(defect.expected),
            "{}: refused, but not for the stated reason (wanted {:?}): {msg}",
            defect.what,
            defect.expected
        );
    }

    Ok(())
}

/// The exemption still holds: a query naming a type that only `types.json`
/// defines must keep compiling on the `--types` path.
///
/// This is the failure the split has to avoid — calling the whole of
/// `validate()` from `merge_files` would reject the workflow's normal case.
#[test]
fn a_query_may_still_reference_a_type_from_types_json() -> Result<()> {
    let temp = TempDir::new()?;

    let toml_path = temp.path().join("fraiseql.toml");
    fs::write(&toml_path, "[queries.user]\nreturn_type = \"User\"\nsql_source = \"v_user\"\n")?;

    let types_path = temp.path().join("schema.json");
    fs::write(
        &types_path,
        serde_json::json!({
            "types": [{
                "name": "User",
                "sql_source": "v_user",
                "fields": [{"name": "id", "type": "ID", "nullable": false}]
            }],
            "queries": [],
            "mutations": []
        })
        .to_string(),
    )?;

    SchemaMerger::merge_files(&types_path.to_string_lossy(), &toml_path.to_string_lossy())?;

    Ok(())
}
