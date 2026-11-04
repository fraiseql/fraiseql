#!/usr/bin/env bash
# sccache_bootstrap_test.sh — tools/ci-stamp-sources.sh starts sccache's server before cargo
# does, compiles uncached and says so when no server answers, and prints the server's error
# log when an exec fails (#1914).
#
# Runs in the rustBase image (`dagger call sccache-bootstrap`), where the leg's sccache is. A
# missing sccache fails it rather than skipping it: without the binary it proves nothing.
#
# Whether a server is listening is probed with a bare TCP connect, never an sccache command,
# which would start a server itself and pass whether or not the bootstrap did.
set -uo pipefail
cd "$(dirname "$0")/../.."

fail=0
ok() { echo "  ok: $*"; }
bad() {
    echo "  FAIL: $*"
    fail=1
}

if ! command -v sccache >/dev/null; then
    echo "FAIL: sccache is not on PATH; this test needs the leg's real sccache"
    exit 1
fi

# A port and cache of its own, so no other sccache server is reached or disturbed.
export SCCACHE_SERVER_PORT=42617
SCCACHE_DIR="$(mktemp -d)"
export SCCACHE_DIR
scratch="$(mktemp -d)"
trap 'sccache --stop-server >/dev/null 2>&1; rm -rf "$SCCACHE_DIR" "$scratch"' EXIT

answers() { (exec 3<>"/dev/tcp/127.0.0.1/${SCCACHE_SERVER_PORT}") 2>/dev/null; }

# The pid of this test's sccache server: the sccache process started for its port. Read from
# /proc (procps may not be installed), so other sccache processes on the host do not count.
server_pid() {
    local p
    for p in /proc/[0-9]*; do
        [[ "$(cat "$p/comm" 2>/dev/null)" == sccache ]] || continue
        { tr '\0' '\n' <"$p/environ"; } 2>/dev/null |
            grep -qx "SCCACHE_SERVER_PORT=${SCCACHE_SERVER_PORT}" && echo "${p#/proc/}"
    done
}

# Source the script in a fresh bash, as a leg's exec does, and print what it left behind.
exec_with() {
    env -u FRAISEQL_SCCACHE_READY -u FRAISEQL_SCCACHE_LOG_TRAP -u FRAISEQL_STAMP_ROOT \
        -u FRAISEQL_SOURCE_TOUCHED_AT -u SCCACHE_ERROR_LOG "$@" bash -c \
        'source tools/ci-stamp-sources.sh
echo "wrapper=${RUSTC_WRAPPER:-}"
echo "ready=${FRAISEQL_SCCACHE_READY:-}"' 2>&1
}

echo "a fresh exec starts the server before cargo does"
sccache --stop-server >/dev/null 2>&1
if answers; then
    bad "a server was answering before the exec started"
fi
out="$(exec_with RUSTC_WRAPPER=sccache)"
if answers; then ok "a server answers after the exec's shell started"; else bad "no server answers: $out"; fi
[[ "$out" == *"wrapper=sccache"* ]] && ok "rustc still goes through sccache" || bad "$out"
[[ "$out" == *"ready=1"* ]] && ok "the outcome is exported" || bad "$out"

echo "the probe leaves the server serving"
before="$(server_pid)"
if answers && answers; then ok "it answers probe after probe"; else bad "a probe stopped it"; fi
if sccache --show-stats >/dev/null 2>&1; then ok "it serves a real request"; else bad "--show-stats failed"; fi
after="$(server_pid)"
[[ -n "$before" && "$before" == "$after" ]] && ok "the same server, not a new one ($before)" ||
    bad "the server changed: '$before' -> '$after'"

echo "a nested bash inherits the outcome and starts nothing"
sccache --stop-server >/dev/null 2>&1
out="$(exec_with RUSTC_WRAPPER=sccache FRAISEQL_SCCACHE_READY=1)"
if answers; then bad "a nested bash started a server: $out"; else ok "no server started"; fi

echo "with no server to be had, the exec compiles uncached and says so"
cat >"$scratch/sccache" <<'FAKE'
#!/usr/bin/env bash
exit 1
FAKE
chmod +x "$scratch/sccache"
out="$(exec_with RUSTC_WRAPPER=sccache PATH="$scratch:$PATH")"
[[ "$out" == *"::warning::sccache"* ]] && ok "it warns, naming sccache" || bad "no warning: $out"
[[ "$out" == *"wrapper="$'\n'* ]] && ok "RUSTC_WRAPPER is unset" || bad "the wrapper was kept: $out"
[[ "$out" == *"ready=uncached"* ]] && ok "the outcome is exported" || bad "$out"

echo "a failing exec prints the server's error log; a passing one does not"
printf 'sccache: the server stopped\n' >"$scratch/error.log"
out="$(env -u FRAISEQL_SCCACHE_LOG_TRAP SCCACHE_ERROR_LOG="$scratch/error.log" \
    bash -c 'source tools/ci-stamp-sources.sh; exit 3' 2>&1)"
rc=$?
[[ "$out" == *"the server stopped"* ]] && ok "the log is printed" || bad "not printed: $out"
[[ $rc -eq 3 ]] && ok "the exec's status is kept" || bad "status $rc, not 3"
out="$(env -u FRAISEQL_SCCACHE_LOG_TRAP SCCACHE_ERROR_LOG="$scratch/error.log" \
    bash -c 'source tools/ci-stamp-sources.sh; exit 0' 2>&1)"
[[ "$out" != *"the server stopped"* ]] && ok "a passing exec stays quiet" || bad "$out"

if [[ $fail -ne 0 ]]; then
    echo "FAILED"
    exit 1
fi
echo "PASSED"
