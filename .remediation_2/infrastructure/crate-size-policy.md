# Crate Size Policy

## Problem

`fraiseql-core` reached 209,277 lines before a split was planned. At that
size, incremental compilation is severely degraded — a one-line change in
`db/mysql/adapter.rs` forces recompilation of all of `graphql/`, `cache/`,
`security/`, etc.

This policy prevents any single crate from reaching that state again.

---

## Size Budgets

| Crate | Budget | Action if exceeded |
|-------|--------|--------------------|
| `fraiseql-core` | 150,000 lines | Required split review |
| `fraiseql-server` | 80,000 lines | Required split review |
| `fraiseql-auth` | 40,000 lines | Required split review |
| `fraiseql-observers` | 40,000 lines | Required split review |
| All other crates | 30,000 lines | Required split review |

"Required split review" means: before a PR that would exceed the budget merges,
an architectural review must happen to identify which module should be extracted
into a new crate.

---

## Enforcement Script

**File**: `tools/check-crate-sizes.sh`

```bash
#!/usr/bin/env bash
# check-crate-sizes.sh — Fail if any crate exceeds its size budget.
# Called from CI. Exit code 1 if any budget is exceeded.

set -euo pipefail

FAILED=0

check_crate() {
    local crate_path="$1"
    local budget="$2"
    local crate_name
    crate_name=$(basename "$crate_path")

    local count
    count=$(find "$crate_path/src" -name "*.rs" -exec wc -l {} + 2>/dev/null \
        | tail -1 | awk '{print $1}')

    if [[ -z "$count" || "$count" == "0" ]]; then
        return  # crate has no src/ yet
    fi

    if (( count > budget )); then
        echo "FAIL: $crate_name has $count lines (budget: $budget)"
        echo "      A crate split review is required before this PR can merge."
        echo "      See .remediation_2/infrastructure/crate-size-policy.md"
        FAILED=1
    else
        echo "OK:   $crate_name: $count / $budget lines"
    fi
}

check_crate "crates/fraiseql-core"      150000
check_crate "crates/fraiseql-server"     80000
check_crate "crates/fraiseql-auth"       40000
check_crate "crates/fraiseql-observers"  40000
check_crate "crates/fraiseql-cli"        30000
check_crate "crates/fraiseql-secrets"    30000
check_crate "crates/fraiseql-webhooks"   30000
check_crate "crates/fraiseql-arrow"      30000
check_crate "crates/fraiseql-wire"       30000
check_crate "crates/fraiseql-error"      30000

exit $FAILED
```

Make executable: `chmod +x tools/check-crate-sizes.sh`

---

## CI Integration

Add to `ci.yml` under the `check` job:

```yaml
- name: Check crate size budgets
  run: bash tools/check-crate-sizes.sh
```

This runs on every PR. It is advisory for now (non-blocking) until the
`fraiseql-core` split (Batch 5) brings the core crate under 150K. After
the split, make it blocking.

---

## Split Review Process

When a crate approaches its budget (within 10%):

1. Open a GitHub issue labelled `crate-split-review`
2. The issue must document:
   - Which modules are the largest (from `wc -l`)
   - Which modules have no dependencies on each other (safe to extract)
   - The proposed new crate name and its dependency graph
3. Assign to the next planning cycle
4. The crate budget is not raised without an architectural decision record (ADR)

---

## Current Status (as of Campaign 2 start)

| Crate | Lines | Budget | Status |
|-------|-------|--------|--------|
| fraiseql-core | 209,277 | 150,000 | ❌ Over — split in progress (Batch 5) |
| fraiseql-server | ~54,000 | 80,000 | ✅ |
| fraiseql-observers | ~31,000 | 40,000 | ✅ |
| fraiseql-auth | ~20,000 (est.) | 40,000 | ✅ |
| All others | < 20,000 | 30,000 | ✅ |
