#!/usr/bin/env bash
# Run a clippy command, then refuse its output if clippy could not resolve a path that
# .clippy.toml names.
#
# Usage:  tools/clippy-config-paths-gate.sh cargo clippy … -- -D warnings
#
# A `disallowed-methods` or `disallowed-types` entry whose path clippy cannot reach is
# not an error: clippy prints a configuration warning, which `-D warnings` does not
# deny, and drops the ban. The build stays green while the ban stops existing. Clippy
# 1.100 stopped following re-exports, and every ban that named one went silent this way.
# This wrapper turns that warning into a failure, from the output of the clippy run
# that already happens, so no second compile is needed.
#
# The command's own exit status is kept: a clippy failure still fails, with or without
# the warning.
set -uo pipefail

# Clippy's wording for a configured path it cannot resolve, as 1.95 and 1.100 print it.
UNREACHABLE="does not refer to a reachable"

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <clippy command…>" >&2
    exit 2
fi

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

"$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}

if grep -cF -- "$UNREACHABLE" "$log" >/dev/null; then
    echo >&2
    echo "ERROR: clippy could not resolve a path in .clippy.toml, so that ban is not enforced:" >&2
    grep -F -A3 -- "$UNREACHABLE" "$log" >&2
    echo "Name each item where it is defined, never through a re-export." >&2
    exit 1
fi

exit "$status"
