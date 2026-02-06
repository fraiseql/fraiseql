# Quick Start Guide - Rust Mutation Pipeline Implementation

## Overview

This is an **8-phase implementation** to replace the fragile 5-layer Python/Rust mutation architecture with a clean 2-layer Rust pipeline.

**Timeline**: 13.5-18.5 days (67.5-92.5 hours)
**LOC Impact**: ~1300 lines deleted, ~1000 lines Rust added (net -300 LOC)

## Phases at a Glance

| Phase | Duration | What | Status |
|-------|----------|------|--------|
| **1. Core Types** | 1-2 days | Rust types, parsing, format detection | ✅ Completed |
| **2. Entity Processing** | 2-3 days | Wrapper detection, __typename, CASCADE | ✅ Completed |
| **3. Response Building** | 2-3 days | GraphQL response construction | ✅ Completed |
| **4. Python Integration** | 2 days | Simplify Python, delete old code | ⬜ Not Started |
| **5. Testing** | 3-4 days | Comprehensive tests, edge cases | ⬜ Not Started |
| **6. Documentation** | 1-2 days | Docs, migration guide, examples | ⬜ Not Started |
| **7. Naming Cleanup** | 0.5 days | Remove "v2" terminology | ⬜ Not Started |
| **8. Cleanup Audit** | 1 day | Remove old doc/code remnants | ⬜ Not Started |

## How to Use This

### Option 1: Implement Yourself with Agent Help

Work through each phase sequentially:

```bash
# Start with Phase 1
cat .phases/rust-mutation-pipeline/phase1-core-types.md

# Implement Task 1.1 manually or with agent help
# Then Task 1.2, Task 1.3

# Move to Phase 2 when Phase 1 complete
cat .phases/rust-mutation-pipeline/phase2-entity-processing.md

# Continue through all phases
```

### Option 2: Use Task-by-Task with Agents

Each task is designed to be agent-implementable:

```bash
# Example: Give Task 1.1 to an agent
"Please implement Task 1.1 from .phases/rust-mutation-pipeline/phase1-core-types.md"

# Agent implements, you verify
cargo test mutation::types --lib

# Move to next task
"Please implement Task 1.2..."
```

### Option 3: Phase-by-Phase Review

Use phases as planning documents, implement in your own style:

```bash
# Read phase overview
cat .phases/rust-mutation-pipeline/phase3-response-building.md

# Understand requirements
# Implement however you prefer
# Check against acceptance criteria
```

## Current State Check

Before starting, verify current state:

```bash
# Check existing Rust code
ls fraiseql_rs/src/

# Check existing Python mutation code
ls src/fraiseql/mutations/

# Run existing tests (should all pass before starting)
pytest tests/unit/mutations/test_rust_executor.py -v
pytest tests/integration/graphql/mutations/test_mutation_patterns.py -v

# Check Rust compiles
cd fraiseql_rs && cargo build && cd ..
```

## Key Decisions Already Made

1. **Two formats only**: Simple (entity-only) and Full (mutation_response)
2. **CASCADE is a field**: Not a separate format variant
3. **Auto-detection**: Based on presence of valid `status` field
4. **Keep existing tests**: Update incrementally, don't start from scratch
5. **Dict responses**: Phase 4 changes from typed objects to dicts

## Critical Invariants

These must ALWAYS be true:

1. ✅ CASCADE at success level (sibling to entity, NEVER nested inside)
2. ✅ __typename always present in Success response and entity
3. ✅ Format detection is deterministic
4. ✅ Status → HTTP code mapping correct
5. ✅ camelCase conversion reversible

## When to Stop and Ask Questions

Stop and clarify if:

- ❌ Tests are failing and you don't know why
- ❌ CASCADE ends up nested in entity
- ❌ __typename is missing anywhere
- ❌ Format detection seems ambiguous
- ❌ You're not sure how to update tests in Phase 4

## Verification Commands

After each task:

```bash
# Rust
cd fraiseql_rs
cargo test
cargo clippy
cd ..

# Python (Phases 1-3: should still pass unchanged)
pytest tests/unit/mutations/test_rust_executor.py -v

# Python (Phase 4+: update for new behavior)
pytest tests/integration/graphql/mutations/ -v
```

## Common Pitfalls

1. **Forgetting to update `mod.rs`**: Always export new modules
2. **Missing PyO3 bindings**: Phase 3.4 is critical
3. **Breaking tests too early**: Don't change Python until Phase 4
4. **CASCADE placement**: Always verify it's at success level, not in entity
5. **Skipping acceptance criteria**: Check every box before moving on

## File Tracking

### New Files (Phases 1-3)

- `fraiseql_rs/src/mutation/mod.rs`
- `fraiseql_rs/src/mutation/types.rs`
- `fraiseql_rs/src/mutation/parser.rs`
- `fraiseql_rs/src/mutation/entity_processor.rs`
- `fraiseql_rs/src/mutation/response_builder.rs`
- `fraiseql_rs/src/mutation/tests.rs` (Phase 5)

### Files to Delete (Phase 4)

- `src/fraiseql/mutations/entity_flattener.py` ❌
- `src/fraiseql/mutations/parser.py` ❌
- `tests/unit/mutations/test_entity_flattener.py` ❌

### Files to Update (Phase 4)

- `src/fraiseql/mutations/rust_executor.py` (simplify)
- `src/fraiseql/mutations/mutation_decorator.py` (return dicts)
- `tests/unit/mutations/test_rust_executor.py` (minor updates)
- `tests/integration/graphql/mutations/*.py` (dict access)

## Progress Tracking

Mark phases as complete:

```bash
# Update this file as you progress
# Or use a separate tracking document

# Example:
# Phase 1: ✅ Complete (2025-XX-XX)
# Phase 2: 🔄 In Progress (Task 2.2 done)
# Phase 3: ⬜ Not Started
```

## Emergency Rollback

If something goes catastrophically wrong:

```bash
# Phases 1-3: Just delete Rust files (Python unaffected)
rm -rf fraiseql_rs/src/mutation/

# Phase 4+: Revert commits
git log  # Find last good commit
git revert <commit-hash>

# Or full rollback
git reset --hard <before-phase-4-commit>
```

## Questions?

- Check the main plan: `/tmp/fraiseql_rust_greenfield_implementation_plan_v2.md`
- Review specific phase files in `.phases/rust-mutation-pipeline/`
- Look at existing test patterns in `tests/`
- Ask specific questions about individual tasks

## Success Criteria (Final)

Before declaring complete:

- [ ] All phases 1-6 complete
- [ ] All tests passing (Rust + Python)
- [ ] ~1300 LOC deleted
- [ ] CASCADE never nested in entity
- [ ] __typename always present
- [ ] Code coverage >90% Rust, >85% Python
- [ ] Documentation complete
- [ ] No known critical bugs

**Good luck! 🚀**
