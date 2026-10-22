#!/usr/bin/env bash
# check-doc-claims.sh — fail when a document claims something the tree does not have.
#
# Background (trust closure, 2026-09-21): after the MySQL, SQLite and SQL Server adapters were
# removed in v2.15.0, seventeen documents still listed them as supported. architecture.md named
# two dozen source files that do not exist, five of them encryption modules that never did. The
# observers crate advertised SMS and push actions its config rejects (H24). roadmap.md said the
# current stable was v2.8.0 while the workspace stood at 2.15.0. README.md said the parity suite
# covered nine SDKs when it drives five. Each rule below is one of those claims, checked against
# the tree rather than against a list someone has to remember to update.
#
# Rules:
#   1. A removed backend (MySQL, SQL Server, SQLite) may be named only where history is recorded
#      — CHANGELOG.md, DEPRECATIONS.md, ADRs, migration guides, audit records, the removal notice
#      and two dated work logs — or on a line that reads as history or as a negative ("removed",
#      "no longer", "not supported", "#374", ...). Any other mention is a claim of support.
#   2. Every `<name>.rs` that architecture.md names exists under crates/, as a file or as a
#      directory of that name.
#   3. In crates/fraiseql-observers nothing is described as a stub, and on its advertising
#      surfaces (README, docs/, doc comments) a line that names SMS or push notifications also
#      says the config rejects them, or cites H24 / #428 / the missing transport.
#   4. roadmap.md carries no version status line; versions are CHANGELOG.md's job.
#   5. README.md's "parity suite: N authoring SDKs" equals the number of SDK directories
#      sdks/official/tests/run_parity.sh names.
#   6. `after:ingest` dispatch is called at-least-once only next to what makes it so: the
#      dispatch ledger (#1175), which records each dispatch with its message and re-runs one a
#      crash left unsettled. Before the ledger the claim was false, and its bare form could
#      return without it. Positional:
#      "at-least-once" counts only on a doc line (a `.rs` comment, or markdown) whose sentence
#      (the line, joined to the line above when that one runs on into it) names both
#      `after:ingest` and its dispatch, and does not name the ledger, so at-least-once
#      *ingestion* (a pull source re-polling into the deduplicating spine), a provider's
#      at-least-once delivery, and outbound delivery stay legitimate.
#   7. No document claims a tamper-evident audit chain until the tree has one (#1630): a line
#      naming `tamper-evident`, a hash chain, `integrity_hash` or `fraiseql_audit_logs` must
#      read as a negative ("not yet", "does not", "#1630", ...). Positional, per line. The rule
#      stands down once crates/ holds both a `CREATE TABLE ... fraiseql_audit_logs` (an INSERT
#      alone does not count) and the CLI ships `fraiseql audit verify`.
#   8. A performance figure in README.md or docs/ names its evidence (#1636): the markdown
#      section it sits in (heading to heading) cites an issue (`#NNN`) or a bench file or
#      harness (`benches/`, `benchmarks/`, `*bench*.rs`, velocitybench). A figure is a speed
#      ratio (`4.2×`, `10x faster`, `30% slower`), a rate (`req/s`, `rps`, `ops/s`, `qps`), or a
#      time (`ms`, `µs`, `ns`) on a line that presents it as measured or as latency (measured,
#      p50, p95, p99, latency, faster, slower, per page, per row, a cell, on a N-row ...).
#      Positional: a citation in another section does not count. Code fences are skipped.
#      History (the rule 1 paths, docs/history/) and docs/runbooks/, whose figures are alert
#      thresholds for an operator to set rather than claims about FraiseQL, are not scanned. No
#      figure anywhere is a failure: the rule would be checking nothing.
#
# A rule with nothing to check is a failure, not a pass: no markdown, no architecture.md, no
# names in it, no observers crate, no roadmap, no README sentence.
#
# Overrides, for testing:  DOC_CLAIMS_ROOT=<dir>
set -euo pipefail
if [ -n "${DOC_CLAIMS_ROOT:-}" ]; then
  cd "$DOC_CLAIMS_ROOT"
else
  cd "$(git rev-parse --show-toplevel)"
fi

failures=0
fail() {
  echo "$1" >&2
  failures=$((failures + 1))
}
report() { sed 's/^/  /' >&2; }

# --- Rule 1: removed backends -------------------------------------------------------------
historical_paths='^\./(CHANGELOG\.md|DEPRECATIONS\.md|docs/security/audits/|docs/adr/|docs/migration/|docs/guides/v1-to-v2-migration\.md|docs/database-compatibility\.md|docs/contributing/dagger-parity-notes\.md|docs/architecture/async-trait-migration\.md)'
historical_words='removed|deleted|no longer|not supported|does not support|unsupported|#374|until |were listed|dropped|previously|used to|legacy'
md_files=$(find . \( -path '*/target' -o -path '*/node_modules' -o -path './.git' -o -path './.phases' -o -path './.claude' \) -prune -o -name '*.md' -print | sort)
if [ -z "$md_files" ]; then
  fail "rule 1: no markdown files found to scan"
else
  hits=$(printf '%s\n' "$md_files" | grep -vE "$historical_paths" | xargs -d '\n' grep -n -E '\b(MySQL|SQL Server|SQLite)\b' 2>/dev/null | grep -viE "$historical_words" || true)
  if [ -n "$hits" ]; then
    fail "rule 1: removed backends (MySQL, SQL Server, SQLite) claimed as current:"
    printf '%s\n' "$hits" | report
  fi
fi

# --- Rule 2: architecture.md names only files that exist ----------------------------------
if [ ! -f architecture.md ]; then
  fail "rule 2: architecture.md not found"
else
  names=$(grep -oE '\b[a-z_]+\.rs\b' architecture.md | sort -u || true)
  if [ -z "$names" ]; then
    fail "rule 2: architecture.md names no .rs files, so there is nothing to check"
  fi
  for n in $names; do
    b=${n%.rs}
    if ! find crates -not -path '*/target/*' \( -name "$n" -o -type d -name "$b" \) -print -quit 2>/dev/null | grep -q .; then
      fail "rule 2: architecture.md names $n, which exists nowhere under crates/ as a file or a directory"
    fi
  done
fi

# --- Rule 3: observers advertise only what the config accepts -----------------------------
obs=crates/fraiseql-observers
if [ ! -d "$obs" ]; then
  fail "rule 3: $obs not found"
else
  stubs=$(grep -rn 'stub for' "$obs" --include='*.rs' --include='*.md' 2>/dev/null || true)
  if [ -n "$stubs" ]; then
    fail "rule 3: fraiseql-observers still describes an action as a stub:"
    printf '%s\n' "$stubs" | report
  fi
  # Advertising surfaces: the crate README, its docs/, and doc comments. A line that names SMS or
  # push must carry the rejection on the same line (or the finding / issue that records it).
  ads=$( { grep -rn -E '^[[:space:]]*//[/!]' "$obs/src" --include='*.rs' 2>/dev/null;
           grep -n '' "$obs/README.md" 2>/dev/null | sed "s#^#$obs/README.md:#";
           grep -rn '' "$obs/docs" --include='*.md' 2>/dev/null; } \
         | grep -iE '\bSMS\b|push notification' | grep -viE 'unsupported|rejected|#428|H24|no real transport' || true)
  if [ -n "$ads" ]; then
    fail "rule 3: fraiseql-observers advertises SMS or push without saying the config rejects them:"
    printf '%s\n' "$ads" | report
  fi
fi

# --- Rule 4: roadmap.md carries no version status ------------------------------------------
if [ ! -f roadmap.md ]; then
  fail "rule 4: roadmap.md not found"
else
  vs=$(grep -n -iE 'current stable|in development|^## .*released' roadmap.md || true)
  if [ -n "$vs" ]; then
    fail "rule 4: roadmap.md carries version status lines; CHANGELOG.md is the record of versions:"
    printf '%s\n' "$vs" | report
  fi
fi

# --- Rule 5: README parity-suite count matches run_parity.sh -------------------------------
parity=sdks/official/tests/run_parity.sh
readme_n=$(grep -oE 'parity suite: [0-9]+ authoring SDKs' README.md 2>/dev/null | grep -oE '[0-9]+' | head -1 || true)
if [ -z "$readme_n" ]; then
  fail "rule 5: README.md has no 'parity suite: N authoring SDKs' sentence to check"
elif [ ! -f "$parity" ]; then
  fail "rule 5: $parity not found"
else
  suite_n=0
  while IFS= read -r t; do
    [ -n "$t" ] && [ -d "sdks/official/$t" ] && suite_n=$((suite_n + 1))
  done < <(grep -oE '\bfraiseql-[a-z]+\b' "$parity" | sort -u || true)
  if [ "$readme_n" != "$suite_n" ]; then
    fail "rule 5: README.md says the parity suite covers $readme_n SDKs; $parity drives $suite_n"
  fi
fi

# --- Rule 6: after:ingest dispatch is not claimed at-least-once (#1175) ----------------------
ingest_docs=$( { grep -rln --include='*.rs' 'after:ingest' crates 2>/dev/null;
                 find . \( -path '*/target' -o -path './.git' -o -path './.phases' -o -path './.claude' \
                        -o -path './docs/adr' \) -prune -o -name '*.md' -print 2>/dev/null \
                   | grep -v '^\./CHANGELOG\.md$' | xargs -d '\n' grep -l 'after:ingest' 2>/dev/null; } \
               | sed 's#^\./##' | sort -u || true)
if [ -z "$ingest_docs" ]; then
  fail "rule 6: no document names after:ingest to check"
else
  claims=$(printf '%s\n' "$ingest_docs" | while IFS= read -r f; do
    awk -v f="$f" '
      { doc = (f !~ /\.rs$/) || ($0 ~ /^[[:space:]]*\/\//) }
      # The sentence this line ends: the line above too, unless that line ended its own.
      { window = (prev != "" && prev !~ /[.:;][[:space:]]*$/) ? prev " " $0 : $0 }
      doc && /at-least-once/ && window !~ /ledger|_fraiseql_inbound_dispatch/ \
          && window ~ /after:ingest/ && window ~ /dispatch/ { print f ":" NR ": " $0 }
      { prev = doc ? $0 : "" }' "$f"
  done)
  if [ -n "$claims" ]; then
    fail "rule 6: after:ingest dispatch claimed at-least-once without naming the dispatch ledger that makes it so (#1175):"
    printf '%s\n' "$claims" | report
  fi
fi

# --- Rule 7: no audit hash chain is claimed until one exists (#1630) ------------------------
chain_terms='tamper-evident|tamper evident|hash chain|hash-chain|integrity_hash|fraiseql_audit_logs'
chain_negatives="not yet|no tamper|does not|doesn't|is not|isn't|planned|#1630|no hash chain"
if grep -rqsE 'CREATE TABLE[^;]*fraiseql_audit_logs' crates \
   && grep -rqs 'fraiseql audit verify' crates/fraiseql-cli/src; then
  :
elif [ -n "$md_files" ]; then
  hits=$(printf '%s\n' "$md_files" | grep -vE "$historical_paths" | xargs -d '\n' grep -n -iE "$chain_terms" 2>/dev/null | grep -viE "$chain_negatives" || true)
  if [ -n "$hits" ]; then
    fail "rule 7: a tamper-evident audit chain is claimed, and the tree has none (#1630):"
    printf '%s\n' "$hits" | report
  fi
fi

# --- Rule 8: performance figures name their evidence (#1636) -------------------------------
perf_docs=$(printf '%s\n' "$md_files" | grep -E '^\./(README\.md|docs/)' \
            | grep -vE "$historical_paths" | grep -vE '^\./docs/(history|runbooks)/' || true)
if [ -z "$perf_docs" ]; then
  fail "rule 8: no README.md or docs/ markdown to check"
else
  perf=$(printf '%s\n' "$perf_docs" | xargs -d '\n' awk '
    function flush() {
      if (nfig > 0 && !cited) for (i = 1; i <= nfig; i++) print "UNCITED " fig[i]
      nfig = 0; cited = 0
    }
    FNR == 1 { flush(); fence = 0; file = substr(FILENAME, 3) }
    /^[[:space:]]*```/ { fence = !fence; next }
    fence { next }
    /^#{1,6} / { flush() }
    /#[0-9][0-9][0-9]+([^0-9]|$)|benches\/|benchmarks\/|velocitybench|[a-z_]*bench[a-z_]*\.rs/ { cited = 1 }
    {
      l = $0
      ratio = l ~ /[0-9][0-9.]*[[:space:]]*×([[:space:]]*(faster|slower|the|cheaper|speedup)|\*\*|\)|[[:space:]]*\||[[:space:]]*$)/ \
           || l ~ /[0-9][0-9.]*x[[:space:]]+(faster|slower|cheaper|speedup)/ \
           || l ~ /[0-9]+%[[:space:]]+(faster|slower)/
      rate = l ~ /[0-9][0-9,.]*[[:space:]]*(req\/s|rps|ops\/s|qps)([^A-Za-z0-9]|$)/
      tm   = l ~ /[0-9][0-9,.]*[[:space:]]*(ms|µs|ns)([^A-Za-z0-9]|$)/ \
          && tolower(l) ~ /measured|p50|p9[59]|latency|faster|slower|per page|per row|a cell|on a [0-9]/
      if (ratio || rate || tm) { total++; nfig++; fig[nfig] = file ":" FNR ": " l }
    }
    END { flush(); print "TOTAL " total + 0 }' 2>/dev/null || true)
  if [ "$(printf '%s\n' "$perf" | sed -n 's/^TOTAL //p')" = "0" ]; then
    fail "rule 8: no performance figure found in README.md or docs/, so there is nothing to check"
  fi
  uncited=$(printf '%s\n' "$perf" | sed -n 's/^UNCITED //p')
  if [ -n "$uncited" ]; then
    fail "rule 8: performance figures whose section cites no issue or bench (#1636):"
    printf '%s\n' "$uncited" | report
  fi
fi

if [ "$failures" -gt 0 ]; then
  echo "doc claims: $failures rule failure(s)" >&2
  exit 1
fi
echo "doc claims: ok — backends, architecture names, observer actions, roadmap status, the parity count, the after:ingest guarantee, the audit chain and the performance citations all match the tree"
