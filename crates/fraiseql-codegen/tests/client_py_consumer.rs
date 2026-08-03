//! End-to-end consumer integration for the Python client: generate the client
//! into a fresh package, drop a usage file that imports it and exercises every
//! operation, and type-check the whole thing with `ty`. This proves the
//! generated client is not just internally valid but usable by a real consumer.
//!
//! Gated `#[ignore]` because it shells out to `uvx ty` (network + uv). Run with:
//!
//! ```sh
//! cargo test -p fraiseql-codegen --test client_py_consumer -- --ignored
//! ```
//!
//! CI runs it in `sdk-conformance.yml` (`generated-clients` job) alongside the
//! `TypeScript` twin.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::process::Command;

use fraiseql_codegen::client::python;
use fraiseql_core::schema::CompiledSchema;

const FIXTURE: &str = include_str!("fixtures/tutorial.schema.compiled.json");

/// A realistic consumer: constructs a client, calls each operation, and narrows
/// a mutation result with `is_error_result`. If any generated type were wrong,
/// this would fail `ty check`.
const USAGE_PY: &str = r#"from fraiseql_client import (
    FraiseqlClient,
    UserRole,
    createUser,
    getUser,
    is_error_result,
    postsConnection,
    users,
)


def demo() -> None:
    client = FraiseqlClient("https://api.example.com/graphql")

    user = getUser(client, id="u1")
    if user is not None:
        role: UserRole = user["role"]
        print(user["id"], user["email"], user["displayName"], role, user["createdAt"])

    listing = users(client, filter={"role": "EDITOR"})
    print([u["email"] for u in listing])

    page = postsConnection(client, first=10)
    for edge in page["edges"]:
        print(edge["cursor"], edge["node"]["title"], edge["node"]["viewCount"])
    print(page["pageInfo"]["hasNextPage"])

    result = createUser(client, input={"email": "a@b.c", "role": "EDITOR"})
    print(is_error_result(result))
    # Narrow the result union on its `__typename` discriminant — the pattern
    # type checkers narrow TypedDict unions by.
    if result["__typename"] == "EmailTakenError":
        print(result["status"])
    else:
        print(result["id"], result["email"])
"#;

#[test]
#[ignore = "requires network (uvx ty) and uv"]
fn generated_client_type_checks_in_a_consumer_project() {
    let schema: CompiledSchema = serde_json::from_str(FIXTURE).unwrap();
    let generated = python::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let package_dir = project.path().join("fraiseql_client");
    std::fs::create_dir_all(&package_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(package_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("usage.py"), USAGE_PY).unwrap();

    // Syntax first (fast, no network), then full type-check via ty.
    let compile = Command::new("python3")
        .args(["-m", "compileall", "-q", "."])
        .current_dir(project.path())
        .output()
        .expect("failed to run python3 — is Python installed?");
    assert!(
        compile.status.success(),
        "python rejected the generated client syntax:\n{}\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    let output = Command::new("uvx")
        .args(["ty", "check", "--python-version", "3.12", "."])
        .current_dir(project.path())
        .output()
        .expect("failed to run uvx ty — is uv installed?");
    assert!(
        output.status.success(),
        "ty rejected the generated consumer project:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
