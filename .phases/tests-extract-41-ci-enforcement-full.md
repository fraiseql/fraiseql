---
title: CI Enforcement — extend lint-tests-layout to full workspace
status: planned
---

# Phase 41: Full Workspace CI Enforcement

## Objective

Widen the `lint-tests-layout` check from `routes/rest/` to the entire
workspace `src/` tree. After this phase, the pattern is permanently
self-enforcing across all 15 crates.

## Prerequisites

Phases 11–40 must be complete. Run the final verification first:

```bash
grep -rn "^mod tests {" crates/*/src/ --include="*.rs" | grep -v "/tests\.rs:"
```

Expected output: empty (zero violations).

## Changes

### Makefile

Replace the scoped check in `lint-tests-layout` with a workspace-wide check:

**Before:**
```makefile
lint-tests-layout:
	@echo "=== Checking for inline test blocks in routes/rest/ ==="
	@violations=$$(grep -rn "^mod tests {" \
		crates/fraiseql-server/src/routes/rest/ --include="*.rs" \
		| grep -v "/tests\.rs:" || true); \
	...
```

**After:**
```makefile
lint-tests-layout:
	@echo "=== Checking for inline test blocks in src/ (workspace-wide) ==="
	@violations=$$(grep -rn "^mod tests {" \
		crates/*/src/ --include="*.rs" \
		| grep -v "/tests\.rs:" || true); \
	if [ -n "$$violations" ]; then \
		echo "ERROR: inline test blocks found — extract to tests.rs:"; \
		echo "$$violations"; \
		exit 1; \
	fi; \
	echo "OK: no inline test blocks in workspace src/"
```

### ci.yml

The CI step name should be updated to reflect the wider scope:

**Before:**
```yaml
- name: Enforce tests.rs layout in routes/rest/
```

**After:**
```yaml
- name: Enforce tests.rs layout (workspace-wide)
```

## Commit

```
ci: widen lint-tests-layout to full workspace — all crates enforced
```

## Final verification

```bash
make lint-tests-layout
# Expected: OK: no inline test blocks in workspace src/
```
