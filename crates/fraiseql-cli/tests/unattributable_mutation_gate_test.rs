#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Compile-time refusals for declarations the runtime cannot honour (#910, #983).
//!
//! **#910** — a mutation that resolves to no view must not compile alongside a cacheable view.
//!
//! The issue's shape, verbatim: `@fraiseql.mutation(sql_source="fn_rebuild_pricing")`
//! returning `RebuildResult` — a payload with no backing view and no `entity` field —
//! while `v_price` is annotated `cache_ttl_seconds = 0` ("mutation-invalidated only").
//! The function rewrites `tb_price`; the engine cannot know that, so it invalidates
//! nothing and cached price lists never refresh for the process lifetime.
//!
//! The refusal is at **compile**, not boot and not a warning: the compiler is the one
//! place that knows every mutation and every TTL at once, so it can make the shape
//! impossible rather than loud.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** unsafe to split. `compile_to_schema` resolves its input against the
//! *process* working directory, so three `#[tokio::test]`s each chdir'ing into their own
//! temp dir race: the first draft of this file passed its negative case because a sibling
//! had already chdir'd to a schema that legitimately compiles — and adding #983's case as a
//! second `#[tokio::test]` reproduced it immediately. Everything goes in the one test.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use tempfile::TempDir;

/// `v_price` is cacheable with the mutation-invalidated-only TTL; `rebuildPricing`
/// returns an unbacked payload and declares nothing.
const UNATTRIBUTABLE: &str = r#"
{
  "types": [
    {
      "name": "Price",
      "fields": [{"name": "id", "type": "Int", "nullable": false}],
      "sql_source": "v_price",
      "is_input": false
    },
    {
      "name": "RebuildResult",
      "fields": [{"name": "rows", "type": "Int", "nullable": false}],
      "sql_source": "",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "prices",
      "return_type": "Price",
      "returns_list": true,
      "sql_source": "v_price",
      "cache_ttl_seconds": 0,
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [
    {
      "name": "rebuildPricing",
      "return_type": "RebuildResult",
      "sql_source": "fn_rebuild_pricing",
      "arguments": []
    }
  ],
  "subscriptions": [],
  "version": "2.0.0"
}
"#;

/// Compile `schema_json` from a fresh temp dir, restoring the working directory.
async fn compile(schema_json: &str) -> anyhow::Result<()> {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), schema_json).expect("write schema.json");

    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");

    result.map(|_| ())
}

#[tokio::test]
async fn the_unattributable_mutation_gate() {
    // 1. The issue's shape refuses to compile, and the message is actionable.
    let err = compile(UNATTRIBUTABLE)
        .await
        .expect_err("a mutation that resolves to no view must not compile (#910)");
    let msg = format!("{err:#}");

    assert!(
        msg.contains("rebuildPricing"),
        "the refusal must name the offending mutation; got: {msg}"
    );
    assert!(
        msg.contains("invalidates_views"),
        "the refusal must name the annotation that fixes it; got: {msg}"
    );
    assert!(
        msg.contains("v_price"),
        "the refusal must name the cacheable view at risk; got: {msg}"
    );

    // 2. Declaring what the mutation writes is the fix the message asks for, and it works.
    let fixed = UNATTRIBUTABLE.replace(
        r#""sql_source": "fn_rebuild_pricing","#,
        r#""sql_source": "fn_rebuild_pricing", "invalidates_views": ["v_price"],"#,
    );
    assert_ne!(fixed, UNATTRIBUTABLE, "the fixture edit must have applied");
    compile(&fixed)
        .await
        .expect("declaring what the mutation writes resolves the refusal");

    // 3. Guard against over-refusal: with no `cache_ttl_seconds` anywhere the adapter's opt-in mode
    //    caches nothing, so there is no entry to strand and demanding the annotation would be
    //    noise.
    let uncached = UNATTRIBUTABLE.replace(r#""cache_ttl_seconds": 0,"#, "");
    assert_ne!(uncached, UNATTRIBUTABLE, "the fixture edit must have applied");
    compile(&uncached).await.expect("a schema with no cacheable view is unaffected");

    // 4. #983's gate, in the same test for the same chdir reason. An authorization rule no enforcer
    //    reads must not compile — a rule that is silently not applied is the worst shape a security
    //    declaration can take.
    let err = compile(UNENFORCED_AUTHZ)
        .await
        .expect_err("a rule that enforces nothing must not compile (#983)");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("no enforcer"),
        "the refusal must say the declaration enforces nothing; got: {msg}"
    );
    assert!(
        msg.contains("security.rules"),
        "the refusal must name the declaration; got: {msg}"
    );
    assert!(
        msg.contains("requires_scope"),
        "the refusal must name the mechanism that does work; got: {msg}"
    );
}

/// #983 — `security.rules`, `security.field_auth` and `security.default_policy` are
/// carried by the compiled seam and read by nothing. An operator who writes an
/// authorization rule must not get a successful compile and zero enforcement.
const UNENFORCED_AUTHZ: &str = r#"
{
  "types": [
    {
      "name": "Price",
      "fields": [{"name": "id", "type": "Int", "nullable": false}],
      "sql_source": "v_price",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "prices",
      "return_type": "Price",
      "returns_list": true,
      "sql_source": "v_price",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [],
  "subscriptions": [],
  "security": {
    "rules": [{"name": "deny_all", "rule": "false"}]
  },
  "version": "2.0.0"
}
"#;
