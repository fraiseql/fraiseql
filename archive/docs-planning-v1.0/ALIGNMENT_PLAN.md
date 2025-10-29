# Documentation Alignment Plan

**Based on**: AUDIT_REPORT.md findings (corrected)
**Total Issues**: 9 total issues (2 critical, 2 high priority, 3 medium, 2 low)
**Estimated Total Effort**: 6-7 hours

## Prioritized Task List

### Critical Tasks (Fix Immediately - Blockers for Examples/Teaching)
**Impact**: Prevents teaching wrong patterns to users

- **C1: Remove DataLoader Anti-Pattern from blog_api Example**
  - **Description**: Replace DataLoader usage in `examples/blog_api/` with PostgreSQL views to teach correct Rust pipeline pattern
  - **Files**: `examples/blog_api/queries.py`, `examples/blog_api/dataloaders.py`, `examples/blog_api/db.py`
  - **Why Critical**: Beginner example teaching wrong pattern that breaks PostgreSQL → Rust → HTTP flow
  - **Success Criteria**: blog_api uses PostgreSQL views instead of DataLoader; tests pass

- **C2: Correct tv_* Auto-Update Documentation**
  - **Description**: Remove incorrect claims that tv_* tables auto-update via PostgreSQL GENERATED ALWAYS
  - **Files**: `docs/database/TABLE_NAMING_CONVENTIONS.md`
  - **Why Critical**: PostgreSQL doesn't support cross-table GENERATED columns; misleading users
  - **Success Criteria**: Documentation accurately describes tv_* as manually maintained projection tables requiring explicit sync

### High Priority Tasks (Fix Soon - Messaging & Quality)
**Impact**: Ensures consistent messaging and professional documentation

- **H1: Clean Documentation Hygiene Issues**
  - **Description**: Remove timestamps, "NEW:" prefixes, and editing traces
  - **Files**: `docs/migration-guides/multi-mode-to-rust-pipeline.md`, `docs/core/ddl-organization.md`, `docs/database/TABLE_NAMING_CONVENTIONS.md`
  - **Why High**: Unprofessional appearance in documentation
  - **Success Criteria**: No timestamps or editing traces visible

- **H2: Verify Performance Claims**
  - **Description**: Audit and verify performance claims against benchmarks
  - **Files**: `docs/core/concepts-glossary.md`, `docs/strategic/V1_VISION.md`, `docs/rust/RUST_FIRST_PIPELINE.md`, `docs/performance/PERFORMANCE_GUIDE.md`
  - **Why High**: Unsubstantiated claims damage credibility
  - **Success Criteria**: All claims backed by benchmark data or removed

### Medium Priority Tasks (Improve - Consistency & Links)
**Impact**: Enhances usability and navigation

- **M1: Verify Cross-References**
  - **Description**: Systematically check all internal links work correctly
  - **Scope**: All docs/ files with internal links
  - **Why Medium**: Broken links frustrate users
  - **Success Criteria**: All internal links functional

- **M2: Improve Example Quality Audit**
  - **Description**: Ensure all examples follow best practices and correct patterns
  - **Scope**: All examples/ directory (already mostly good per audit)
  - **Why Medium**: Examples are primary learning tools
  - **Success Criteria**: All examples use PostgreSQL views, no DataLoader

- **M3: Standardize Architecture Descriptions**
  - **Description**: Ensure consistent PostgreSQL → Rust → HTTP flow descriptions
  - **Scope**: Files mentioning architecture flow
  - **Why Medium**: Core messaging consistency
  - **Success Criteria**: Uniform architecture descriptions

### Low Priority Tasks (Polish - Nice to Have)
**Impact**: Minor improvements for completeness

- **L1: Fix Minor Formatting Inconsistencies**
  - **Description**: Address formatting issues found in audit
  - **Scope**: Files with formatting problems
  - **Why Low**: Cosmetic improvements
  - **Success Criteria**: Consistent formatting across docs

- **L2: Add Additional Cross-References**
  - **Description**: Add more links between related documentation sections
  - **Examples**: Link performance claims to benchmark docs
  - **Why Low**: Enhances navigation but not critical
  - **Success Criteria**: Improved cross-linking where beneficial

## Dependencies

- **C1** must complete before **M2** (example quality depends on DataLoader removal)
- **C2** has no dependencies
- **H1-H2** can run in parallel
- **M1-M3** can run in parallel after C1 completes
- **L1-L2** can run anytime after M1-M3

## Effort Estimates

- **C1: Remove DataLoader (2-3 hours)**: Refactor blog_api example to use PostgreSQL views
- **C2: Correct tv_* docs (45 minutes)**: Update documentation text
- **H1: Clean hygiene (30 minutes)**: Remove timestamps and prefixes
- **H2: Verify performance (45 minutes)**: Cross-reference with benchmarks
- **M1: Verify links (1 hour)**: Systematic link checking
- **M2: Example audit (30 minutes)**: Verify all examples correct (mostly done)
- **M3: Architecture consistency (30 minutes)**: Standardize descriptions
- **L1: Formatting (30 minutes)**: Fix inconsistencies
- **L2: Cross-references (30 minutes)**: Add beneficial links

**Total Estimate**: 6-7 hours

## Execution Progress Tracker

### Critical Tasks
- [x] C1: Remove DataLoader Anti-Pattern from Examples ✅ Completed
- [x] C2: Correct tv_* Auto-Update Misconceptions ✅ Completed

### High Priority Tasks
- [x] H1: Clean Up Documentation Hygiene Issues ✅ Completed
- [x] H2: Verify and Justify Performance Claims ✅ Completed

### Medium Priority Tasks
- [ ] M1: Verify Cross-References and Navigation
- [ ] M2: Improve Architecture Consistency Descriptions
- [ ] M3: Enhance Example Quality and Documentation

### Low Priority Tasks
- [ ] L1: Minor Formatting and Consistency Improvements
- [ ] L2: Add Additional Cross-References

## Completion Criteria
- [ ] All critical tasks completed (DataLoader removed, tv_* misconceptions fixed)
- [ ] All high priority tasks completed (hygiene clean, performance claims verified)
- [ ] Medium priority tasks at 80%+ completion (cross-references working, examples improved)
- [ ] Low priority tasks attempted (formatting consistent, extra links added)
- [ ] All changes tested and verified
- [ ] Git commits are clean with good messages

## Execution Order

1. Phase 3A: Critical tasks (C1, C2) - Sequential, 2-3 hours
2. Phase 3B: High priority tasks (H1, H2) - Can parallelize, 1.5 hours
3. Phase 3C: Medium priority tasks (M1, M2, M3) - Sequential, 2 hours
4. Phase 3D: Low priority tasks (L1, L2) - Final polish, 1 hour

**Total estimated effort**: 6-7 hours
