#!/usr/bin/env bash
# check-test-imports.sh — fail when any file under crates/ resolves DATABASE_URL
# itself instead of going through the canonical test helper.
#
# Why this exists: `fraiseql_test_support::database_url()` panics with an actionable
# message when the variable is unset, and `try_database_url()` is the explicit
# self-skipping form. Its own doc comment states the stake: "A swallowed or
# silently-defaulted URL here would let every DB-backed test skip when CI fails to
# inject the URL — a false-green meta-risk larger than most single findings."
# Resolving the variable inline is how a test acquires a silent default.
#
# ⚠ THIS GATE COULD NEVER FAIL UNTIL #1075. The pattern was written as
#
#     PATTERN='std::env::var\("DATABASE_URL"\)'
#     grep -r "$PATTERN" …
#
# and `grep` without `-E` is BRE, where `\(` and `\)` are *group delimiters*, not
# literal parentheses. The regex therefore matched `std::env::var"DATABASE_URL"` —
# a string that cannot occur in Rust source. The gate printed OK on every tree, in
# `make preflight` and in the required Dagger ShellGates leg, while 15 files under
# `crates/*/tests/` carried the exact text it forbade. Three of them defaulted to a
# *different* database (`fraiseql_bench`, `test_fraiseql`, `fraiseql_test`) when the
# variable was unset, which is the false green the rule exists to prevent.
#
# Two consequences shape what this script now does:
#
#   1. It matches with `grep -F` on the bare variable name and then filters, rather
#      than with a hand-escaped regex. No escaping, nothing to get wrong.
#   2. It is FAIL-CLOSED over both `crates/*/tests/` and `crates/*/src/`. The old
#      scope was `crates/*/tests/` only, so unit tests living in `src/**/tests.rs`
#      and in inline `#[cfg(test)] mod` blocks were invisible even to a working
#      pattern — and four such violations existed. Production code that legitimately
#      reads the variable (the CLI's `--database-url` fallbacks, the server's config
#      loader) is named in the allowlist with a reason, so a NEW production reader is
#      a deliberate edit rather than a silent one.
#
# Scope notes, stated rather than implied:
#   - `benches/` and `examples/` are out of scope: a benchmark reading its own target
#     URL is not a test that can go falsely green.
#   - `TLS_DATABASE_URL`, `TEST_DATABASE_URL` and `STANDBY_DATABASE_URL` are separate
#     variables with their own resolution policy; this gate is about `DATABASE_URL`.
#
# Overrides, for testing:
#   TEST_IMPORTS_ALLOW=<path>   allowlist to use instead of the default
#   TEST_IMPORTS_ROOT=<dir>     tree to scan instead of the repo root
set -euo pipefail
cd "${TEST_IMPORTS_ROOT:-$(git rev-parse --show-toplevel)}"

ALLOW_FILE="${TEST_IMPORTS_ALLOW:-tools/test-imports.allow}"

if [ ! -f "$ALLOW_FILE" ]; then
  echo "ERROR: allowlist not found: $ALLOW_FILE" >&2
  exit 1
fi

# Allowed paths, comments and blank lines stripped.
allowed="$(sed 's/#.*//' "$ALLOW_FILE" | awk 'NF {print $1}' | sort -u)"

# Every file that names the variable at all. `-F` so there is no pattern to escape;
# the `DATABASE_URL` suffix guard below is what separates it from its siblings.
candidates="$(grep -rlF 'DATABASE_URL' crates/*/src/ crates/*/tests/ --include='*.rs' 2>/dev/null | sort -u || true)"

violations=""
for file in $candidates; do
  # Only a bare resolution of DATABASE_URL itself counts. `env::var("DATABASE_URL")`
  # in any spelling — `std::env::var`, a `use std::env` then `env::var`, or a bare
  # `var(` after `use std::env::var` — and not TLS_/TEST_/STANDBY_DATABASE_URL, whose
  # names end in the same token.
  if grep -nE '(^|[^_[:alnum:]])var\("DATABASE_URL"\)' "$file" >/dev/null 2>&1; then
    if ! printf '%s\n' "$allowed" | grep -qxF "$file"; then
      violations="${violations}${file}"$'\n'
    fi
  fi
done

# A stale allowlist row is a failure too: an entry for a file that no longer reads the
# variable (or no longer exists) is an exemption nobody pruned, and the next reader
# trusts it. A gate that only fails one way rots into an allowlist nobody reads.
stale=""
while IFS= read -r entry; do
  [ -z "$entry" ] && continue
  if [ ! -f "$entry" ] || ! grep -nE '(^|[^_[:alnum:]])var\("DATABASE_URL"\)' "$entry" >/dev/null 2>&1; then
    stale="${stale}${entry}"$'\n'
  fi
done <<<"$allowed"

status=0

if [ -n "$violations" ]; then
  status=1
  echo "ERROR: bare DATABASE_URL resolution outside the canonical helper." >&2
  echo >&2
  echo "  Test code must use:" >&2
  echo "    fraiseql_test_support::database_url()      — panics with an actionable message" >&2
  echo "    fraiseql_test_support::try_database_url()  — None, for a test that self-skips" >&2
  echo >&2
  echo "  (fraiseql_test_utils re-exports both, if the crate already depends on it.)" >&2
  echo >&2
  echo "  Production code that must read the variable belongs in $ALLOW_FILE, with a reason." >&2
  echo >&2
  printf '%s' "$violations" | sed 's/^/  /' >&2
fi

if [ -n "$stale" ]; then
  status=1
  echo "ERROR: stale rows in $ALLOW_FILE — these no longer resolve DATABASE_URL:" >&2
  echo >&2
  printf '%s' "$stale" | sed 's/^/  /' >&2
  echo >&2
  echo "  Remove them, so the allowlist keeps meaning what it says." >&2
fi

if [ "$status" -ne 0 ]; then
  exit 1
fi

allowed_count="$(printf '%s\n' "$allowed" | awk 'NF' | wc -l | tr -d ' ')"
echo "OK: no bare DATABASE_URL resolution in crates/ ($allowed_count allowed production readers)."
