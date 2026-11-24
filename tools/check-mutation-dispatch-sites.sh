#!/usr/bin/env bash
# check-mutation-dispatch-sites.sh — no production file both builds a write statement and
# hands it to `execute_raw_query`, so a new transport cannot execute a write past the
# engine's gates (#1327).
#
# WHY THIS GATE EXISTS
# --------------------
# `execute_mutation_impl` (fraiseql-core, runtime/executor/runners/mutation/mod.rs) is
# where every gate on a write is enforced: the operation `Authorizer` (#422),
# `requires_role`, `requires_actor` (#966), selection-set validation (#1005),
# argument-name validation (#1154), the inline-argument merge (#719), the change-log
# write, and the `before:mutation` chain (#1327). Its doc comment calls itself "the
# universal mutation chokepoint", and that claim is only true while nothing reaches the
# database's write path on its own.
#
# There are two ways to. The first is to call the adapter's write, `Writer::execute_write`
# or `execute_writes`, as the gRPC mutation arm did until #1330. That one is banned by
# clippy (`disallowed-methods` in .clippy.toml, #1780): unlike the pattern this gate used
# to match, the lint resolves the call, so an import alias, a `Writer::execute_write(..)`
# spelling or a function pointer is caught as surely as a method call. The chokepoint and
# the cache wrapper below it carry the only production `#[allow]`s.
#
# The second is what this gate matches, because clippy cannot: a file that builds an
# INSERT/UPDATE/DELETE itself and hands it to `execute_raw_query`, a general query method
# with ~20 legitimate production callers (Arrow analytics, tenancy DDL, the fact-table
# cache, the sql_source probe). Banning that method would be 19 allowlist entries and one
# defect, an allowlist nobody reads. So this gate matches the PAIR: a file that both builds
# write SQL and dispatches raw.
#
# That pair is precise in the way that matters: it discriminates a fixed path from an
# unfixed one. `flight_server/handlers/do_exchange.rs` moved off `execute_raw_query` in #953
# so the rows and their change-log outbox rows commit together; it names the method only in
# the comment explaining that, so it does not match. `handlers/do_put.rs` matched until
# #1355, when it made the same move. A gate that cannot tell those two apart would be
# useless here, and the pair is what tells them apart: both files still build write SQL, and
# only the unfixed one also dispatched it raw.
#
# ⚠ Both bypasses this rule was written for were invisible while the gate printed "no
# known bypasses". That sentence is the reason the gate exists; it must not be able to be
# false. Both are now fixed: the saga's local write (#1354), whose orchestrator moved above
# the engine so its local arm calls the chokepoint, and the Flight DoPut upload (#1355),
# which moved onto `execute_gated_upload`. The staleness loop below is what said so each
# time, by refusing to keep listing a file that had stopped matching. KNOWN_RAW is empty,
# which is the state this gate exists to hold.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The pair's two halves. A file matching BOTH constructs a write statement and dispatches it
# outside the chokepoint. Deliberately not keyed on the builder function names alone: a raw
# `"INSERT INTO ..."` literal counts too, so renaming a helper does not slip past.
RAW_DISPATCH='\.execute_raw_query\s*\('
WRITE_SQL='build_(insert|update|delete)_query|"[[:space:]]*(INSERT INTO|UPDATE |DELETE FROM)'

# Files that build write SQL and dispatch it raw. Each is a named defect with an issue, and
# each must keep matching both halves or it is stale — see the staleness loop below.
# Empty since #1355: no production file both builds a write statement and hands it to
# `execute_raw_query`. An entry here is a named defect with an issue, never a resting place.
KNOWN_RAW=''

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
  if echo "$file" | grep -cE "^($KNOWN_RAW)$" >/dev/null; then
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

# Non-vacuity, checked BEFORE the staleness loop and not after — proven by
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
  echo "ERROR: the dispatch half matched no production file at all."
  echo "\`execute_raw_query\` was renamed or removed — update RAW_DISPATCH — or this half"
  echo "is now vacuous and it cannot see a raw write dispatch."
  echo "Do NOT prune KNOWN_RAW on the strength of this run: a blind pattern makes every"
  echo "entry look fixed."
  exit 1
fi

if [ "$write_live" -eq 0 ]; then
  echo "ERROR: the write-SQL half matched no production file at all."
  echo "The INSERT/UPDATE/DELETE builders were renamed — update WRITE_SQL — or this half"
  echo "is now vacuous and it cannot tell a write from a read."
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
  if ! echo "$raw_seen" | grep -cF "$known " >/dev/null; then
    echo "ERROR: KNOWN_RAW entry $known no longer builds and dispatches write SQL."
    echo "The bypass it tracks is fixed: remove it so the file is gated again."
    exit 1
  fi
done

if [ -n "$KNOWN_RAW" ]; then
  echo "OK: every raw write dispatch outside the engine chokepoint is tracked."
  echo "    known bypasses: $(echo "$KNOWN_RAW" | tr '|' ' ')"
else
  echo "OK: no production file builds a write statement and dispatches it raw (no known bypasses)"
fi
