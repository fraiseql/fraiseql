#!/usr/bin/env bash
# check-snapshot-pairing.sh — Enforce the SQL snapshot pairing policy.
#
# Every .snap file in crates/fraiseql-core/tests/snapshots/ must be registered
# in tests/snapshot-pairs.md with a non-empty status column.
#
# Exit codes:
#   0  All snapshots registered
#   1  One or more snapshots are unregistered
#
# Usage:
#   ./tools/check-snapshot-pairing.sh            # from repo root
#   ./tools/check-snapshot-pairing.sh --verbose  # print registered snapshots too

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SNAPSHOTS_DIR="${REPO_ROOT}/crates/fraiseql-core/tests/snapshots"
REGISTRY="${REPO_ROOT}/tests/snapshot-pairs.md"
VERBOSE="${1:-}"

if [[ ! -d "${SNAPSHOTS_DIR}" ]]; then
    echo "ERROR: snapshots directory not found: ${SNAPSHOTS_DIR}" >&2
    exit 1
fi

if [[ ! -f "${REGISTRY}" ]]; then
    echo "ERROR: snapshot registry not found: ${REGISTRY}" >&2
    echo "Create tests/snapshot-pairs.md and register every snapshot." >&2
    exit 1
fi

unregistered=()
registered=()

for snap_path in "${SNAPSHOTS_DIR}"/*.snap; do
    [[ -e "${snap_path}" ]] || continue  # no matches

    # Derive short name: strip directory, strip "sql_snapshots__" prefix, strip ".snap" suffix.
    snap_file="$(basename "${snap_path}" .snap)"
    short="${snap_file#sql_snapshots__}"

    # Check that the registry contains this short name followed by a | and a non-empty status.
    if grep -qP "^\|\s*\`?${short}\`?\s*\|" "${REGISTRY}"; then
        registered+=("${short}")
    else
        unregistered+=("${short}")
    fi
done

if [[ "${VERBOSE}" == "--verbose" ]]; then
    echo "Registered snapshots (${#registered[@]}):"
    for s in "${registered[@]}"; do
        echo "  ✓ ${s}"
    done
    echo ""
fi

if [[ ${#unregistered[@]} -gt 0 ]]; then
    echo "FAIL: ${#unregistered[@]} snapshot(s) not registered in tests/snapshot-pairs.md:"
    for s in "${unregistered[@]}"; do
        echo "  ✗ ${s}"
    done
    echo ""
    echo "Add each missing snapshot to tests/snapshot-pairs.md with an appropriate"
    echo "status (generator | behavioral | db-integration | cross-db-parity | doc-only)."
    echo "See docs/testing.md for the full policy."
    exit 1
fi

echo "OK: all ${#registered[@]} snapshots are registered."
exit 0
