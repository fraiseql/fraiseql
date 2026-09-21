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

if [ "$failures" -gt 0 ]; then
  echo "doc claims: $failures rule failure(s)" >&2
  exit 1
fi
echo "doc claims: ok — backends, architecture names, observer actions, roadmap status and the parity count all match the tree"
