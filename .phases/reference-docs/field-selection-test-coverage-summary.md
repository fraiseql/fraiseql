# Field Selection Test Coverage - Quick Summary

**Prompt Location**: `/tmp/field-selection-test-coverage-improvement-prompt.md`
**Date**: 2025-12-12

---

## 📊 Current State

### ✅ What Works
- **1 critical test passing**: Rust field filtering for Success types
- **Integration tests exist**: Python selection filter utilities
- **Core functionality works**: Auto-injected fields ARE filtered when not requested

### ❌ What's Broken
- **4/5 tests failing**: Outdated expectations (`errors` on Success types)
- **Missing Error type tests**: No comprehensive Error response field filtering tests
- **Missing edge cases**: Named fragments, empty selection, performance tests

---

## 🎯 What We Need

### Priority 1: Fix Outdated Tests (2 hours)
- Remove `errors` field expectations from Success type tests
- Fix Rust API signatures in integration tests
- Update to v1.9.0+ behavior

### Priority 2: Add Error Type Coverage (2 hours)
- Test Error response field filtering
- Verify `code`, `errors` fields are filtered
- Verify `updatedFields`, `id` NOT present (v1.8.1)

### Priority 3: Add Edge Cases (2 hours)
- Named fragment support
- Empty/null selection
- Nested fields
- Cascade filtering
- Multiple entities

### Priority 4: E2E & Performance (2 hours)
- Real GraphQL → Database → Rust pipeline
- Performance benchmarks
- Canary tests for regression detection

---

## 📝 Key Questions for CTO

1. **Test Organization**: Where should field selection tests live?
   - `tests/unit/mutations/`
   - `tests/integration/`
   - New directory: `tests/unit/mutations/field_selection/`?

2. **Outdated Tests**: Fix or remove `tests/test_mutation_field_selection_integration.py`?
   - Uses old Rust API signatures
   - Expects removed fields

3. **Coverage Strategy**: Unit vs Integration vs E2E balance?

4. **Performance Baseline**: What's acceptable for field filtering overhead?

5. **Documentation**: Document field selection in API docs?

---

## 🎯 Success Criteria

After CTO's implementation plan:
- [ ] All field selection tests passing (100%)
- [ ] Success type field filtering: Comprehensive coverage
- [ ] Error type field filtering: Comprehensive coverage
- [ ] Edge cases covered: Named fragments, empty selection, etc.
- [ ] E2E tests: Real GraphQL queries
- [ ] Performance tests: Benchmarks exist
- [ ] Documentation: Examples and test README

---

## 📚 Reference

**Read this first**: `/tmp/field-selection-test-coverage-improvement-prompt.md`

Contains:
- Detailed context and current state analysis
- Specific test failures and root causes
- Required coverage matrix
- Code examples
- Risk assessment
- Timeline estimates
- Questions to address

---

**Next Step**: CTO reviews prompt and creates detailed implementation plan
