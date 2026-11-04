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
#
# It also starts the leg's sccache server before cargo does, and prints that server's error
# log when an exec fails (#1914); see below.
if [[ -z "${FRAISEQL_SOURCE_TOUCHED_AT:-}" && -n "${FRAISEQL_STAMP_ROOT:-}" ]]; then
    FRAISEQL_SOURCE_TOUCHED_AT="$(date +%s)"
    export FRAISEQL_SOURCE_TOUCHED_AT
    find "${FRAISEQL_STAMP_ROOT}" -mindepth 1 -path "${FRAISEQL_STAMP_ROOT%/}/target" -prune \
        -o -exec touch -h -d "@${FRAISEQL_SOURCE_TOUCHED_AT}" {} +
fi

# sccache's server is started here, once, by this one shell, before cargo runs rustc in
# parallel (#1914). Left to cargo, a fresh container's first parallel rustc calls each
# start a server: one is told it is listening, the others are told the port is taken, and
# all of them then only retry the connection for 5 s. If that server dies in the meantime,
# no client starts another, and every one of them fails the crate as "could not compile".
# Here a server that is not answering is started again, up to five times.
#
# The probe is a bare TCP connect, not an sccache command: `sccache --show-stats` starts a
# server itself, so it cannot tell whether one was already listening.
#
# A broken cache must not fail a correctness leg, and must not go unnoticed either: when
# no server answers, this exec says so and compiles without the cache. A nested bash
# inherits the outcome and does not try again.
if [[ "${RUSTC_WRAPPER:-}" == *sccache && -z "${FRAISEQL_SCCACHE_READY:-}" ]]; then
    fraiseql_sccache_answers() {
        (exec 3<>"/dev/tcp/127.0.0.1/${SCCACHE_SERVER_PORT:-4226}") 2>/dev/null
    }
    for _ in 1 2 3 4 5; do
        fraiseql_sccache_answers && break
        "${RUSTC_WRAPPER}" --start-server >/dev/null 2>&1 || true
        fraiseql_sccache_answers && break
        sleep 1
    done
    if fraiseql_sccache_answers; then
        FRAISEQL_SCCACHE_READY=1
    else
        echo "::warning::sccache: no server answered on 127.0.0.1:${SCCACHE_SERVER_PORT:-4226} after 5 starts; this exec compiles without the cache" >&2
        if [[ -s "${SCCACHE_ERROR_LOG:-}" ]]; then
            cat "${SCCACHE_ERROR_LOG}" >&2
        fi
        unset RUSTC_WRAPPER
        FRAISEQL_SCCACHE_READY=uncached
    fi
    export FRAISEQL_SCCACHE_READY
    unset -f fraiseql_sccache_answers
fi

# The sccache server writes its errors to SCCACHE_ERROR_LOG (set in rustBase). An exec that
# fails with something in that log prints it, so a server that died says why. Set by the
# exec's own shell only; a script that sets its own EXIT trap replaces this one.
if [[ -n "${SCCACHE_ERROR_LOG:-}" && -z "${FRAISEQL_SCCACHE_LOG_TRAP:-}" ]]; then
    export FRAISEQL_SCCACHE_LOG_TRAP=1
    trap 'fraiseql_rc=$?
if [[ $fraiseql_rc -ne 0 && -s "${SCCACHE_ERROR_LOG}" ]]; then
    echo "::group::sccache server log (${SCCACHE_ERROR_LOG})" >&2
    cat "${SCCACHE_ERROR_LOG}" >&2
    echo "::endgroup::" >&2
fi
exit $fraiseql_rc' EXIT
fi
