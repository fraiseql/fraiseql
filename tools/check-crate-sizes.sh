#!/usr/bin/env bash
# check-crate-sizes.sh — Fail if any workspace crate exceeds its line-count budget.
#
# Budget values are read from [workspace.metadata.crate-size-budget] in the
# root Cargo.toml. Lines are counted across all *.rs files under each crate's
# src/ directory.
#
# Usage:
#   tools/check-crate-sizes.sh              # check all crates
#   tools/check-crate-sizes.sh fraiseql-core # check a single crate
#
# Exit code: 0 if all crates are within budget, 1 if any crate is over.
#
# Requires: bash 4+, awk, wc, grep

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$REPO_ROOT/Cargo.toml"

# ---------------------------------------------------------------------------
# Parse [workspace.metadata.crate-size-budget] from Cargo.toml
# Returns lines of the form: crate-name=budget
# ---------------------------------------------------------------------------
parse_budgets() {
    awk '
        /^\[workspace\.metadata\.crate-size-budget\]/ { in_section=1; next }
        in_section && /^\[/ { in_section=0 }
        in_section && /^[a-z]/ {
            # Strip underscores from numbers (TOML numeric separators)
            gsub(/_/, "", $0)
            # Remove inline comments
            sub(/#.*/, "", $0)
            # Trim whitespace
            gsub(/[ \t]/, "", $0)
            print $0
        }
    ' "$CARGO_TOML"
}

# ---------------------------------------------------------------------------
# Count source lines for a crate
# ---------------------------------------------------------------------------
count_lines() {
    local src_dir="$1"
    find "$src_dir" -name "*.rs" -print0 2>/dev/null \
        | xargs -0 wc -l 2>/dev/null \
        | tail -1 \
        | awk '{print $1}'
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
filter_crate="${1:-}"
failures=0
checked=0

echo "FraiseQL Crate Size Check"
echo "========================="
printf "%-30s %10s %10s %10s\n" "Crate" "Lines" "Budget" "Status"
printf "%-30s %10s %10s %10s\n" "-----" "-----" "------" "------"

while IFS='=' read -r name budget; do
    [[ -z "$name" || -z "$budget" ]] && continue

    # If a specific crate was requested, skip others
    if [[ -n "$filter_crate" && "$name" != "$filter_crate" ]]; then
        continue
    fi

    src_dir="$REPO_ROOT/crates/$name/src"
    if [[ ! -d "$src_dir" ]]; then
        printf "%-30s %10s %10s %10s\n" "$name" "N/A" "$budget" "SKIP (no src/)"
        continue
    fi

    lines=$(count_lines "$src_dir")
    lines="${lines:-0}"
    checked=$((checked + 1))

    if [[ "$lines" -gt "$budget" ]]; then
        printf "%-30s %10d %10d %10s\n" "$name" "$lines" "$budget" "❌ OVER"
        echo "  → $name exceeds budget by $((lines - budget)) lines ($lines > $budget)"
        echo "  → Consider splitting this crate. See .remediation_2/batches/batch-5-crate-split.md"
        failures=$((failures + 1))
    elif [[ "$lines" -gt $((budget * 85 / 100)) ]]; then
        # Warn at 85% of budget
        printf "%-30s %10d %10d %10s\n" "$name" "$lines" "$budget" "⚠ WARNING"
        echo "  → $name is at $(( lines * 100 / budget ))% of its budget"
    else
        printf "%-30s %10d %10d %10s\n" "$name" "$lines" "$budget" "✅ OK"
    fi
done < <(parse_budgets)

echo ""
if [[ "$checked" -eq 0 ]]; then
    echo "No budgets found in Cargo.toml [workspace.metadata.crate-size-budget]"
    exit 1
fi

if [[ "$failures" -gt 0 ]]; then
    echo "❌ $failures crate(s) exceed their size budget."
    echo "   To update a budget, edit [workspace.metadata.crate-size-budget] in Cargo.toml"
    echo "   and add a comment explaining why the increase is justified."
    exit 1
fi

echo "✅ All $checked crate(s) are within budget."
