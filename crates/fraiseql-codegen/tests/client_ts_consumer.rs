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
