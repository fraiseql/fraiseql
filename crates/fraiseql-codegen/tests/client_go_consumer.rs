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

const BIG_INT_FIXTURE: &str = include_str!("fixtures/big_int_schema.json");

/// A response holding `BigInt`s past 2^53, the first integer an `f64` cannot hold exactly,
/// and at the ends of the signed 64-bit range. The Go type declares the discriminant.
const BIG_INT_WIRE: &str = r#"{"__typename":"Ledger","id":"l1","balance":"9007199254740993","credit":"-9223372036854775808","history":["9223372036854775807","9007199254740993"],"postedAt":"2026-07-01T09:30:00"}"#;

/// Reads a response into the generated type and writes it out again. `encoding/json`
/// refuses a string at a numeric field, so a numeric type for a `BigInt` fails the decode.
const BIG_INT_MAIN_GO: &str = r#"package main

import (
	"encoding/json"
	"fmt"
	"os"

	client "example.com/consumer/fraiseqlclient"
)

// lookup writes a BigInt argument as the digits the server reads it from.
func lookup(c *client.Client) (*client.Ledger, error) {
	atLeast := "9007199254740993"
	return c.Ledger("l1", &atLeast)
}

// filterAndAdjust writes a BigInt filter operand, input field and argument as digits, and
// a LocalDateTime as its text: each is a string field the compiler holds them to.
func filterAndAdjust(c *client.Client) (client.Ledger, error) {
	gt := "9007199254740993"
	lt := "2026-07-01T09:30:00"
	where := client.LedgerWhereInput{
		Balance:  &client.BigIntFilter{Gt: &gt, In: []string{"-9223372036854775808"}},
		PostedAt: &client.LocalDateTimeFilter{Lt: &lt},
	}
	if _, err := c.Ledgers(&where, nil, nil); err != nil {
		return client.Ledger{}, err
	}
	change := client.AdjustLedgerInput{
		Id:    "l1",
		By:    "9007199254740993",
		Steps: []string{"9223372036854775807"},
		At:    &lt,
	}
	limit := "9223372036854775807"
	return c.AdjustLedger(change, &limit)
}

func main() {
	_, _ = lookup, filterAndAdjust
	var ledger client.Ledger
	if err := json.Unmarshal([]byte(os.Args[1]), &ledger); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	var balance string = ledger.Balance
	var credit *string = ledger.Credit
	var history []string = ledger.History
	var postedAt *string = ledger.PostedAt
	_, _, _, _ = balance, credit, history, postedAt
	encoded, err := json.Marshal(ledger)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Print(string(encoded))
}
"#;

/// The generated type of a `BigInt` carries a value past 2^53 exactly: decoded from the
/// wire and encoded again, every digit is the one the server sent.
#[test]
#[ignore = "requires the Go toolchain"]
fn a_big_int_past_two_to_the_53_round_trips_exactly() {
    // Loaded as a server loads it, so the derived `where` inputs are generated too.
    let schema = CompiledSchema::from_json(BIG_INT_FIXTURE, false).unwrap();
    let generated = go::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let package_dir = project.path().join("fraiseqlclient");
    std::fs::create_dir_all(&package_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(package_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("go.mod"), GO_MOD).unwrap();
    std::fs::write(project.path().join("main.go"), BIG_INT_MAIN_GO).unwrap();

    let output = run(project.path(), "go", &["run", ".", BIG_INT_WIRE]);
    assert!(
        output.status.success(),
        "the BigInt consumer did not build or run:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.stdout,
        output.stderr,
    );
    let served: serde_json::Value = serde_json::from_str(BIG_INT_WIRE).unwrap();
    let encoded: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(encoded, served, "the generated type did not carry every digit");
}

const FACT_TABLE_FIXTURE: &str = include_str!("fixtures/fact_table_schema.json");

/// A consumer of a fact table's two roots: an aggregate grouped and filtered through the
/// published inputs, read back through its typed row, and a window, whose rows are `JSON`.
const FACTS_GO: &str = r#"package usage

import (
	"fmt"

	client "example.com/consumer/fraiseqlclient"
)

func Facts(c *client.Client) error {
	eu := "EU"
	yes := true
	limit := 10
	rows, err := c.Sales_aggregate(
		&client.SalesAggregateWhere{Region_eq: &eu},
		&client.SalesAggregateGroupBy{Region: &yes, Occurred_on_month: &yes},
		[]client.SalesAggregateSelection{{Revenue_sum: &yes, Count: &yes}},
		nil, nil, &limit, nil,
	)
	if err != nil {
		return err
	}
	for _, row := range rows {
		var region *string = row.Region
		var revenue *string = row.Revenue_sum
		var count *int = row.Count
		fmt.Println(region, revenue, count, row.Occurred_on_month)
	}

	name := "revenue"
	windowed, err := c.Sales_window(
		[]client.WindowColumn{{Type: client.WindowColumnKindMeasure, Name: &name, Alias: "r"}},
		nil, nil, nil, nil, nil,
	)
	if err != nil {
		return err
	}
	var json []any = windowed
	fmt.Println(len(json))
	return nil
}
"#;

/// The fact-table roots are typed operations a consumer calls: Go compiles arguments built
/// from the published inputs and holds the aggregate row's fields to their published types.
#[test]
#[ignore = "requires the Go toolchain"]
fn fact_table_roots_compile_in_a_consumer_project() {
    let schema = CompiledSchema::from_json(FACT_TABLE_FIXTURE, false).unwrap();
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
    std::fs::write(usage_dir.join("facts.go"), FACTS_GO).unwrap();

    let formatted = run(project.path(), "gofmt", &["-l", "fraiseqlclient"]);
    assert!(
        formatted.status.success() && formatted.stdout.trim().is_empty(),
        "gofmt would reformat the generated client:\n{}\n{}",
        formatted.stdout,
        formatted.stderr,
    );
    for args in [["build", "./..."], ["vet", "./..."]] {
        let output = run(project.path(), "go", &args);
        assert!(
            output.status.success(),
            "`go {}` rejected the fact-table consumer project:\n--- stdout ---\n{}\n--- stderr ---\n{}",
            args.join(" "),
            output.stdout,
            output.stderr,
        );
    }
}
