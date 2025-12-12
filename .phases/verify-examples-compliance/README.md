# Examples Compliance Verification Plan

## Overview

This multi-phase plan will systematically verify that all FraiseQL examples (especially SQL examples) match the expected patterns documented in:
- PrintOptim Database Patterns (`~/.claude/skills/printoptim-database-patterns.md`)
- FraiseQL documentation (`docs/core/concepts-glossary.md`, `README.md`)
- Code/tests in the codebase

## Objectives

1. **Verify Trinity Identifier Pattern Compliance**
   - All tables have `pk_*`, `id`, `identifier` where appropriate
   - Views expose `id` (and `pk_*` if referenced by other views)
   - JSONB never includes `pk_*` (internal only)
   - GraphQL types never expose `pk_*`

2. **Verify JSONB View Pattern Compliance**
   - Views use `jsonb_build_object()` correctly
   - All fields in JSONB match GraphQL type definitions
   - No accidental field exposure (security)

3. **Verify Foreign Key Pattern Compliance**
   - All FKs reference `pk_*` (INTEGER), never `id` (UUID)
   - Correct FK naming: `fk_<entity>`

4. **Verify Helper Function Pattern Compliance**
   - Helper functions follow naming: `core.get_pk_<entity>()`, `core.get_<entity>_id()`
   - Variable naming conventions: `v_<entity>_pk`, `v_<entity>_id`, etc.
   - Functions use helpers instead of inline subqueries

5. **Verify Mutation Function Pattern Compliance**
   - Functions return JSONB with proper structure
   - Success/error handling follows patterns
   - Explicit sync calls for `tv_*` tables (no auto-update assumptions)

6. **Verify Documentation Examples Match Code**
   - SQL examples in README.md match actual code
   - Code comments match documentation
   - Examples are executable and produce expected results

## Success Criteria

- ✅ All examples pass automated verification script
- ✅ Discrepancies documented with clear remediation plan
- ✅ Examples can be used as reference implementations
- ✅ Documentation is accurate and up-to-date

## Phases

1. **Phase 1: Discovery** - Inventory all examples and patterns
2. **Phase 2: Pattern Extraction** - Extract verification rules from docs/code
3. **Phase 3: Automated Verification** - Build verification script
4. **Phase 4: Manual Review** - Review edge cases and complex patterns
5. **Phase 5: Remediation** - Fix identified issues
6. **Phase 6: Documentation Update** - Update docs with findings

## Timeline

Estimated: 2-3 days (with agent automation)

## Dependencies

- Access to all example directories
- Access to documentation files
- PostgreSQL connection for testing SQL examples
- Test suite execution capability
