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
    "noEmit": true,
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
/// a `bigint` would describe a value `JSON.parse` never produces. Node then runs the file
/// (its types stripped; the import is type-only, so nothing is resolved).
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
        .args(["--experimental-strip-types", "round_trip.ts", BIG_INT_WIRE])
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
