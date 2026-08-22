#!/usr/bin/env bash
# check-crate-sizes.sh — Fail if any workspace crate exceeds its line-count budget,
# or has no budget at all.
#
# Budget values are read from [workspace.metadata.crate-size-budget] in the
# root Cargo.toml. Lines are counted across all *.rs files under each crate's
# src/ directory — which includes unit-test modules (`src/**/tests.rs`), so the
# number is the size of the crate's source tree, not of its production code alone.
#
# ⚠ THIS GATE RAN NOWHERE UNTIL #1055/#990. Its only caller was `tools/lint.sh`, a
# harness with no Makefile target, no Dagger function and no workflow — while
# Cargo.toml claimed the budget was "enforced by tools/check-crate-sizes.sh in CI".
# Unrun, it drifted into being useless in both directions at once:
#
#   - four crates were OVER budget, one of them by 91% (fraiseql-server, 105420 vs
#     55000), so running it would have failed on clean trunk; and
#   - five crates (codegen, federation, functions, storage, fraiseql) had no budget
#     row at all and were silently skipped — including three above 16k lines.
#
# Both are fixed: budgets are re-baselined at today's sizes plus room, and a crate
# with a `src/` directory and no budget row is now a FAILURE rather than a skip, so
# the next crate cannot arrive unmeasured.
#
# What this gate is, stated honestly: a runaway-growth ratchet, not a split mandate.
# The original comment said crates over budget "must be split before merging", which
# nothing has ever done. Raising a budget is a one-line edit with a comment saying
# why — the same shape as the async-trait and errors-doc ratchets in the Makefile.
#
# Usage:
#   tools/check-crate-sizes.sh              # check all crates
#   tools/check-crate-sizes.sh fraiseql-core # check a single crate
#
# Exit code: 0 if all crates are within budget, 1 if any crate is over or unbudgeted.
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
        echo "  → Split the crate, or raise its budget in [workspace.metadata.crate-size-budget]"
        echo "    with a comment saying why the growth is expected."
        failures=$((failures + 1))
    elif [[ "$lines" -gt $((budget * 85 / 100)) ]]; then
        # Warn at 85% of budget
        printf "%-30s %10d %10d %10s\n" "$name" "$lines" "$budget" "⚠ WARNING"
        echo "  → $name is at $(( lines * 100 / budget ))% of its budget"
    else
        printf "%-30s %10d %10d %10s\n" "$name" "$lines" "$budget" "✅ OK"
    fi
done < <(parse_budgets)

# Fail-closed the other way: a crate on disk with no budget row is not "fine", it is
# unmeasured. Five crates sat in that state while the gate was unrun, three of them
# over 16k lines. Skipping them silently is how the table stopped describing the
# workspace (#1055).
unbudgeted=()
if [[ -z "$filter_crate" ]]; then
    budgeted="$(parse_budgets | sed 's/=.*//')"
    for dir in "$REPO_ROOT"/crates/*/; do
        name="$(basename "$dir")"
        [[ -d "$dir/src" ]] || continue
        if ! printf '%s\n' "$budgeted" | grep -qxF "$name"; then
            unbudgeted+=("$name")
        fi
    done
fi

echo ""
if [[ "$checked" -eq 0 ]]; then
    echo "No budgets found in Cargo.toml [workspace.metadata.crate-size-budget]"
    exit 1
fi

if [[ "${#unbudgeted[@]}" -gt 0 ]]; then
    echo "❌ ${#unbudgeted[@]} crate(s) have a src/ directory but no size budget:"
    for name in "${unbudgeted[@]}"; do
        lines="$(count_lines "$REPO_ROOT/crates/$name/src")"
        printf "     %-28s %s lines\n" "$name" "${lines:-0}"
    done
    echo "   Add a row to [workspace.metadata.crate-size-budget] in Cargo.toml."
    failures=$((failures + ${#unbudgeted[@]}))
fi

if [[ "$failures" -gt 0 ]]; then
    echo ""
    echo "❌ $failures crate size problem(s)."
    echo "   To update a budget, edit [workspace.metadata.crate-size-budget] in Cargo.toml"
    echo "   and add a comment explaining why the increase is justified."
    exit 1
fi

echo "✅ All $checked crate(s) are within budget, and every crate has one."
