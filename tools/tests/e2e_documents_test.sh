#!/usr/bin/env bash
# e2e_documents_test.sh — the red capability of tools/check-e2e-documents.py.
#
# The gate's static half proved on synthetic trees: the offending shape must go RED
# and the legitimate shape must stay GREEN. (Its running half counts `... ok` lines
# rather than trusting cargo's exit code, because a `--exact` filter that matches
# nothing exits 0; that half is exercised by the real tree in `make preflight`.)
#
# Case 4 is the one that matters most: a guard that consults DATABASE_URL skips in
# exactly the runs it exists for, and would pass every other check here.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-e2e-documents.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  E2E_DOCUMENTS_ROOT="$1" python3 "$gate" --static-only > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

expect() { # expect <rc> <substring> <label>
  if [ "$(cat "${tmp}/rc")" = "$1" ] && grep -F "$2" "${tmp}/out" >/dev/null; then
    pass "$3"
  else
    fail "$3"
  fi
}

# ── 1. A suite that compiles a document and has no guard fails ───────────────────────
# The shape of 2b843cd27: the document is compiled after `try_database_url()?`.
root="${tmp}/unguarded"
mk "${root}/crates/fraiseql-server/tests/budget_e2e_pg.rs" '
async fn rig() -> Option<()> {
    let url = try_database_url()?;
    let (compiled, _) = compile_to_schema(opts).await.unwrap();
    Some(())
}'
run_gate "$root"
expect 1 "budget_e2e_pg.rs: loads a document but defines no" "a compiling suite without a guard fails"

# ── 2. The same suite with a guard passes ────────────────────────────────────────────
root="${tmp}/guarded"
mk "${root}/crates/fraiseql-server/tests/budget_e2e_pg.rs" '
async fn rig() -> Option<()> {
    let url = try_database_url()?;
    let (compiled, _) = compile_to_schema(opts).await.unwrap();
    Some(())
}
#[tokio::test]
async fn the_document_loads_without_a_database() {
    compile_to_schema(opts).await.unwrap();
}'
run_gate "$root"
expect 0 "all 1 database-gated document-loading test binaries carry" "a compiling suite with a guard passes"

# ── 3. A hand-authored JSON document counts as loading one ───────────────────────────
root="${tmp}/json"
mk "${root}/crates/fraiseql-server/tests/tenant_e2e_pg.rs" '
fn doc() -> Value {
    json!({ "fraiseql_version": fraiseql_core::schema::CURRENT_FRAISEQL_VERSION, "types": [] })
}
async fn t() { let Some(url) = try_database_url() else { return }; serve(doc(), url); }'
run_gate "$root"
expect 1 "tenant_e2e_pg.rs: loads a document" "a hand-written compiled document needs a guard"

# ── 4. A guard that can skip is refused ──────────────────────────────────────────────
root="${tmp}/skipping"
mk "${root}/crates/fraiseql-server/tests/budget_e2e_pg.rs" '
#[test]
fn the_document_loads_without_a_database() {
    let Some(_url) = try_database_url() else { return; };
    let s = CompiledSchema::from_json(DOC, false).unwrap();
    if s.types.is_empty() { panic!() }
}'
run_gate "$root"
expect 1 "consults DATABASE_URL, so it can skip" "a guard that consults DATABASE_URL fails"

# ── 5. The guard doc may name DATABASE_URL; only its body counts ─────────────────────
root="${tmp}/docmention"
mk "${root}/crates/fraiseql-server/tests/budget_e2e_pg.rs" '
/// Unlike the rest, this needs no DATABASE_URL and never calls try_database_url().
#[test]
fn the_document_loads_without_a_database() {
    CompiledSchema::from_json(DOC, false).unwrap();
}
fn after() { let _ = std::env::var("DATABASE_URL"); }'
run_gate "$root"
expect 0 "all 1 database-gated" "DATABASE_URL in the doc comment or a later fn is not the guard's body"

# ── 6. A suite that loads no document is not asked for a guard ───────────────────────
# Most e2e suites build their schema as a struct literal, which no load-time check sees.
root="${tmp}/literal"
mk "${root}/crates/fraiseql-server/tests/literal_e2e_pg.rs" '
// A comment naming compile_to_schema and CompiledSchema::from_json does not count.
fn schema() -> CompiledSchema { CompiledSchema::default() }'
run_gate "$root"
expect 0 "all 0 database-gated" "a suite loading no document (loader named only in a comment) passes"

# ── 7. A binary that loads a document but never looks for a database is out of scope ─
# Its document is loaded in every run, so a refusal of it already fails.
root="${tmp}/nodb"
mk "${root}/crates/fraiseql-server/tests/federation_unit_test.rs" '
#[test]
fn t() { CompiledSchema::from_json(include_str!("s.json"), false).unwrap(); }'
run_gate "$root"
expect 0 "all 0 database-gated" "a binary that never consults a database is not asked for a guard"

# ── 8. The suffix is not the criterion: any binary that finds a database is in scope ─
# pipeline_e2e_test.rs is not *_e2e_pg.rs and compiles its document after finding one.
root="${tmp}/suffix"
mk "${root}/crates/fraiseql-server/tests/pipeline_e2e_test.rs" '
async fn t() {
    let schema = compile_to_schema(opts).await.unwrap();
    let pg = fraiseql_test_support::postgres().await.expect("db");
}'
run_gate "$root"
expect 1 "pipeline_e2e_test.rs: loads a document" "a non-_pg binary that finds a database needs a guard"

# ── 9. A binary carrying the guard stays in scope when it stops matching ─────────────
# Found by mutation: an edit to the document that removed the loader token the scan
# keys on dropped hs256_revocation_e2e_pg out of scope, and its guard went unrun.
root="${tmp}/sticky"
mk "${root}/crates/fraiseql-server/tests/hs256_e2e_pg.rs" '
fn schema() -> CompiledSchema { serde_json::from_value(json!({ "fraiseql_version": 7 })).unwrap() }
#[test]
fn the_document_loads_without_a_database() { schema(); }'
run_gate "$root"
expect 0 "all 1 database-gated" "a binary defining the guard is in scope without a loader token"

# ── 10. A document an SDK authors and the converter compiles counts as loading one ───
# Eight suites built theirs as an `IntermediateSchema` passed to
# `SchemaConverter::convert_artifact`, which the scan did not name: none carried a guard,
# and five that did left the scan's scope when their guard was removed (#1945).
root="${tmp}/converter"
mk "${root}/crates/fraiseql-server/tests/cache_e2e_pg.rs" '
fn schema() -> CompiledSchema {
    let intermediate: IntermediateSchema = serde_json::from_value(json!({ "types": [] })).unwrap();
    SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default()).unwrap().schema
}
async fn t() { let Some(url) = try_database_url() else { return }; serve(schema(), url); }'
run_gate "$root"
expect 1 "cache_e2e_pg.rs: loads a document but defines no" "a converter-compiled document needs a guard"

# ── 11. A database found through another variable is still a database ──────────────
# The pg_tviews suites read TVIEWS_DATABASE_URL, which `\bDATABASE_URL\b` does not match:
# three of them left the scan's scope when their guard was removed (#1945).
root="${tmp}/tviews"
mk "${root}/crates/fraiseql-server/tests/tviews_e2e_pg.rs" '
fn tviews_database_url() -> Option<String> { std::env::var("TVIEWS_DATABASE_URL").ok() }
async fn t() { let Some(url) = tviews_database_url() else { return }; serve(compile_to_schema(opts), url); }'
run_gate "$root"
expect 1 "tviews_e2e_pg.rs: loads a document but defines no" "a suite gated on TVIEWS_DATABASE_URL needs a guard"

# ── 12. A guard that consults another database variable can skip too ────────────────
root="${tmp}/tviews-skip"
mk "${root}/crates/fraiseql-server/tests/tviews_e2e_pg.rs" '
#[test]
fn the_document_loads_without_a_database() {
    if std::env::var("TVIEWS_DATABASE_URL").is_err() { return; }
    compile_to_schema(opts);
}'
run_gate "$root"
expect 1 "consults DATABASE_URL, so it can skip" "a guard that consults TVIEWS_DATABASE_URL fails"

# ── 13-14. Each other loader the scan names is one ──────────────────────────────────
# Found by mutation (#1945): no case named these two, so dropping either passed.
root="${tmp}/merge"
mk "${root}/crates/fraiseql-server/tests/example_e2e_pg.rs" '
async fn t() { let Some(url) = try_database_url() else { return }; serve(merge_from_domains(dir), url); }'
run_gate "$root"
expect 1 "example_e2e_pg.rs: loads a document" "a merged example needs a guard"

root="${tmp}/fromjson"
mk "${root}/crates/fraiseql-server/tests/parsed_e2e_pg.rs" '
async fn t() {
    let Some(url) = try_database_url() else { return };
    serve(CompiledSchema::from_json(DOC, false).unwrap(), url);
}'
run_gate "$root"
expect 1 "parsed_e2e_pg.rs: loads a document" "parsed compiled JSON needs a guard"

# ── 15. A loader named only in a comment of a database-gated suite does not count ───
# Case 6's suite finds no database, so it passed with comments left in (#1945 mutation).
root="${tmp}/dbcomment"
mk "${root}/crates/fraiseql-server/tests/literal_e2e_pg.rs" '
// Unlike its neighbours, this suite never calls compile_to_schema.
/* nor CompiledSchema::from_json */
async fn t() { let Some(url) = try_database_url() else { return }; serve(CompiledSchema::default(), url); }'
run_gate "$root"
expect 0 "all 0 database-gated" "a database-gated suite naming a loader only in comments passes"

if [ "$failures" -ne 0 ]; then
  echo "e2e_documents_test: ${failures} case(s) FAILED"
  exit 1
fi
echo "e2e_documents_test: all 15 cases passed"
