//! End-to-end consumer integration for the Go client: generate the client into
//! a fresh module, drop a usage file that imports it and exercises every
//! operation, and compile the whole thing. This proves the generated client is
//! not just internally valid but usable by a real consumer.
//!
//! Gated `#[ignore]` because it shells out to the Go toolchain. Run with:
//!
//! ```sh
//! cargo test -p fraiseql-codegen --test client_go_consumer -- --ignored
//! ```
//!
//! CI runs it in `sdk-conformance.yml` (`generated-clients` job) alongside the
//! `TypeScript`, Python and Rust twins.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{path::Path, process::Command};

use fraiseql_codegen::client::go;
use fraiseql_core::schema::CompiledSchema;

const FIXTURE: &str = include_str!("fixtures/tutorial.schema.compiled.json");

const GO_MOD: &str = "module example.com/consumer\n\ngo 1.21\n";

/// A realistic consumer: constructs a client, calls each operation, and narrows
/// a mutation result on its union discriminant. If any generated type were
/// wrong, this would fail to compile.
const USAGE_GO: &str = r#"package usage

import (
	"fmt"

	client "example.com/consumer/fraiseqlclient"
)

func Demo() error {
	c := client.NewClient("https://api.example.com/graphql")
	c.Headers = func() map[string]string {
		return map[string]string{"authorization": "Bearer token"}
	}

	user, err := c.GetUser("u1")
	if err != nil {
		return err
	}
	if user != nil {
		var role client.UserRole = user.Role
		fmt.Println(user.Id, user.Email, user.DisplayName, role, user.CreatedAt)
	}

	limit := 10
	listing, err := c.Users(&client.UserFilter{Role: roleOf(client.UserRoleEditor)}, nil, &limit, nil)
	if err != nil {
		return err
	}
	for _, u := range listing {
		fmt.Println(u.Email)
	}

	page, err := c.PostsConnection(&limit, nil)
	if err != nil {
		return err
	}
	for _, edge := range page.Edges {
		fmt.Println(edge.Cursor, edge.Node.Title, edge.Node.ViewCount, edge.Node.Score, edge.Node.Metadata)
	}
	fmt.Println(page.PageInfo.HasNextPage, page.PageInfo.EndCursor, page.TotalCount)

	created, err := c.CreateUser(client.CreateUserInput{
		Email: "a@b.c",
		Role:  client.UserRoleEditor,
	})
	if err != nil {
		return err
	}
	fmt.Println(client.IsErrorResult(created.Typename))
	// Narrow the union on its __typename discriminant: exactly one member is set.
	if created.EmailTakenError != nil {
		fmt.Println(created.EmailTakenError.Message, created.EmailTakenError.AttemptedEmail)
	} else if created.User != nil {
		fmt.Println(created.User.Id, created.User.Email)
	}

	name := "new name"
	updated, err := c.UpdateUser("u1", client.UpdateUserInput{DisplayName: &name})
	if err != nil {
		return err
	}
	fmt.Println(updated.Id)

	deleted, err := c.DeleteUser("u1")
	if err != nil {
		return err
	}
	fmt.Println(deleted.Id)

	meta := client.Relationships["User"]["posts"]
	fmt.Println(meta.TargetType, meta.Cardinality, meta.ForeignKey, meta.ReferencedKey)
	return nil
}

func roleOf(role client.UserRole) *client.UserRole { return &role }
"#;

/// Generate the client, then compile a consumer module against it.
#[test]
#[ignore = "requires the Go toolchain"]
fn generated_client_compiles_in_a_consumer_project() {
    let schema: CompiledSchema = serde_json::from_str(FIXTURE).unwrap();
    let generated = go::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let package_dir = project.path().join("fraiseqlclient");
    std::fs::create_dir_all(&package_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(package_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("go.mod"), GO_MOD).unwrap();
    let usage_dir = project.path().join("usage");
    std::fs::create_dir_all(&usage_dir).unwrap();
    std::fs::write(usage_dir.join("usage.go"), USAGE_GO).unwrap();

    // The generator does its own column alignment rather than shelling out to
    // gofmt, so the emitted source has to be checked against the real thing:
    // `gofmt -l` names every file it would reformat.
    let formatted = run(project.path(), "gofmt", &["-l", "fraiseqlclient"]);
    if !formatted.status.success() || !formatted.stdout.trim().is_empty() {
        let diff = run(project.path(), "gofmt", &["-d", "fraiseqlclient"]);
        panic!(
            "gofmt would reformat the generated client:\n{}\n{}\n--- diff ---\n{}",
            formatted.stdout, formatted.stderr, diff.stdout,
        );
    }

    for args in [["build", "./..."], ["vet", "./..."]] {
        let output = run(project.path(), "go", &args);
        assert!(
            output.status.success(),
            "`go {}` rejected the generated consumer project:\n--- stdout ---\n{}\n--- stderr ---\n{}",
            args.join(" "),
            output.stdout,
            output.stderr,
        );
    }
}

struct Output {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run(dir: &Path, program: &str, args: &[&str]) -> Output {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        // A sandboxed HOME keeps the module cache out of the developer's, and
        // GOFLAGS=-mod=mod stops Go from demanding a vendor directory.
        .env("GOFLAGS", "-mod=mod")
        .output()
        .unwrap_or_else(|e| panic!("failed to run {program} — is the Go toolchain installed? {e}"));
    Output {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}
