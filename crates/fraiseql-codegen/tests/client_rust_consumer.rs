//! End-to-end consumer integration for the Rust client: generate the client
//! into a fresh crate, drop a usage module that imports it and exercises every
//! operation, and compile the whole thing with warnings denied. This proves the
//! generated client is not just internally valid but usable by a real consumer.
//!
//! Gated `#[ignore]` because it shells out to `cargo` (and so needs the crates
//! registry). Run with:
//!
//! ```sh
//! cargo test -p fraiseql-codegen --test client_rust_consumer -- --ignored
//! ```
//!
//! CI runs it in `sdk-conformance.yml` (`generated-clients` job) alongside the
//! `TypeScript`, Python and Go twins.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::process::Command;

use fraiseql_codegen::client::rust;
use fraiseql_core::schema::CompiledSchema;

const FIXTURE: &str = include_str!("fixtures/tutorial.schema.compiled.json");

/// `[workspace]` is load-bearing: without it `cargo` walks up from the temp
/// directory looking for a workspace root and can adopt an unrelated one.
const CARGO_TOML: &str = r#"[package]
name = "fraiseql-generated-consumer"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[workspace]
"#;

/// `deny(warnings)` scopes the gate to the consumer crate and the generated
/// module inside it — `RUSTFLAGS` would apply it to `serde` too, which is not
/// what is being asserted here.
const LIB_RS: &str = "#![deny(warnings)]\n\npub mod generated;\npub mod usage;\n";

/// A realistic consumer: constructs a client, calls each operation, and matches
/// a mutation result on its union discriminant. If any generated type were
/// wrong, this would fail to compile.
const USAGE_RS: &str = r#"use crate::generated::{
    Connection, CreateUserInput, CreateUserResult, Error, FraiseqlClient, Post, RELATIONSHIPS,
    Transport, UpdateUserInput, User, UserFilter, UserRole, mutations, queries,
};

/// Build a client over a transport that never actually leaves the process — the
/// point here is that every call below type-checks.
fn client() -> FraiseqlClient<impl Transport> {
    FraiseqlClient::new(|_body: &str| Err(Error::transport("not wired")))
}

pub fn demo() -> Result<(), Error> {
    let client = client();

    let found: Option<User> = queries::get_user(&client, "u1".to_string())?;
    if let Some(user) = found {
        let role: UserRole = user.role;
        println!(
            "{} {} {:?} {role:?} {}",
            user.id, user.email, user.display_name, user.created_at
        );
    }

    let listing: Vec<User> = queries::users(
        &client,
        Some(UserFilter {
            email: None,
            role: Some(UserRole::Editor),
            tags: Some(vec!["a".to_string()]),
        }),
        None,
        Some(10),
        None,
    )?;
    for user in &listing {
        println!("{}", user.email);
    }

    let page: Connection<Post> = queries::posts_connection(&client, Some(10), None)?;
    for edge in &page.edges {
        let post: &Post = &edge.node;
        println!(
            "{} {} {} {:?} {:?}",
            edge.cursor, post.title, post.view_count, post.score, post.metadata
        );
    }
    println!("{} {:?} {:?}", page.page_info.has_next_page, page.page_info.end_cursor, page.total_count);

    let created = mutations::create_user(
        &client,
        CreateUserInput {
            email: "a@b.c".to_string(),
            display_name: None,
            role: UserRole::Editor,
        },
    )?;
    // Narrow the union on its `__typename` discriminant — the whole point of
    // rendering it as an internally tagged enum.
    match created {
        CreateUserResult::User(user) => println!("{} {}", user.id, user.email),
        CreateUserResult::EmailTakenError(error) => {
            println!("{} {}", error.message, error.attempted_email);
            assert!(mutations::is_error_typename("EmailTakenError"));
        },
    }

    let updated = mutations::update_user(
        &client,
        "u1".to_string(),
        UpdateUserInput {
            display_name: Some("new name".to_string()),
            role: None,
        },
    )?;
    println!("{}", updated.id);

    let deleted = mutations::delete_user(&client, "u1".to_string())?;
    println!("{}", deleted.id);

    for entry in RELATIONSHIPS {
        for meta in entry.relationships {
            println!(
                "{} {} {} {} {}",
                entry.type_name, meta.name, meta.target_type, meta.cardinality, meta.foreign_key
            );
        }
    }
    Ok(())
}
"#;

/// Generate the client, then type-check a consumer crate against it.
#[test]
#[ignore = "requires cargo and the crates.io registry"]
fn generated_client_compiles_in_a_consumer_project() {
    let schema: CompiledSchema = serde_json::from_str(FIXTURE).unwrap();
    let generated = rust::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let generated_dir = project.path().join("src").join("generated");
    std::fs::create_dir_all(&generated_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(generated_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("Cargo.toml"), CARGO_TOML).unwrap();
    std::fs::write(project.path().join("src").join("lib.rs"), LIB_RS).unwrap();
    std::fs::write(project.path().join("src").join("usage.rs"), USAGE_RS).unwrap();

    let output = Command::new(env!("CARGO"))
        .args(["check", "--quiet"])
        .current_dir(project.path())
        // The parent `cargo test` sets these for *this* workspace; leaking them
        // into the nested build would point it at the wrong target directory.
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUSTFLAGS")
        .output()
        .expect("failed to run cargo");

    assert!(
        output.status.success(),
        "cargo rejected the generated consumer crate:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

const BIG_INT_FIXTURE: &str = include_str!("fixtures/big_int_schema.json");

/// A response holding `BigInt`s past 2^53, the first integer an `f64` cannot hold exactly,
/// and at the ends of the signed 64-bit range.
const BIG_INT_WIRE: &str = r#"{"id":"l1","balance":"9007199254740993","credit":"-9223372036854775808","history":["9223372036854775807","9007199254740993"]}"#;

const BIG_INT_LIB_RS: &str = r#"#![deny(warnings)]

pub mod generated;

use generated::{Error, FraiseqlClient, Ledger, Transport, queries};

/// A `BigInt` argument is written as the digits the server reads it from.
pub fn lookup(client: &FraiseqlClient<impl Transport>) -> Result<Option<Ledger>, Error> {
    queries::ledger(client, "l1".to_string(), Some("9007199254740993".to_string()))
}

/// Decode a response into the generated type and encode it again.
pub fn round_trip(wire: &str) -> String {
    let ledger: Ledger = serde_json::from_str(wire).expect("the generated type decodes the wire");
    let balance: &String = &ledger.balance;
    let credit: &Option<String> = &ledger.credit;
    let history: &Vec<String> = &ledger.history;
    assert_eq!((balance.len(), credit.is_some(), history.len()), (16, true, 2));
    serde_json::to_string(&ledger).expect("the generated type encodes")
}
"#;

/// The generated type of a `BigInt` carries a value past 2^53 exactly: decoded from the
/// wire and encoded again, every digit is the one the server sent.
#[test]
#[ignore = "requires cargo and the crates.io registry"]
fn a_big_int_past_two_to_the_53_round_trips_exactly() {
    let schema: CompiledSchema = serde_json::from_str(BIG_INT_FIXTURE).unwrap();
    let generated = rust::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let generated_dir = project.path().join("src").join("generated");
    std::fs::create_dir_all(&generated_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(generated_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("Cargo.toml"), CARGO_TOML).unwrap();
    std::fs::write(project.path().join("src").join("lib.rs"), BIG_INT_LIB_RS).unwrap();
    std::fs::write(
        project.path().join("src").join("main.rs"),
        format!(
            "fn main() {{\n    print!(\"{{}}\", fraiseql_generated_consumer::round_trip({BIG_INT_WIRE:?}));\n}}\n"
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(project.path())
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("RUSTFLAGS")
        .output()
        .expect("failed to run cargo");

    assert!(
        output.status.success(),
        "the consumer did not build or run:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let served: serde_json::Value = serde_json::from_str(BIG_INT_WIRE).unwrap();
    let encoded: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(encoded, served, "the generated type did not carry every digit");
}
