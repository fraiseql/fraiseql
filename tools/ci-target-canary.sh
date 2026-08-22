#!/usr/bin/env bash
# ci-target-canary.sh — the #880 stale-target-cache canary.
#
# The CI legs mount a PERSISTENT target/ cache volume and let cargo judge
# freshness by mtime. Across a Dagger mount that judgement can be wrong: the leg
# has (three times) run test binaries linked from artifacts built from DIFFERENT
# source than the commit under test — twice loudly, once silently validating
# pre-fix code. This canary converts the whole class from silent to loud, and
# self-heals:
#
#   1. Digest the compilable source (crates/, sql/, Cargo.toml, Cargo.lock).
#   2. Run the leg's first build (`cargo <args> --message-format=json`) and
#      count freshly compiled units ("fresh":false compiler-artifact messages).
#   3. If the digest CHANGED since the marker in the target volume but cargo
#      compiled ZERO units, the cache is stale: purge target/debug, rebuild,
#      and fail loudly if it still builds nothing.
#   4. Record the digest marker and print the diagnostic (fresh-unit count +
#      digest-changed flag) so no one needs the `gh run view | grep -c`
#      incantation again.
#
# Usage:  bash tools/ci-target-canary.sh -- test -p crate --features 'x,y'
#         bash tools/ci-target-canary.sh -- build --all-features
# (`test` invocations get --no-run appended: the canary builds, the leg runs.)
#
# FRAISEQL_CANARY_NO_HEAL=1 skips the self-heal purge and exits 1 on detection
# (used by the red-capability proof; CI uses the self-healing default).

set -euo pipefail

if [[ "${1:-}" != "--" ]]; then
    echo "usage: ci-target-canary.sh -- <cargo args…>" >&2
    exit 2
fi
shift

# One marker PER INVOCATION, not per target volume (#1132).
#
# The marker was a single path inside the mounted `target/` cache, and a Dagger leg
# mounts one volume for many suites — `integrationBase` mounts
# `fraiseql-rust-target-integ4-<toolchain>` and FOURTEEN canary-wrapped suites run
# against it. Whichever ran first wrote the current digest; the other thirteen read it
# back equal and printed `source digest changed: no` about a run where the source
# certainly had changed. Since detection requires `changed == yes && fresh == 0`, the
# canary was structurally incapable of detecting a stale cache in any suite but the
# first one per volume, per run — and its diagnostic line actively said the opposite.
#
# Keying on the cargo argument vector makes each suite compare against its own last
# build, which is the question the canary is actually asking. sha256 of the NUL-joined
# args, so `--features a,b` and `--features a` cannot collide.
ARGS_KEY="$(printf '%s\0' "$@" | sha256sum | cut -c1-16)"
MARKER="target/.fraiseql-ci-src-digest-${ARGS_KEY}"
BUILD_LOG="$(mktemp)"
trap 'rm -f "${BUILD_LOG}"' EXIT

digest_sources() {
    local roots=()
    local p
    for p in crates sql Cargo.toml Cargo.lock; do
        [[ -e "${p}" ]] && roots+=("${p}")
    done
    find "${roots[@]}" -type f \
        \( -name '*.rs' -o -name '*.toml' -o -name '*.sql' -o -name '*.json' -o -name '*.lock' \) \
        -not -path '*/target/*' -print0 \
        | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1
}

run_build() {
    local extra=()
    if [[ "$1" == "test" ]]; then
        extra=(--no-run)
    fi
    cargo "$@" "${extra[@]}" --message-format=json-render-diagnostics >"${BUILD_LOG}"
}

count_fresh_units() {
    # compiler-artifact messages carry "fresh":true (cached) / false (compiled).
    grep -c '"fresh":false' "${BUILD_LOG}" || true
}

cur="$(digest_sources)"
prev="$(cat "${MARKER}" 2>/dev/null || echo none)"
changed="no"
[[ "${cur}" != "${prev}" ]] && changed="yes"

run_build "$@"
fresh="$(count_fresh_units)"

if [[ "${changed}" == "yes" && "${fresh}" -eq 0 ]]; then
    echo "#880 CANARY: STALE TARGET CACHE DETECTED — source digest changed" \
         "(${prev} -> ${cur}) but cargo compiled 0 units. The mounted target/" \
         "volume is serving artifacts built from different source than this commit."
    if [[ "${FRAISEQL_CANARY_NO_HEAL:-0}" == "1" ]]; then
        echo "#880 CANARY: FRAISEQL_CANARY_NO_HEAL=1 — failing without self-heal."
        exit 1
    fi
    echo "#880 CANARY: self-healing — purging target/debug and rebuilding."
    rm -rf target/debug
    run_build "$@"
    fresh="$(count_fresh_units)"
    if [[ "${fresh}" -eq 0 ]]; then
        echo "#880 CANARY: rebuild after purge STILL compiled 0 units — refusing to test."
        exit 1
    fi
fi

echo "${cur}" >"${MARKER}"
echo "#880 canary OK: fresh-built units: ${fresh}; source digest changed since this" \
     "suite's last run: ${changed} (marker ${ARGS_KEY})"
