#!/usr/bin/env bash
# check-feature-chains.sh — Detect ghost features in workspace Cargo.toml files.
#
# A "ghost feature" is declared as `name = []` (empty deps) in a crate's
# [features] section but has zero `#[cfg(feature = "name")]` usage in any
# Rust source file across the workspace.
#
# Some empty features are intentional markers (e.g. `postgres = []` gates code
# in downstream crates). These are listed in the ALLOWLIST below.
#
# Usage:
#   tools/check-feature-chains.sh
#
# Exit code: 0 if clean, 1 if ghost features found.
#
# Requires: bash 4+, awk, grep

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ---------------------------------------------------------------------------
# Intentional empty marker features (not ghosts).
# These are declared as `name = []` but gate code via downstream cfg checks
# or serve as opt-in markers for test/bench infrastructure.
# ---------------------------------------------------------------------------
ALLOWLIST=(
    # Database backend markers — gate code in fraiseql-db, fraiseql-core, etc.
    "postgres"
    "test-postgres"
    "test-mysql"
    "test-sqlserver"
    # Transport markers — gate code in downstream crates
    "grpc"
    # Test/bench infrastructure — gate code via cfg(any(test, feature = "..."))
    "testing"
    "test-utils"
    # Benchmark infrastructure
    "bench-with-postgres"
    "bench-with-tokio-postgres"
    # Unstable API marker
    "unstable"
    # Schema linting — gates modules in fraiseql-core
    "schema-lint"
    # Observer subsystem markers
    "checkpoint"
    # Wire backend marker in fraiseql-arrow
    "wire-backend"
    # CORS — always-on middleware; feature is a downstream marker
    "cors"
    # Database — marker for DB dependency inclusion
    "database"
)

is_allowlisted() {
    local name="$1"
    for allowed in "${ALLOWLIST[@]}"; do
        [[ "$name" == "$allowed" ]] && return 0
    done
    return 1
}

# ---------------------------------------------------------------------------
# Extract empty features (`name = []`) from a Cargo.toml [features] section.
# Outputs one feature name per line.
# ---------------------------------------------------------------------------
extract_empty_features() {
    local toml="$1"
    awk '
        /^\[features\]/ { in_section=1; next }
        in_section && /^\[/ { in_section=0 }
        in_section && /= *\[\]/ {
            # Extract feature name before the =
            sub(/[ \t]*=.*/, "", $0)
            gsub(/[ \t]/, "", $0)
            # Skip comments and the special "default" / "minimal" / "full" Cargo features
            if ($0 != "" && $0 !~ /^#/ && $0 != "default" && $0 != "minimal" && $0 != "full") print $0
        }
    ' "$toml"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
echo "FraiseQL Feature Chain Check"
echo "============================"

ghosts=0
checked=0

for toml in "$REPO_ROOT"/crates/*/Cargo.toml; do
    crate_dir="$(dirname "$toml")"
    crate_name="$(basename "$crate_dir")"

    while IFS= read -r feature; do
        [[ -z "$feature" ]] && continue
        checked=$((checked + 1))

        # Skip allowlisted features
        if is_allowlisted "$feature"; then
            continue
        fi

        # Search for cfg(feature = "name") across ALL workspace Rust sources
        pattern="cfg.*feature *= *\"${feature}\""
        if ! grep -rq --include='*.rs' "$pattern" "$REPO_ROOT/crates/"; then
            echo "GHOST: $crate_name declares '$feature = []' but no cfg usage found"
            ghosts=$((ghosts + 1))
        fi
    done < <(extract_empty_features "$toml")
done

echo ""
if [[ "$checked" -eq 0 ]]; then
    echo "No empty features found in workspace."
    exit 0
fi

if [[ "$ghosts" -gt 0 ]]; then
    echo "ERROR: Found $ghosts ghost feature(s)."
    echo "  Either delete the feature or add it to the ALLOWLIST in this script"
    echo "  with a comment explaining why it must remain."
    exit 1
fi

echo "OK: $checked empty features checked, all are used or allowlisted."
