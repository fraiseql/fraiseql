#!/usr/bin/env bash
# Shared helper for the pre-tag release dry-run gate.
#
# Sourced by .dagger/release.go (PublishDryRun, in the mounted /src) and by
# tools/tests/dry_run_tolerance_test.sh. Pure: it only reads the log file given.

# Decide whether a `cargo publish --dry-run` failure is tolerable for a pre-tag,
# pre-publish gate. A dry-run can fail purely because a sibling crate this one
# depends on is not on crates.io at the required version yet — the first publish
# of a new crate (e.g. fraiseql-codegen), or a synchronized version bump where
# the new sibling versions are published later in the same run (the floor-bump
# case). cargo phrases this two ways:
#   "no matching package named `X` found"                   (X never published)
#   "failed to select a version for the requirement `X = …`" (X published, not at the version)
# Tolerate either ONLY when every unresolved package is one of our own
# publishable crates AND the failure is a packaging-prep failure, not a compile
# error in the verify build. A genuinely missing external dep, a real compile
# error, or any other failure still hard-fails.
#
# Prints the unresolved sibling names on tolerable (rc 0); prints nothing and
# returns 1 otherwise. Written to be safe under `set -e` and `set -o pipefail`.
#
# Usage: dry_run_failure_is_tolerable <log-file> <space-separated-publishable-crates>
dry_run_failure_is_tolerable() {
    local log="$1" crates="$2" missing m
    grep -q "failed to prepare local package for uploading" "$log" || return 1
    if grep -qE "could not compile|error\[E[0-9]|aborting due to" "$log"; then
        return 1
    fi
    missing=$(
        {
            grep -oE "no matching package named \`[a-zA-Z0-9_-]+\`" "$log" || true
            grep -oE "failed to select a version for the requirement \`[a-zA-Z0-9_-]+" "$log" || true
        } | sed -E "s/.*\`([a-zA-Z0-9_-]+).*/\1/" | sort -u
    )
    [ -n "$missing" ] || return 1
    for m in $missing; do
        case " $crates " in
            *" $m "*) ;;       # one of our publishable crates → first-publish ordering, ok
            *) return 1 ;;     # an external dependency is genuinely unresolved → fail
        esac
    done
    printf '%s\n' "$missing"
    return 0
}
