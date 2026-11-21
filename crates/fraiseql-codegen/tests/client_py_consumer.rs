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

    assert_typed_dict_keys_match_the_wire(project.path());
}

/// The generated `TypedDict`s must declare the keys the server actually sends.
///
/// This is deliberately a **runtime** check, because neither gate above can see
/// the failure it guards. `CPython` mangles `__typename` in a class body to
/// `_User__typename` (#1033), but `ty` does not model mangling — it accepts
/// `u["__typename"]` and rejects `u["_User__typename"]` — and `compileall` only
/// parses. A `TypedDict` whose declared discriminant no consumer can satisfy
/// therefore passes both, and is visible only by importing the module and
/// reading `__required_keys__`.
fn assert_typed_dict_keys_match_the_wire(project: &std::path::Path) {
    const PROBE: &str = r#"from fraiseql_client.types import User

keys = set(User.__required_keys__) | set(User.__optional_keys__)
assert "__typename" in keys, f"discriminant is mangled away; declared keys: {sorted(keys)}"
assert not any(k.startswith("_User__") for k in keys), f"name-mangled key present: {sorted(keys)}"
print("ok")
"#;
    std::fs::write(project.join("probe_keys.py"), PROBE).unwrap();

    let probe = Command::new("python3")
        .args(["probe_keys.py"])
        .current_dir(project)
        .output()
        .expect("failed to run python3");

    assert!(
        probe.status.success(),
        "the generated TypedDict does not declare the key the wire carries:\n{}\n{}",
        String::from_utf8_lossy(&probe.stdout),
        String::from_utf8_lossy(&probe.stderr),
    );
}

const BIG_INT_FIXTURE: &str = include_str!("fixtures/big_int_schema.json");

/// A response holding `BigInt`s past 2^53, the first integer an `f64` cannot hold exactly,
/// and at the ends of the signed 64-bit range.
const BIG_INT_WIRE: &str = r#"{"id": "l1", "balance": "9007199254740993", "credit": "-9223372036854775808", "history": ["9223372036854775807", "9007199254740993"], "postedAt": "2026-07-01T09:30:00"}"#;

/// `ty` holds each `BigInt` and `LocalDateTime` position to `str`, the text the server sends
/// and reads, in every module that can name one: a response type, a filter input, a declared
/// input, a query's and a mutation's arguments. The run reads a response into the generated
/// type and writes it out again.
const BIG_INT_USAGE_PY: &str = r#"import json
import sys
from typing import assert_type, cast

from fraiseql_client import FraiseqlClient, adjustLedger, ledger, ledgers
from fraiseql_client.inputs import (
    AdjustLedgerInput,
    BigIntFilter,
    LedgerWhereInput,
    LocalDateTimeFilter,
)
from fraiseql_client.types import Ledger


def lookup(client: FraiseqlClient) -> Ledger | None:
    return ledger(client, id="l1", atLeast="9007199254740993")


def filter_and_adjust(client: FraiseqlClient) -> Ledger:
    where: LedgerWhereInput = {
        "balance": {"gt": "9007199254740993", "in": ["-9223372036854775808"]},
        "postedAt": {"lt": "2026-07-01T09:30:00"},
    }
    ledgers(client, where=where)
    change: AdjustLedgerInput = {
        "id": "l1",
        "by": "9007199254740993",
        "steps": ["9223372036854775807"],
        "at": "2026-07-01T09:30:00",
    }
    return adjustLedger(client, input=change, limit="9223372036854775807")


def input_types(
    balance: BigIntFilter, posted: LocalDateTimeFilter, change: AdjustLedgerInput
) -> None:
    assert_type(balance.get("eq"), str | None)
    assert_type(balance.get("in"), list[str] | None)
    assert_type(posted.get("lt"), str | None)
    assert_type(change["by"], str)
    assert_type(change.get("at"), str | None)


def round_trip(wire: str) -> str:
    decoded = cast(Ledger, json.loads(wire))
    assert_type(decoded["balance"], str)
    assert_type(decoded["credit"], str | None)
    assert_type(decoded["history"], list[str])
    assert_type(decoded["postedAt"], str | None)
    return json.dumps(decoded)


if __name__ == "__main__":
    print(round_trip(sys.argv[1]), end="")
"#;

/// The generated type of a `BigInt` carries a value past 2^53 exactly: `ty` holds it to a
/// `str`, and a response read into it and written out again keeps every digit.
#[test]
#[ignore = "requires network (uvx ty) and uv"]
fn a_big_int_past_two_to_the_53_round_trips_exactly() {
    // Loaded as a server loads it, so the derived `where` inputs are generated too.
    let schema = CompiledSchema::from_json(BIG_INT_FIXTURE, false).unwrap();
    let generated = python::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let package_dir = project.path().join("fraiseql_client");
    std::fs::create_dir_all(&package_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(package_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("usage.py"), BIG_INT_USAGE_PY).unwrap();

    let checked = Command::new("uvx")
        .args(["ty", "check", "--python-version", "3.12", "."])
        .current_dir(project.path())
        .output()
        .expect("failed to run uvx ty — is uv installed?");
    assert!(
        checked.status.success(),
        "ty rejected the BigInt consumer:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr),
    );

    let run = Command::new("python3")
        .args(["usage.py", BIG_INT_WIRE])
        .current_dir(project.path())
        .output()
        .expect("failed to run python3");
    assert!(
        run.status.success(),
        "the BigInt consumer did not run:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    let served: serde_json::Value = serde_json::from_str(BIG_INT_WIRE).unwrap();
    let encoded: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(encoded, served, "the generated type did not carry every digit");
}
