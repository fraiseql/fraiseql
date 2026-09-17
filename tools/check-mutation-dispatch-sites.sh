#!/usr/bin/env bash
# check-mutation-dispatch-sites.sh — pin the set of production sites that dispatch a
# mutation to the database, so a new transport cannot execute a write past the engine's
# gates (#1327).
#
# WHY THIS GATE EXISTS
# --------------------
# `execute_mutation_impl` (fraiseql-core, runtime/executor/runners/mutation/mod.rs) is
# where every gate on a write is enforced: the operation `Authorizer` (#422),
# `requires_role`, `requires_actor` (#966), selection-set validation (#1005),
# argument-name validation (#1154), the inline-argument merge (#719), the change-log
# write, and the `before:mutation` chain (#1327). Its doc comment calls itself "the
# universal mutation chokepoint", and that claim is only true while nothing reaches the
# adapter's mutation methods on its own.
#
# #1327 is what happens when enforcement lives anywhere else. The `before:mutation`
# chain ran in the GraphQL handler, so three request shapes executed a mutation without
# it — a second root field, inline arguments, and the REST write route. Moving the chain
# to the chokepoint fixes those three; this gate is what keeps a FOURTH from appearing,
# because a new route that calls the adapter directly would be invisible to every test
# the three bypasses have.
#
# It was not hypothetical. The gRPC mutation arm did exactly that until #1330: it built
# its arguments from the protobuf message and called `execute_function_call` itself, so
# every gate above was skipped on that transport. It is fixed, and the KNOWN list is now
# EMPTY — which is the state this gate exists to hold. An entry here is a named defect
# with an issue, never a resting place.
#
# Mirrors the established shell-gate pattern (lint-graphql-parse, lint-internal-flag).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The adapter layer IMPLEMENTS these methods, so it necessarily calls them: the trait's
# own default impls, the PostgreSQL adapter's internal fallbacks, and the caching
# adapter that wraps another adapter and forwards. None of them is a route — they sit
# BELOW the chokepoint, not around it. `cache/adapter/mutation.rs` additionally calls
# `bump_tf_version`, a cache-invalidation function, which is not a user mutation.
ALLOWED_DIRS='crates/fraiseql-db/src/|crates/fraiseql-core/src/cache/adapter/'

# The one engine seam every transport must converge on.
CHOKEPOINT='crates/fraiseql-core/src/runtime/executor/runners/mutation/mod.rs'

# Sites that bypass the chokepoint and are tracked as defects. Each needs an issue.
# Empty since #1330: every transport converges on the chokepoint.
KNOWN=''

# Every adapter method that performs a write. `execute_direct_mutation` is the DirectSql
# strategy; the `execute_function_call*` family is the stored-function strategy.
PATTERN='\.(execute_function_call(_with_session|_with_changelog|_dry_run)?|execute_direct_mutation)\s*\('

# Production code only: a test may drive an adapter directly, and a bench must.
violations=$(
  grep -rEn "$PATTERN" crates/*/src --include='*.rs' \
    | grep -vE '/(tests|[a-z_]+_tests)\.rs:' \
    | grep -vE '^[^:]*/tests/' \
    | grep -vE "^($ALLOWED_DIRS)" \
    | grep -vE "^($CHOKEPOINT):" \
    | { if [ -n "$KNOWN" ]; then grep -vE "^($KNOWN):"; else cat; fi } \
    | grep -vE ':[0-9]+:\s*(//|///|//!)' \
    || true
)

if [ -n "$violations" ]; then
  echo "ERROR: a mutation is dispatched to the database outside the engine chokepoint (#1327):"
  echo "$violations"
  echo
  echo "Every gate on a write — the Authorizer (#422), requires_role, requires_actor"
  echo "(#966), selection-set (#1005) and argument-name (#1154) validation, the"
  echo "inline-argument merge, the change-log write and the before:mutation chain —"
  echo "is enforced in execute_mutation_impl. A site that calls the adapter itself"
  echo "skips all of them, silently."
  echo
  echo "Route this through the executor instead:"
  echo "  Executor::execute / execute_with_security   (a GraphQL document)"
  echo "  Executor::execute_mutation                  (typed, SupportsMutations)"
  echo "  Executor::execute_mutation_as               (structured args + a principal)"
  echo "  Executor::execute_mutation_with_security    (a REST write with a principal)"
  echo
  echo "If a site genuinely cannot, add it to KNOWN in"
  echo "tools/check-mutation-dispatch-sites.sh with the issue that tracks the gap."
  exit 1
fi

# The allowlists are only safe while they are exact. A chokepoint that stopped
# dispatching, or a KNOWN entry that was fixed, must not stay listed: the first means
# the gate is pinning nothing, the second means the gate is excusing a file that no
# longer needs it and would hide a NEW bypass added to that same file.
chokepoint_calls=$(grep -rEc "$PATTERN" "$CHOKEPOINT" || true)
if [ "${chokepoint_calls:-0}" -eq 0 ]; then
  echo "ERROR: $CHOKEPOINT no longer dispatches any mutation."
  echo "Either the chokepoint moved (update CHOKEPOINT) or this gate is now vacuous."
  exit 1
fi

for known in $(echo "${KNOWN:-}" | tr '|' ' '); do
  if [ ! -f "$known" ]; then
    echo "ERROR: KNOWN entry $known does not exist — remove it from this gate."
    exit 1
  fi
  if ! grep -rEq "$PATTERN" "$known"; then
    echo "ERROR: KNOWN entry $known no longer dispatches a mutation directly."
    echo "The bypass it tracks is fixed: remove it from KNOWN so the file is gated again."
    exit 1
  fi
done

if [ -n "$KNOWN" ]; then
  echo "OK: mutations are dispatched only from the engine chokepoint (known bypasses: $KNOWN)"
else
  echo "OK: mutations are dispatched only from the engine chokepoint (no known bypasses)"
fi
