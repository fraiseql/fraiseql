# Rust Mutation Pipeline - Phased Implementation

## Overview

This directory contains the phased implementation plan for the greenfield Rust mutation pipeline that will replace the current 5-layer Python/Rust architecture with a unified 2-layer Rust pipeline.

## Goal

Replace:
- **Current**: PostgreSQL → Python Normalize → Python Flatten → Rust Transform → Python Parse → JSON (~2300 LOC)
- **Target**: PostgreSQL → Rust Pipeline → JSON/Dict (~1000 LOC Rust)

**Net reduction**: ~1300 LOC, single source of truth, type-safe throughout

## Two Formats

1. **Simple**: Entity-only JSONB (no status field) - auto-detected
2. **Full**: mutation_response with status/message/entity/cascade - auto-detected

CASCADE is just an optional field in Full format, not a separate format variant.

## Phases

### Phase 1: Core Rust Types (FOUNDATION)
**Duration**: 1-2 days
**Files**: 3 Rust files
**Tests**: Keep existing, will be green throughout

Define the foundational type system and format detection.

- [ ] Task 1.1: Core types (MutationResponse, StatusKind)
- [ ] Task 1.2: Format detection logic
- [ ] Task 1.3: Basic parser (JSON → types)

### Phase 2: Entity Processing (CORE LOGIC)
**Duration**: 2-3 days
**Files**: 2 Rust files
**Tests**: Keep existing, update incrementally

Handle entity extraction, __typename, and camelCase conversion.

- [ ] Task 2.1: Entity processor (wrapper detection)
- [ ] Task 2.2: __typename injection
- [ ] Task 2.3: CASCADE processing

### Phase 3: Response Building (TRANSFORMATION)
**Duration**: 2-3 days
**Files**: 2 Rust files
**Tests**: Keep existing, update for dict responses

Build GraphQL-compliant responses from processed data.

- [ ] Task 3.1: Success response builder
- [ ] Task 3.2: Error response builder
- [ ] Task 3.3: Schema validation

### Phase 4: Python Integration (GLUE)
**Duration**: 2 days
**Files**: 3 Python files (simplify/delete)
**Tests**: Update for new behavior

Integrate Rust pipeline with Python layer.

- [ ] Task 4.1: PyO3 bindings
- [ ] Task 4.2: Simplify rust_executor.py
- [ ] Task 4.3: Update mutation_decorator.py
- [ ] Task 4.4: Delete obsolete files

### Phase 5: Testing & Validation (QUALITY)
**Duration**: 3-4 days
**Files**: Test updates
**Tests**: Comprehensive validation

Ensure everything works correctly.

- [ ] Task 5.1: Update existing tests for dict responses
- [ ] Task 5.2: Add Rust unit tests
- [ ] Task 5.3: Add integration tests
- [ ] Task 5.4: Property-based tests

### Phase 6: Documentation (POLISH)
**Duration**: 1-2 days
**Files**: Documentation
**Tests**: N/A

Document the new architecture.

- [ ] Task 6.1: Architecture docs
- [ ] Task 6.2: Migration guide
- [ ] Task 6.3: Examples

### Phase 7: Naming Cleanup (POLISH)
**Duration**: 0.5 days (4 hours)
**Files**: Test file, docs
**Tests**: Rename only

Remove confusing "v2" terminology, use clear "Simple" and "Full" format names.

- [ ] Task 7.1: Update test comments (v2 → Full)
- [ ] Task 7.2: Rename result2 → result_reparsed
- [ ] Task 7.3: Verify types.rs terminology
- [ ] Task 7.4: Verify phase docs
- [ ] Task 7.5: Add terminology glossary

### Phase 8: Cleanup Audit (FINAL QUALITY GATE)
**Duration**: 1 day (8 hours)
**Files**: Docs, comments, examples
**Tests**: Verification only

Audit and remove outdated documentation and code remnants from old architecture.

- [ ] Task 8.1: Audit documentation files
- [ ] Task 8.2: Audit code comments
- [ ] Task 8.3: Audit docstrings
- [ ] Task 8.4: Audit test comments
- [ ] Task 8.5: Audit examples and guides
- [ ] Task 8.6: Check for import remnants
- [ ] Task 8.7: Audit configuration files
- [ ] Task 8.8: Create cleanup summary

## Total Timeline

**13.5-18.5 days** (67.5-92.5 developer hours)

## Test Strategy

### Existing Tests: KEEP THEM

We have excellent test coverage:
- `test_rust_executor.py` - Tests current executor (will update incrementally)
- `test_mutation_patterns.py` - Tests both simple and class-based patterns (will stay green)
- `test_entity_flattener.py` - Will be deleted in Phase 4 (entity_flattener.py deleted)
- `test_executor.py` - Tests mutation execution (will update for new behavior)

### Approach

1. **Phase 1-3**: Existing tests stay green (Rust is additive)
2. **Phase 4**: Update tests for dict responses (from typed objects)
3. **Phase 5**: Add new tests for edge cases

## Success Criteria

- [ ] All existing tests pass (with minimal updates)
- [ ] ~1300 LOC deleted (entity_flattener.py, parser.py, etc.)
- [ ] Zero CASCADE bugs
- [ ] Zero __typename bugs
- [ ] >95% Rust test coverage
- [ ] >85% Python test coverage

## Next Steps

1. Review each phase file in detail
2. Start with Phase 1, Task 1.1
3. Run tests after each task
4. Commit after each task completes
