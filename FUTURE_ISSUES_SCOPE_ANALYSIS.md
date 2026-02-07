# Scope Analysis: Issues #258 and #225

**Date**: February 7, 2026
**Status**: Planning document for future releases

---

## Quick Summary

| Issue | Title | Scope | Complexity | Risk | Target |
|-------|-------|-------|-----------|------|--------|
| #258 | Schema dependency graph | 1,300-1,900 LOC | Medium | Low-Med | v2.1.0+ |
| #225 | Security testing gaps | 1,850-2,350 LOC | Medium-High | Medium-High | v1.9.6 |

**Key Insight**: #225 is significantly more complex and risky due to security-critical nature. #258 is a cleaner feature addition with lower risk.

---

## Issue #258: Schema Dependency Graph and Validation

### What It Does
```bash
# Analyze a view's dependencies
$ fraiseql deps --view v_order_summary

v_order_summary
├── source: tb_order
│   ├── required columns: [id, fk_customer, status, amount]
│   └── populated by:
│       ├── fn_create_order ✅ (all columns)
│       └── fn_import_orders ⚠️ (missing: fk_customer)
├── joins:
│   ├── tb_customer ON fk_customer → customer_name
│   └── tb_product ON fk_product → product_name
└── tenant_filter: fk_customer → tv_organization.pk

# Validate entire schema
$ fraiseql deps --validate
✅ v_user: All dependencies satisfied
⚠️ v_order_summary: fn_import_orders missing fk_customer
❌ v_audit_log: No functions populate tb_audit_log
```

### Work Breakdown

| Phase | Component | Size | Complexity | Risk |
|-------|-----------|------|-----------|------|
| 1 | SQL Parser Enhancement | 200-300 LOC | Medium | Low |
| 2 | Graph Data Structure | 150-200 LOC | Low-Med | Low |
| 3 | Validation Rules | 200-300 LOC | Medium | Med |
| 4 | CLI Commands | 100-150 LOC | Low | Low |
| 5 | Output Formatters | 150-200 LOC | Low | Very Low |
| 6 | Tests | 400-600 LOC | Medium | Low |
| 7 | Documentation | 100-150 LOC | Low | Low |

### Implementation Path

```
Week 1: Parser Enhancement + Graph Data Structure
  ↓
Week 2: Validation Rules + Comprehensive Tests
  ↓
Week 3: CLI Commands + Output Formatters
  ↓
Week 4: Documentation + CI/CD Integration
```

### Success Criteria
- ✅ All entity types supported (views, tables, functions)
- ✅ CLI commands work as documented
- ✅ 80%+ test coverage
- ✅ No regressions in existing features
- ✅ Documentation complete and examples work

### Resource Profile
- **Team Size**: 1-2 engineers
- **Skills Needed**: SQL parsing, graph algorithms, CLI design
- **Code Review**: 1 reviewer (architecture focus)
- **Domain Experts**: Optional (helpful but not required)

### Why This Is Lower Risk
- Pure analysis feature (no enforcement)
- Uses existing schema parser
- Can be tested independently
- Non-breaking addition
- Incremental delivery possible

---

## Issue #225: Security Testing & Enforcement Gaps

### What It Does

**Current State** (v1.9.5):
- JWT validation tests: 11 stubs (not implemented)
- RBAC enforcement: Framework exists, not verified
- Security profiles: 15 settings configured, 0 enforced
- Field filtering: Only for APQ queries
- Documentation: 57% accuracy on security claims

**Goal** (v1.9.6):
- JWT validation: Complete implementation + 100% test coverage
- RBAC enforcement: Verified in all tests
- Security profiles: All 3 fully enforced (STANDARD, REGULATED, etc.)
- Field filtering: Works for all GraphQL query types
- Documentation: 90%+ accurate

### Work Breakdown

| Phase | Component | Size | Complexity | Risk |
|-------|-----------|------|-----------|------|
| 1 | JWT Tests (11 stubs) | 150-200 LOC | Low | Low |
| 2 | RBAC Enforcement Tests | 200-300 LOC | Medium | Med |
| 3 | Profile Enforcement | 300-400 LOC | High | High |
| 4 | Field Filtering Unification | 150-200 LOC | Medium | Med |
| 5 | Startup Validation | 100-150 LOC | Low | Low |
| 6 | Test Framework Helpers | 150-200 LOC | Low-Med | Low |
| 7 | Documentation Review | 200-300 LOC | Low | Low |
| 8 | Comprehensive Tests | 600-800 LOC | Medium | Low |

### Implementation Path

**Phase 1: Quick Wins (Parallel)**
```
JWT Tests Completion        RBAC Tests              Documentation
(straightforward tests)   (enforcement scenarios)  (review & fixes)
```

**Phase 2: Core Enforcement (Sequence)**
```
Field Filtering Tests
  ↓
Field Filtering Implementation
  ↓
Profile Enforcement Implementation
  ↓
Startup Validation
```

**Phase 3: Verification**
```
Full Regression Suite
  ↓
Security Audit
  ↓
Staged Rollout (feature flags)
```

### Success Criteria
- ✅ All 11 JWT tests completed (not stubs)
- ✅ RBAC enforcement verified in tests
- ✅ All security profiles enforced
- ✅ Field filtering consistent across all query types
- ✅ Documentation 90%+ accurate
- ✅ Startup validation catches misconfiguration
- ✅ 70% of tests verify enforcement (not just config)
- ✅ Zero security regressions
- ✅ All 5,991+ existing tests still pass

### Resource Profile
- **Team Size**: 2-3 engineers (security-critical)
- **Skills Needed**: Security architecture, testing patterns, enforcement design
- **Code Review**: 2 reviewers (security + architecture)
- **Domain Experts**: Required (security expertise essential)
- **Security Audit**: Mandatory before release

### Why This Is Higher Risk
- Security-critical (enforces user access control)
- Multiple enforcement points across codebase
- Behavioral changes (field filtering expansion)
- Regression potential (must not weaken existing enforcement)
- Requires deep security expertise

### Mitigation Strategies
1. **Continuous Testing**: Run security suite after every change
2. **Feature Flags**: Gate new enforcement behind flags
3. **Staged Rollout**: Gradual production deployment
4. **Audit Trail**: Log all enforcement decisions
5. **Regression Pack**: Comprehensive test suite before release

---

## Comparative Analysis

### Complexity Comparison

```
Issue #225 (Security)
████████████████████ 100% (High: multiple critical components)

Issue #258 (Schema Analysis)
██████████░░░░░░░░░░ 60% (Medium: feature addition, lower risk)
```

### Recommended Release Scheduling

**Option A: Parallel** (More resources, faster)
- v1.9.6: Issue #225 (security fixes)
- v2.1.0: Issue #258 (schema analysis)

**Option B: Sequential** (Fewer resources, staggered)
- v1.9.6: Issue #225 (security fixes - critical)
- v1.9.7: Issue #258 Phase 1 (parser + graph)
- v2.0.0: Issue #258 Phase 2 (CLI + validation)

**Recommended**: Option A (security is time-sensitive, schema analysis can follow)

### Effort Distribution

If executing both:
- **Issue #225**: 60% of resources (critical path, security)
- **Issue #258**: 40% of resources (can proceed in parallel after initial phase)

---

## Implementation Recommendations

### For Issue #258

**Start With**:
1. Create `crates/fraiseql-core/src/schema/dependency_graph.rs` module
2. Write tests for graph operations
3. Extend parser to track dependencies

**Key Decisions**:
- Which dependency types to track (all or subset?)
- Performance impact on schema compilation
- False positive tolerance for validation rules

**Optional Features** (can defer):
- Graphviz export
- SARIF CI/CD integration
- Custom validation rules API

### For Issue #225

**Start With**:
1. Inventory all security enforcement points
2. Complete JWT tests (straightforward)
3. Add enforcement verification helpers
4. Update documentation with current state

**Key Decisions**:
- Security profile enforcement priority (STANDARD first?)
- Feature flag names and rollout strategy
- Startup validation failure behavior (fail hard vs. warn)

**Mandatory**:
- Security audit before release
- All JWT tests completed (not optional)
- Zero regressions in existing tests

---

## Technical Debt Addressed

### Issue #258
- No immediate debt; it's a new feature

### Issue #225
- Converts 11 JWT stub tests to real tests
- Removes false claims from documentation
- Provides enforcement verification patterns
- Improves startup validation

---

## Dependencies and Blockers

### Issue #258
- ✅ Schema parser: Ready
- ✅ CLI framework: Ready
- ⚠️ Domain knowledge: Helpful but can proceed

### Issue #225
- ✅ Security framework: Ready
- ✅ RBAC system: Ready
- ⚠️ Profile enforcement: May need refactoring
- ⚠️ Security expertise: Required

---

## Questions to Answer Before Starting

### Issue #258
1. What's the user's primary use case? (Onboarding? CI/CD validation? Troubleshooting?)
2. Should validation be opinionated or customizable?
3. Performance constraints on schema analysis?
4. Priority on export formats (JSON? Dot? SARIF for CI/CD)?

### Issue #225
1. Which security profile is most important? (STANDARD? REGULATED?)
2. How strict should startup validation be? (Fail vs. warn?)
3. Should field filtering be backward compatible or breaking change?
4. Timeline for security audit and staged rollout?

---

## Rollback Plan

### Issue #258
- Easy rollback (feature is additive, no breaking changes)
- Just disable CLI commands if issues found

### Issue #225
- Harder rollback (enforces access control)
- Use feature flags for gradual deployment
- Maintain old enforcement path during transition
- Quick revert if regression detected

---

## Success Metrics

### Issue #258
- Adoption: Developers using `fraiseql deps` command
- Value: Fewer schema-related bugs reported
- Performance: Analysis completes in <1 second for typical schemas
- Test pass rate: 100% (all new tests passing)

### Issue #225
- Security: Zero enforcement regressions
- Coverage: 70%+ of tests verify enforcement
- Accuracy: Documentation 90%+ accurate
- Adoption: All security profiles configured in production
- Test pass rate: 100% (all 5,991+ tests passing)

---

## Next Steps

When ready to implement:

1. **For #258**: Create spike/prototype with parser enhancements
2. **For #225**: Inventory all enforcement points in codebase
3. Create detailed task breakdowns for each work package
4. Allocate resources based on priority
5. Begin with lowest-risk packages first

---

**This document should be reviewed before starting either issue to ensure alignment on scope and approach.**
