#!/usr/bin/env bash
# ci-stamp-sources.sh — stamp a Rust leg's sources once it holds its target volume (#1596).
#
# Sourced, never run: every Rust leg's container sets BASH_ENV to this file (`rustSource` in
# .dagger/main.go), so bash reads it as each of the leg's execs starts, which is after
# Dagger has mounted the leg's PRIVATE target volume and so after any other leg holding it
# has let go. It touches every file under FRAISEQL_STAMP_ROOT, the target volume excepted,
# with one instant, and exports it as FRAISEQL_SOURCE_TOUCHED_AT: no artifact written
# before the instant, by an earlier run or by another branch's leg on the same volume, can
# be judged fresh by cargo, and tools/ci-target-canary.sh refuses one a build reused.
#
# The stamp used to be taken when the pipeline was built and the sources touched in an exec
# of their own, before the volume was mounted. A leg waiting for the volume then got
# artifacts another branch had built after its stamp: newer than its sources, so fresh to
# cargo, and newer than the stamp, so invisible to the canary.
#
# A nested bash (`bash tools/ci-target-canary.sh`) inherits the exported instant and stamps
# nothing. Files are found with `find`, not `git ls-files`: a leg's /src has no .git.
if [[ -z "${FRAISEQL_SOURCE_TOUCHED_AT:-}" && -n "${FRAISEQL_STAMP_ROOT:-}" ]]; then
    FRAISEQL_SOURCE_TOUCHED_AT="$(date +%s)"
    export FRAISEQL_SOURCE_TOUCHED_AT
    find "${FRAISEQL_STAMP_ROOT}" -mindepth 1 -path "${FRAISEQL_STAMP_ROOT%/}/target" -prune \
        -o -exec touch -h -d "@${FRAISEQL_SOURCE_TOUCHED_AT}" {} +
fi
