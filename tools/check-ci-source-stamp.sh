#!/usr/bin/env bash
# check-ci-source-stamp.sh — a Rust leg stamps its sources only once it holds its target
# volume (#1596).
#
# The stamp that makes cargo rebuild what another run built is taken by
# tools/ci-stamp-sources.sh, which bash sources at the start of every exec of a Rust leg
# (BASH_ENV, set by `rustSource`). Three ways to lose that, each refused:
#
#   1. `rustSource` runs an exec of its own, or stops setting BASH_ENV/FRAISEQL_STAMP_ROOT:
#      a stamp taken before the volume is mounted is the defect itself.
#   2. A Go value is exported as FRAISEQL_SOURCE_TOUCHED_AT: an instant taken when the
#      pipeline is built, not when the leg holds the volume.
#   3. A function that builds on a Rust leg's container (`rustSrc`, `rustSource`,
#      `integrationBase`) runs `cargo` directly rather than through bash: BASH_ENV is read
#      by bash only, so that exec would run on an unstamped tree.
#
# Usage: check-ci-source-stamp.sh [dagger-dir]   (default .dagger)
set -euo pipefail

dir="${1:-.dagger}"
fail=0
complain() { echo "check-ci-source-stamp: $1" >&2; fail=1; }

main="${dir}/main.go"
[[ -f "${main}" ]] || { echo "check-ci-source-stamp: no ${main}" >&2; exit 2; }

# 1. rustSource's body: from its `func` line to the next top-level `}`.
body="$(awk '/^func rustSource\(/{f=1} f{print} f&&/^}/{exit}' "${main}")"
[[ -n "${body}" ]] || complain "no func rustSource in ${main}"
grep -q 'WithExec(' <<<"${body}" && complain "rustSource runs an exec before a leg holds its volume"
grep -q '"BASH_ENV", "/src/tools/ci-stamp-sources.sh"' <<<"${body}" \
    || complain "rustSource does not set BASH_ENV to /src/tools/ci-stamp-sources.sh"
grep -q '"FRAISEQL_STAMP_ROOT", "/src"' <<<"${body}" \
    || complain "rustSource does not set FRAISEQL_STAMP_ROOT to /src"

# 2. No Go-side stamp, anywhere.
if grep -n '"FRAISEQL_SOURCE_TOUCHED_AT"' "${dir}"/*.go; then
    complain "FRAISEQL_SOURCE_TOUCHED_AT is set from Go; only ci-stamp-sources.sh may set it"
fi

# 3. Per function: one that builds on a Rust leg's container runs no direct `cargo` exec,
#    on one line (`WithExec([]string{"cargo", …`) or over two (`WithExec([]string{` then
#    `"cargo", …`).
for go in "${dir}"/*.go; do
    awk -v file="${go}" '
        /^func / { fn = $0; uses = 0; prev = "" }
        /(rustSrc|rustSource|integrationBase)\(/ && !/^func / { uses = 1; bad_fn[fn] = bad_fn[fn] }
        /WithExec\(\[\]string\{"cargo"/ || (prev ~ /WithExec\(\[\]string\{[[:space:]]*$/ && /^[[:space:]]*"cargo",/) {
            cargo[fn] = cargo[fn] FNR " "
        }
        { if (uses) leg[fn] = 1; prev = $0 }
        END {
            for (f in cargo) if (f in leg) {
                printf "check-ci-source-stamp: %s: %s runs cargo outside bash at line(s) %s\n", file, f, cargo[f] > "/dev/stderr"
                bad = 1
            }
            exit bad
        }' "${go}" || fail=1
done

if [[ "${fail}" -eq 0 ]]; then
    echo "check-ci-source-stamp: OK — Rust legs stamp their sources inside each exec, after the mount"
fi
exit "${fail}"
