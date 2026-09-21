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
# TWO RULES, because one pattern could not see the second shape.
#
# Rule 1 matches the adapter's named write methods. Rule 2 exists because a bypass does not
# have to call one: it can build an INSERT/UPDATE/DELETE itself and hand it to
# `execute_raw_query`, which is a general query method with ~20 legitimate production callers
# (Arrow analytics, tenancy DDL, the fact-table cache, the sql_source probe). Matching that
# method alone would have been 19 allowlist entries and one defect — an allowlist nobody
# reads. So rule 2 matches the PAIR: a file that both builds write SQL and dispatches raw.
#
# That pair is precise in the way that matters — it discriminates a fixed path from an
# unfixed one. `flight_server/handlers/do_exchange.rs` moved off `execute_raw_query` in #953
# so the rows and their change-log outbox rows commit together; it names the method only in
# the comment explaining that, so it does not match. `handlers/do_put.rs` matched until
# #1355, when it made the same move. A gate that cannot tell those two apart would be
# useless here, and the pair is what tells them apart: both files still build write SQL, and
# only the unfixed one also dispatched it raw.
#
# ⚠ Both bypasses rule 2 was written for were invisible to this gate while it printed "no
# known bypasses". That sentence is the reason the gate exists; it must not be able to be
# false. Both are now fixed — the saga's local write (#1354), whose orchestrator moved above
# the engine so its local arm calls the chokepoint, and the Flight DoPut upload (#1355),
# which moved onto `execute_gated_upload`. The staleness loop below is what said so each
# time, by refusing to keep listing a file that had stopped matching. KNOWN_RAW is empty,
# which is the state this gate exists to hold.
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

# Every adapter method that performs a write: the `execute_function_call*` family, which is
# the stored-function strategy and now the only one. `execute_direct_mutation` was the second
# alternative here until the `DirectSql` strategy was deleted — no adapter had been able to
# return it since #374, so the arm was unreachable and untested. A pattern naming a method
# that no longer exists is the same stale claim this gate just stopped making about its own
# allowlist: it reads as coverage and matches nothing.
PATTERN='\.execute_function_call(_with_session|_with_changelog|_dry_run)?\s*\('

# Rule 2's two halves. A file matching BOTH constructs a write statement and dispatches it
# outside the chokepoint. Deliberately not keyed on the builder function names alone: a raw
# `"INSERT INTO ..."` literal counts too, so renaming a helper does not slip past.
RAW_DISPATCH='\.execute_raw_query\s*\('
WRITE_SQL='build_(insert|update|delete)_query|"[[:space:]]*(INSERT INTO|UPDATE |DELETE FROM)'

# Files that build write SQL and dispatch it raw. Each is a named defect with an issue, and
# each must keep matching both halves or it is stale — see the staleness loop below.
# Empty since #1355: no production file both builds a write statement and hands it to
# `execute_raw_query`. An entry here is a named defect with an issue, never a resting place.
KNOWN_RAW=''

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

# ── Rule 2 ────────────────────────────────────────────────────────────────────────────
# Production files only, and comment lines stripped before either half is counted: the very
# file this rule must NOT match explains #953 by naming `execute_raw_query` in prose. A gate
# that counted that comment would report the fixed path as the defect.
#
# ⚠ Counted with `grep -c ... || true`, never `| grep -q`: under `pipefail` a matching
# `grep -q` closes the pipe, the upstream dies of SIGPIPE, and the pipeline reports failure
# *because it matched* — the inversion that made check-principal-producers.sh's main loop
# skip every file while passing.
# Files under crates/*/src that match "$1", minus test files: a test may drive an adapter
# directly and a bench must. Factored out because the non-vacuity check below has to apply
# exactly the same filter to each half — a half measured over a different file set than the
# pair loop uses could report a pattern live that the loop never reaches.
production_files_matching() {
  grep -rlE "$1" crates/*/src --include='*.rs' \
    | grep -vE '/(tests|[a-z_]+_tests)\.rs$' \
    | grep -vE '^[^:]*/tests/' \
    | grep -vE '/test_support\.rs$' \
    | sort -u
}

raw_violations=''
raw_seen=''
for file in $(production_files_matching "$RAW_DISPATCH"); do
  raw_hits=$(grep -E "$RAW_DISPATCH" "$file" | grep -vcE '^[[:space:]]*(//|///|//!)' || true)
  write_hits=$(grep -E "$WRITE_SQL" "$file" | grep -vcE '^[[:space:]]*(//|///|//!)' || true)
  if [ "${raw_hits:-0}" -eq 0 ] || [ "${write_hits:-0}" -eq 0 ]; then
    continue
  fi
  raw_seen="${raw_seen}${file} "
  if echo "$file" | grep -qE "^($KNOWN_RAW)$"; then
    continue
  fi
  raw_violations="${raw_violations}${file}"$'\n'
done

if [ -n "$raw_violations" ]; then
  echo "ERROR: a write statement is built and dispatched raw, outside the chokepoint (#1327):"
  echo "$raw_violations"
  echo
  echo "Building INSERT/UPDATE/DELETE and handing it to execute_raw_query reaches the"
  echo "database without the Authorizer, requires_role, requires_actor, argument and"
  echo "selection validation, the RLS session variables, the change-log outbox row or the"
  echo "field authorizer — and it looks identical from outside to a write that ran them."
  echo
  echo "Route it through the executor, or — for a bulk load that genuinely is not a"
  echo "compiled mutation — through an adapter method that expresses the transaction,"
  echo "as execute_gated_upload does for Flight uploads (#953)."
  echo
  echo "If a site genuinely cannot, add it to KNOWN_RAW in"
  echo "tools/check-mutation-dispatch-sites.sh with the issue that tracks the gap."
  exit 1
fi

# Non-vacuity for rule 2, checked BEFORE the staleness loop and not after — proven by
# blinding each pattern in turn and watching which branch fires. A renamed pattern makes
# every file fall out at the `continue` above, so `raw_seen` empties and *every* KNOWN_RAW
# entry then looks fixed. Reported in that order the reader is told to delete the entries
# tracking the real bypasses, which would hide them. The cause is the pattern; say so.
#
# ⚠ This was `[ -z "$raw_seen" ]` until #1355, and that test had two causes with opposite
# right answers. Until then the tree always held at least one file matching BOTH halves, so
# "nothing matched the pair" could only mean a broken pattern. #1355 fixed the last one, and
# the goal state is exactly zero files matching the pair — a check keyed on the intersection
# would have gone red for succeeding, and the only way to green it would have been to weaken
# the gate.
#
# So the halves are proven live SEPARATELY. Each matches production code on its own —
# `execute_raw_query` has its legitimate callers (Arrow analytics, tenancy DDL, the
# fact-table cache, the sql_source probe), and write-SQL builders are everywhere — and it is
# only their INTERSECTION that must be empty. A renamed method empties its own half, which
# is the case this still catches, and it catches it before the staleness loop, so a blind
# pattern can never be the reason a KNOWN_RAW entry is pruned.
#
# `|| true` is load-bearing, not defensive noise: a blinded pattern makes `grep -rl` exit 1,
# and under `set -euo pipefail` the failing pipeline aborts the script AT THE ASSIGNMENT —
# so the run exits 1 having printed nothing at all, and the operator is told only that a
# shell gate failed. `wc -l` has already printed its `0` by then, so swallowing the status
# keeps the count and lets the diagnosis below actually reach the reader.
raw_live=$(production_files_matching "$RAW_DISPATCH" | wc -l || true)
write_live=$(production_files_matching "$WRITE_SQL" | wc -l || true)

if [ "$raw_live" -eq 0 ]; then
  echo "ERROR: rule 2's dispatch half matched no production file at all."
  echo "\`execute_raw_query\` was renamed or removed — update RAW_DISPATCH — or this half"
  echo "is now vacuous and rule 2 cannot see a raw write dispatch."
  echo "Do NOT prune KNOWN_RAW on the strength of this run: a blind pattern makes every"
  echo "entry look fixed."
  exit 1
fi

if [ "$write_live" -eq 0 ]; then
  echo "ERROR: rule 2's write-SQL half matched no production file at all."
  echo "The INSERT/UPDATE/DELETE builders were renamed — update WRITE_SQL — or this half"
  echo "is now vacuous and rule 2 cannot tell a write from a read."
  echo "Do NOT prune KNOWN_RAW on the strength of this run: a blind pattern makes every"
  echo "entry look fixed."
  exit 1
fi

# A KNOWN_RAW entry that stopped matching is a fixed gap. Leaving it listed would excuse a
# NEW raw write added to the same file — and would let this gate keep naming a defect that
# no longer exists, which is the other way for its report to be untrue.
for known in $(echo "${KNOWN_RAW:-}" | tr '|' ' '); do
  if [ ! -f "$known" ]; then
    echo "ERROR: KNOWN_RAW entry $known does not exist — remove it from this gate."
    exit 1
  fi
  if ! echo "$raw_seen" | grep -qF "$known "; then
    echo "ERROR: KNOWN_RAW entry $known no longer builds and dispatches write SQL."
    echo "The bypass it tracks is fixed: remove it so the file is gated again."
    exit 1
  fi
done

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

# The report names every tracked bypass. "no known bypasses" is a claim about the tree, and
# it was false for two releases while two of them sat outside this gate's only pattern.
all_known=$(echo "${KNOWN}|${KNOWN_RAW}" | tr '|' ' ' | tr -s ' ' | sed 's/^ //;s/ $//')
if [ -n "$all_known" ]; then
  echo "OK: every mutation dispatch outside the engine chokepoint is tracked."
  echo "    known bypasses: $all_known"
else
  echo "OK: mutations are dispatched only from the engine chokepoint (no known bypasses)"
fi
