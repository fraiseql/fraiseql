#!/usr/bin/env bash
# Unit tests for tools/check-doc-claims.sh.
#
# Run directly:  bash tools/tests/check_doc_claims_test.sh
# Exits non-zero if any assertion fails.
#
# One clean fixture tree, then one mutation per rule in each direction: the claim the rule
# exists to catch is red, the historical or negative form the rule must tolerate is green, and
# a tree with nothing for a rule to scan is red rather than vacuously green.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-doc-claims.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# clean_root <dir>: a tree every rule passes on.
clean_root() {
    local r="$1"
    mkdir -p "$r/docs/adr" "$r/crates/fraiseql-observers/src" "$r/crates/x/src/real_dir" \
             "$r/sdks/official/tests" "$r/sdks/official/fraiseql-a" "$r/sdks/official/fraiseql-b"
    printf '# guide\nPostgreSQL only.\n\n## Speed\nA cached read measured 0.2 ms (#1234).\n' > "$r/docs/guide.md"
    printf '# changelog\nMySQL support\n' > "$r/CHANGELOG.md"
    printf 'crates/x/src/real.rs   thing\nreal_dir.rs   moved into a directory\n' > "$r/architecture.md"
    : > "$r/crates/x/src/real.rs"
    printf '//! Actions: webhook, email.\n' > "$r/crates/fraiseql-observers/src/lib.rs"
    printf '# roadmap\n- open epics only\n' > "$r/roadmap.md"
    printf 'Cross-SDK parity suite: 2 authoring SDKs produce identical schema JSON\n' > "$r/README.md"
    printf 'emit fraiseql-a\nemit fraiseql-b\nfraiseql-cli compile\n' > "$r/sdks/official/tests/run_parity.sh"
    mkdir -p "$r/crates/fraiseql-functions/src"
    printf '/// Persists a message, then dispatches `after:ingest` once it commits.\n' \
        > "$r/crates/fraiseql-functions/src/spine.rs"
}

# assert_gate <name> <expected-rc> <expected-substring> <root>
assert_gate() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local name="$1" want_rc="$2" want_sub="$3" root="$4"
    local out rc
    set +e
    out="$(DOC_CLAIMS_ROOT="$root" bash "$GATE" 2>&1)"
    rc=$?
    set -e
    if [[ "$rc" -eq "$want_rc" && "$out" == *"$want_sub"* ]]; then
        echo "  ok: $name"
    else
        echo "  FAIL: $name — rc=$rc (want $want_rc), output did not contain '$want_sub':" >&2
        printf '%s\n' "$out" | sed 's/^/      /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "check-doc-claims.sh:"
r="$WORK/clean"; clean_root "$r"
assert_gate "clean tree passes" 0 "doc claims: ok" "$r"

r="$WORK/backend"; clean_root "$r"; printf 'Supports MySQL and SQLite.\n' >> "$r/docs/guide.md"
assert_gate "rule 1: backend claimed as current is red" 1 "rule 1" "$r"

r="$WORK/backend_hist"; clean_root "$r"; printf 'MySQL was removed in v2.15.0.\n' >> "$r/docs/guide.md"
assert_gate "rule 1: historical phrasing passes" 0 "doc claims: ok" "$r"

r="$WORK/backend_adr"; clean_root "$r"; printf 'We chose MySQL.\n' > "$r/docs/adr/0001.md"
assert_gate "rule 1: an ADR may name a backend" 0 "doc claims: ok" "$r"

r="$WORK/ghost"; clean_root "$r"; printf 'ghost.rs   never existed\n' >> "$r/architecture.md"
assert_gate "rule 2: phantom file is red" 1 "names ghost.rs" "$r"

r="$WORK/no_names"; clean_root "$r"; printf 'prose only\n' > "$r/architecture.md"
assert_gate "rule 2: architecture.md naming nothing is red" 1 "nothing to check" "$r"

r="$WORK/sms"; clean_root "$r"; printf '//! - SMS, push notifications\n' >> "$r/crates/fraiseql-observers/src/lib.rs"
assert_gate "rule 3: SMS advertised is red" 1 "advertises SMS" "$r"

r="$WORK/sms_ok"; clean_root "$r"; printf '//! SMS and push are rejected as unsupported (H24); see #428.\n' >> "$r/crates/fraiseql-observers/src/lib.rs"
assert_gate "rule 3: SMS named with the rejection passes" 0 "doc claims: ok" "$r"

r="$WORK/stub"; clean_root "$r"; printf '/// Send SMS (stub for)\n' >> "$r/crates/fraiseql-observers/src/lib.rs"
assert_gate "rule 3: stub wording is red" 1 "as a stub" "$r"

r="$WORK/roadmap"; clean_root "$r"; printf '**Current Stable**: v2.8.0\n' >> "$r/roadmap.md"
assert_gate "rule 4: version status line is red" 1 "rule 4" "$r"

r="$WORK/count"; clean_root "$r"; sed -i 's/2 authoring/9 authoring/' "$r/README.md"
assert_gate "rule 5: README count differing from the suite is red" 1 "covers 9 SDKs" "$r"

r="$WORK/no_sentence"; clean_root "$r"; printf 'nothing here\n' > "$r/README.md"
assert_gate "rule 5: README without the sentence is red" 1 "no 'parity suite" "$r"

r="$WORK/chain"; clean_root "$r"; printf 'Audit entries form a SHA256 hash chain.\n' >> "$r/docs/guide.md"
assert_gate "rule 7: an audit hash chain claimed is red" 1 "rule 7" "$r"
r="$WORK/chain_query"; clean_root "$r"; printf 'Verify with `SELECT integrity_hash FROM fraiseql_audit_logs`.\n' >> "$r/docs/guide.md"
assert_gate "rule 7: the verification query is red" 1 "rule 7" "$r"
r="$WORK/chain_neg"; clean_root "$r"; printf 'There is no tamper-evident audit chain yet (#1630).\n' >> "$r/docs/guide.md"
assert_gate "rule 7: the negative form passes" 0 "doc claims: ok" "$r"
r="$WORK/chain_table_only"; clean_root "$r"; printf 'Audit entries form a hash chain.\n' >> "$r/docs/guide.md"
printf 'CREATE TABLE fraiseql_audit_logs (id bigint);\n' > "$r/crates/x/src/audit.sql"
assert_gate "rule 7: a table without the verify command is still red" 1 "rule 7" "$r"
r="$WORK/chain_built"; clean_root "$r"; printf 'Audit entries form a hash chain.\n' >> "$r/docs/guide.md"
printf 'CREATE TABLE fraiseql_audit_logs (id bigint);\n' > "$r/crates/x/src/audit.sql"
mkdir -p "$r/crates/fraiseql-cli/src"; printf '/// `fraiseql audit verify` checks the chain.\n' > "$r/crates/fraiseql-cli/src/audit.rs"
assert_gate "rule 7: the claim passes once the chain and its verifier exist" 0 "doc claims: ok" "$r"
r="$WORK/ingest"; clean_root "$r"
printf '/// so `after:ingest` dispatch is at-least-once.\n' >> "$r/crates/fraiseql-functions/src/spine.rs"
assert_gate "rule 6: at-least-once dispatch claimed on one line is red" 1 "rule 6" "$r"

r="$WORK/ingest_wrapped"; clean_root "$r"
printf '/// then dispatch `after:ingest`\n/// for the new messages (durable, at-least-once).\n' \
    >> "$r/crates/fraiseql-functions/src/spine.rs"
assert_gate "rule 6: a claim wrapped onto the next line is red" 1 "rule 6" "$r"

r="$WORK/ingest_md"; clean_root "$r"
printf 'Its `after:ingest` dispatch is at-least-once.\n' >> "$r/docs/guide.md"
assert_gate "rule 6: the claim in markdown is red" 1 "rule 6" "$r"

r="$WORK/ingest_ok"; clean_root "$r"
printf '/// Re-polling the window into the spine makes ingestion at-least-once.\n' \
    >> "$r/crates/fraiseql-functions/src/spine.rs"
printf '/// A provider retries, so delivery is at-least-once and `after:ingest` handlers\n' \
    >> "$r/crates/fraiseql-functions/src/spine.rs"
printf '/// The dispatch ledger makes `after:ingest` dispatch at-least-once.\n' \
    >> "$r/crates/fraiseql-functions/src/spine.rs"
printf 'let s = "after:ingest dispatch is at-least-once";\n' \
    >> "$r/crates/fraiseql-functions/src/spine.rs"
assert_gate "rule 6: ingestion, provider delivery, the ledger's claim and code pass" 0 "doc claims: ok" "$r"

r="$WORK/ingest_cite"; clean_root "$r"
printf '/// `after:ingest` dispatch is at-least-once (#1175).\n' >> "$r/crates/fraiseql-functions/src/spine.rs"
assert_gate "rule 6: citing the issue is not naming the ledger" 1 "rule 6" "$r"

r="$WORK/ingest_none"; clean_root "$r"; rm "$r/crates/fraiseql-functions/src/spine.rs"
assert_gate "rule 6: no after:ingest doc to check is red" 1 "rule 6: no document" "$r"

# Rule 8: each figure form is red with no citation in its section.
for case in 'ratio:Reads are 4.2× faster than before.' \
            'rate:It serves 5,000 req/s on one core.' \
            'time:Measured at 3 ms per page.' \
            'pct:The new path is 30% faster.' \
            'x:Columnar reads are 10x faster.' \
            'cell:| native `id` | **4.2×** |' \
            'p99:Expected: P99 < 200ms for moderate queries'; do
    r="$WORK/perf_${case%%:*}"; clean_root "$r"
    printf '\n## Claim\n%s\n' "${case#*:}" >> "$r/docs/guide.md"
    assert_gate "rule 8: an uncited ${case%%:*} figure is red" 1 "rule 8: performance figures" "$r"
done

r="$WORK/perf_readme"; clean_root "$r"
printf 'Execute with predictable sub-10ms latency.\n' >> "$r/README.md"
assert_gate "rule 8: README.md is scanned" 1 "README.md:2:" "$r"

r="$WORK/perf_positional"; clean_root "$r"
printf '\n## Claim\nReads are 4.2× faster.\n\n## Notes\nSee #1636.\n' >> "$r/docs/guide.md"
assert_gate "rule 8: a citation in another section does not count" 1 "rule 8: performance figures" "$r"

r="$WORK/perf_cited"; clean_root "$r"
printf '\n## Claim\nReads are 4.2× faster.\n\nThe harness is in `benches/full_pipeline.rs`.\n' >> "$r/docs/guide.md"
printf '\n## Other\nIt serves 5,000 req/s (#1587).\n' >> "$r/docs/guide.md"
printf '\n## Third\nMeasured at 3 ms with velocitybench.\n' >> "$r/docs/guide.md"
assert_gate "rule 8: an issue, a bench file or a harness in the section passes" 0 "doc claims: ok" "$r"

r="$WORK/perf_not_figures"; clean_root "$r"
printf '\n## Words\nNeeds thiserror 2.x and a 500×500 render. The budget defaults to 5 000 ms.\n' >> "$r/docs/guide.md"
printf '\n```text\nReads are 4.2× faster.\n```\n' >> "$r/docs/guide.md"
assert_gate "rule 8: versions, dimensions, config defaults and code fences are not figures" 0 "doc claims: ok" "$r"

r="$WORK/perf_exempt"; clean_root "$r"; mkdir -p "$r/docs/runbooks" "$r/docs/history"
printf '# Latency\nAlert when p99 > 500ms.\n' > "$r/docs/runbooks/latency.md"
printf '# Old\nA 3-5× rebuild speedup.\n' > "$r/docs/history/old.md"
printf '# ADR\nReads were 4.2× faster.\n' > "$r/docs/adr/0001.md"
assert_gate "rule 8: runbooks, history and ADRs are not scanned" 0 "doc claims: ok" "$r"

r="$WORK/perf_none"; clean_root "$r"
printf '# guide\nPostgreSQL only.\n' > "$r/docs/guide.md"
assert_gate "rule 8: no figure anywhere is red" 1 "rule 8: no performance figure" "$r"

r="$WORK/no_md"; mkdir -p "$r"
assert_gate "empty tree is red on every rule, not green" 1 "rule 1: no markdown" "$r"

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
