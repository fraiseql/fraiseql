#!/usr/bin/env bash
# check-rls-policy-construction.sh — no production code outside `fraiseql-core` may
# construct an `RLSPolicy`; it must consult the one the deployment configured (#1348).
#
# WHY THIS GATE EXISTS
# --------------------
# `RuntimeConfig.rls_policy` is `Option<Arc<dyn RLSPolicy>>` and defaults to `None` —
# "no row-level security". The engine's read path applies a policy only when one is
# configured (`runners/query_regular.rs`, `runners/aggregate.rs`).
#
# Both gRPC read arms used to build `DefaultRLSPolicy::new()` themselves, and that was
# wrong in **both** directions at once:
#
#   1. a deployment that configured a custom policy never had it consulted — a security
#      control silently not the one in force;
#   2. a deployment that configured none got `DefaultRLSPolicy` on gRPC reads and no RLS
#      on GraphQL/REST reads, so the same query returned different rows depending on
#      which transport asked. (2) filters *more*, so it fails safe in the narrow sense —
#      and surfaces as "the gRPC client is missing rows" long after the cause is
#      forgotten.
#
# Neither direction is visible from a test of that transport alone: the arm produced a
# perfectly good WHERE clause, just not the configured one. What is checkable is the
# shape — production code building a policy instead of being handed one.
#
# Tests are exempt: a test that drives "the configured policy is the one applied" has to
# construct one to pass in, and that is the assertion rather than the defect.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Every way of naming a concrete policy in the tree.
#
# ⚠ Matching only `::new(` / `::default(` was not enough, and the gate's own proof run is
# what showed it: `NoRLSPolicy` is a **unit struct**, so `let p = NoRLSPolicy;` constructs
# one and slipped straight through. The bare names are matched instead — which also catches
# a `use` or a type annotation, and that is wanted: production code outside `fraiseql-core`
# has no business naming a concrete policy at all, only consulting the configured one.
PATTERN='(DefaultRLSPolicy|NoRLSPolicy|CompiledRLSPolicy)'

# `fraiseql-core` owns the policies: it defines them, and its own read path resolves the
# configured one. A construction there is the implementation, not a bypass.
OWNER='crates/fraiseql-core/src/'

# Sites that build their own policy and are tracked as defects. Each needs an issue.
# Empty since #1348: every read consults `RuntimeConfig.rls_policy`.
KNOWN=''

violations=$(
  grep -rEn "$PATTERN" crates/*/src --include='*.rs' \
    | grep -vE '/(tests|[a-z_]+_tests)\.rs:' \
    | grep -vE '^[^:]*/tests/' \
    | grep -vE '/test_support\.rs:' \
    | grep -vE "^($OWNER)" \
    | { if [ -n "$KNOWN" ]; then grep -vE "^($KNOWN):"; else cat; fi } \
    | grep -vE ':[0-9]+:\s*(//|///|//!)' \
    || true
)

if [ -n "$violations" ]; then
  echo "ERROR: production code outside fraiseql-core constructs an RLSPolicy (#1348):"
  echo "$violations"
  echo
  echo "A policy built here is not the policy the deployment configured. Consult"
  echo "RuntimeConfig.rls_policy instead — and treat None as 'no row filter', which is"
  echo "what the engine's read path does:"
  echo "  self.executor.config().rls_policy.as_deref()"
  echo
  echo "Inventing DefaultRLSPolicy when none is configured is not a safe default: it"
  echo "makes the same query answer differently per transport."
  echo
  echo "If a site genuinely cannot reach the configured policy, add it to KNOWN in"
  echo "tools/check-rls-policy-construction.sh with the issue that tracks the gap."
  exit 1
fi

# The allowlist is only safe while it is exact.
for known in $(echo "${KNOWN:-}" | tr '|' ' '); do
  if [ ! -f "$known" ]; then
    echo "ERROR: KNOWN entry $known does not exist — remove it from this gate."
    exit 1
  fi
  # Comment lines do not construct anything, and the fix for a listed bypass almost
  # always leaves a comment naming what it replaced — so counting prose here would make a
  # KNOWN entry unprunable forever, which is the failure mode #1349 hit in the sibling
  # gate. Strip comments before counting.
  hits=$(grep -vE '^\s*(//|///|//!|\*)' "$known" | grep -cE "$PATTERN" || true)
  if [ "${hits:-0}" -eq 0 ]; then
    echo "ERROR: KNOWN entry $known no longer constructs an RLSPolicy."
    echo "The bypass it tracks is fixed: remove it from KNOWN so the file is gated again."
    exit 1
  fi
done

# Non-vacuity: if the policies were renamed, this gate would match nothing and pass over
# a tree full of bypasses. Check the owner still defines them.
for policy in DefaultRLSPolicy NoRLSPolicy CompiledRLSPolicy; do
  if ! grep -rqE "struct $policy" "$OWNER"; then
    echo "ERROR: $policy no longer exists in $OWNER."
    echo "Either it was renamed (update PATTERN) or this gate is now vacuous."
    exit 1
  fi
done

# And that the consuming side still reads the configured policy, so "consult it instead"
# names something real.
if ! grep -rqE "config\(\)\.rls_policy|config\.rls_policy" crates/*/src --include='*.rs'; then
  echo "ERROR: nothing reads RuntimeConfig.rls_policy any more."
  echo "The advice this gate gives would be impossible to follow; re-target it."
  exit 1
fi

if [ -n "$KNOWN" ]; then
  echo "OK: RLS policies are constructed only in fraiseql-core (known bypasses: $KNOWN)"
else
  echo "OK: RLS policies are constructed only in fraiseql-core (no known bypasses)"
fi
