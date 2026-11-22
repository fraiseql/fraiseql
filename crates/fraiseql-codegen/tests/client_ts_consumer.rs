//! End-to-end consumer integration: generate the client into a fresh project,
//! drop a usage file that imports it and exercises every operation, and type-check
//! the whole thing with `tsc --strict`. This proves the generated client is not
//! just internally valid but usable by a real consumer.
//!
//! Gated `#[ignore]` because it shells out to `npx`/`tsc` (network + Node).
//! Run with:
//!
//! ```sh
//! cargo test -p fraiseql-codegen --test client_ts_consumer -- --ignored
//! ```
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::process::Command;

use fraiseql_codegen::client::typescript;
use fraiseql_core::schema::CompiledSchema;

const FIXTURE: &str = include_str!("fixtures/tutorial.schema.compiled.json");

const CONSUMER_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noEmit": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM"],
    "skipLibCheck": true
  },
  "include": ["generated/**/*.ts", "usage.ts"]
}
"#;

/// A realistic consumer: constructs a client, calls each operation, and narrows a
/// mutation result union with `isErrorResult`. If any generated type were wrong,
/// this would fail `tsc`.
const USAGE_TS: &str = r#"import {
  FraiseqlClient,
  getUser,
  users,
  postsConnection,
  createUser,
  isErrorResult,
  type UserRole,
} from "./generated";

export async function demo(): Promise<void> {
  const client = new FraiseqlClient({ endpoint: "https://api.example.com/graphql" });

  const user = await getUser(client, { id: "u1" });
  if (user) {
    const role: UserRole = user.role;
    console.log(user.id, user.email, user.displayName, role, user.createdAt);
  }

  const list = await users(client, { filter: { role: "ADMIN" } });
  console.log(list.map((u) => u.email));

  const page = await postsConnection(client, { first: 10 });
  for (const edge of page.edges) {
    console.log(edge.cursor, edge.node.title, edge.node.viewCount);
  }
  console.log(page.pageInfo.hasNextPage);

  const result = await createUser(client, { input: { email: "a@b.c", role: "EDITOR" } });
  if (isErrorResult(result)) {
    console.error(result.status, result.attemptedEmail, result.existingUserId);
  } else {
    console.log(result.id, result.email);
  }
}
"#;

#[test]
#[ignore = "requires network (npx typescript) and Node"]
fn generated_client_type_checks_in_a_consumer_project() {
    let schema: CompiledSchema = serde_json::from_str(FIXTURE).unwrap();
    let generated = typescript::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let generated_dir = project.path().join("generated");
    std::fs::create_dir_all(&generated_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(generated_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("tsconfig.json"), CONSUMER_TSCONFIG).unwrap();
    std::fs::write(project.path().join("usage.ts"), USAGE_TS).unwrap();

    let output = Command::new("npx")
        .args(["-y", "-p", "typescript@5", "tsc", "-p", "tsconfig.json"])
        .current_dir(project.path())
        .output()
        .expect("failed to run npx tsc — is Node installed?");

    assert!(
        output.status.success(),
        "tsc rejected the generated consumer project:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

const BIG_INT_FIXTURE: &str = include_str!("fixtures/big_int_schema.json");

/// A response holding `BigInt`s past 2^53, the first integer an `f64` cannot hold exactly,
/// and at the ends of the signed 64-bit range.
const BIG_INT_WIRE: &str = r#"{"id":"l1","balance":"9007199254740993","credit":"-9223372036854775808","history":["9223372036854775807","9007199254740993"],"postedAt":"2026-07-01T09:30:00"}"#;

const BIG_INT_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "outDir": "out",
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM"],
    "types": [],
    "skipLibCheck": true
  },
  "include": ["generated/**/*.ts", "lookup.ts", "round_trip.ts"]
}
"#;

/// A `BigInt` argument, filter operand and input field is written as the digits the server
/// reads it from, and a `LocalDateTime` as its text.
const BIG_INT_LOOKUP_TS: &str = r#"import {
  FraiseqlClient,
  adjustLedger,
  ledger,
  ledgers,
  type AdjustLedgerInput,
  type Ledger,
  type LedgerWhereInput,
} from "./generated";

export async function lookup(client: FraiseqlClient): Promise<Ledger | null> {
  return ledger(client, { id: "l1", atLeast: "9007199254740993" });
}

export async function filterAndAdjust(client: FraiseqlClient): Promise<Ledger> {
  const where: LedgerWhereInput = {
    balance: { gt: "9007199254740993", in: ["-9223372036854775808"] },
    postedAt: { lt: "2026-07-01T09:30:00" },
  };
  await ledgers(client, { where });
  const change: AdjustLedgerInput = {
    id: "l1",
    by: "9007199254740993",
    steps: ["9223372036854775807"],
    at: "2026-07-01T09:30:00",
  };
  return adjustLedger(client, { input: change, limit: "9223372036854775807" });
}
"#;

/// `tsc` holds each `BigInt` position to exactly `string`: a `TypeScript` type is erased at
/// run time, so a `number` would let a consumer do arithmetic the value cannot survive, and
/// a `bigint` would describe a value `JSON.parse` never produces. `tsc` compiles it, and
/// plain `node` runs the output: the import is type-only, so nothing is resolved, and
/// no type stripping is asked of a Node that may not have it.
const BIG_INT_ROUND_TRIP_TS: &str = r#"import type {
  AdjustLedgerInput,
  BigIntFilter,
  Ledger,
  LocalDateTimeFilter,
} from "./generated";

type Exactly<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
const balance: Exactly<Ledger["balance"], string> = true;
const credit: Exactly<Ledger["credit"], string | null> = true;
const history: Exactly<Ledger["history"], string[]> = true;
const posted: Exactly<Ledger["postedAt"], string | null> = true;
const operand: Exactly<BigIntFilter["eq"], string | null | undefined> = true;
const items: Exactly<BigIntFilter["in"], string[] | null | undefined> = true;
const bound: Exactly<LocalDateTimeFilter["lt"], string | null | undefined> = true;
const by: Exactly<AdjustLedgerInput["by"], string> = true;
const inputs = posted && operand && items && bound && by;

const decoded: Ledger = JSON.parse(process.argv[2] ?? "");
process.stdout.write(JSON.stringify(decoded) + (balance && credit && history && inputs ? "" : "!"));

declare const process: { argv: string[]; stdout: { write(text: string): void } };
"#;

/// The generated type of a `BigInt` carries a value past 2^53 exactly: `tsc` holds it to a
/// `string`, and a response read into it and written out again keeps every digit.
#[test]
#[ignore = "requires network (npx typescript) and Node"]
fn a_big_int_past_two_to_the_53_round_trips_exactly() {
    // Loaded as a server loads it, so the derived `where` inputs are generated too.
    let schema = CompiledSchema::from_json(BIG_INT_FIXTURE, false).unwrap();
    let generated = typescript::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let generated_dir = project.path().join("generated");
    std::fs::create_dir_all(&generated_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(generated_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("tsconfig.json"), BIG_INT_TSCONFIG).unwrap();
    std::fs::write(project.path().join("lookup.ts"), BIG_INT_LOOKUP_TS).unwrap();
    std::fs::write(project.path().join("round_trip.ts"), BIG_INT_ROUND_TRIP_TS).unwrap();
    // `tsc` emits ES modules; this tells `node` to read them as such.
    std::fs::write(project.path().join("package.json"), r#"{ "type": "module" }"#).unwrap();

    let checked = Command::new("npx")
        .args(["-y", "-p", "typescript@5", "tsc", "-p", "tsconfig.json"])
        .current_dir(project.path())
        .output()
        .expect("failed to run npx tsc — is Node installed?");
    assert!(
        checked.status.success(),
        "tsc rejected the BigInt consumer:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr),
    );

    let run = Command::new("node")
        .args(["out/round_trip.js", BIG_INT_WIRE])
        .current_dir(project.path())
        .output()
        .expect("failed to run node");
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

const FACT_TABLE_FIXTURE: &str = include_str!("fixtures/fact_table_schema.json");

const FACT_TABLE_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noEmit": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM"],
    "skipLibCheck": true
  },
  "include": ["generated/**/*.ts", "facts.ts"]
}
"#;

/// A consumer of a fact table's two roots: an aggregate grouped and filtered through the
/// published inputs, read back through its typed row, and a window, whose rows are `JSON`.
const FACTS_TS: &str = r#"import {
  FraiseqlClient,
  sales_aggregate,
  sales_window,
  type SalesAggregateRow,
} from "./generated";

type Exactly<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
const region: Exactly<SalesAggregateRow["region"], string | null> = true;
const revenue: Exactly<SalesAggregateRow["revenue_sum"], string | null> = true;
const count: Exactly<SalesAggregateRow["count"], number | null> = true;

export async function demo(client: FraiseqlClient): Promise<void> {
  const rows = await sales_aggregate(client, {
    where: { region_eq: "EU" },
    groupBy: { region: true, occurred_on_month: true },
    aggregates: [{ revenue_sum: true, count: true }],
    limit: 10,
  });
  for (const row of rows) {
    console.log(row.region, row.occurred_on_month, row.revenue_sum, row.count);
  }
  const everything: SalesAggregateRow[] = await sales_aggregate(client);

  const windowed: unknown[] = await sales_window(client, {
    select: [{ type: "measure", name: "revenue", alias: "revenue" }],
  });
  console.log(everything.length, windowed.length, region && revenue && count);
}
"#;

/// The fact-table roots are typed operations a consumer calls: `tsc --strict` accepts
/// arguments built from the published inputs and holds the aggregate row's fields to their
/// published types.
#[test]
#[ignore = "requires network (npx typescript) and Node"]
fn fact_table_roots_type_check_in_a_consumer_project() {
    let schema = CompiledSchema::from_json(FACT_TABLE_FIXTURE, false).unwrap();
    let generated = typescript::generate(&schema).unwrap();

    let project = tempfile::tempdir().unwrap();
    let generated_dir = project.path().join("generated");
    std::fs::create_dir_all(&generated_dir).unwrap();
    for (rel, content) in &generated {
        std::fs::write(generated_dir.join(rel), content).unwrap();
    }
    std::fs::write(project.path().join("tsconfig.json"), FACT_TABLE_TSCONFIG).unwrap();
    std::fs::write(project.path().join("facts.ts"), FACTS_TS).unwrap();

    let output = Command::new("npx")
        .args(["-y", "-p", "typescript@5", "tsc", "-p", "tsconfig.json"])
        .current_dir(project.path())
        .output()
        .expect("failed to run npx tsc — is Node installed?");

    assert!(
        output.status.success(),
        "tsc rejected the fact-table consumer project:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
