# FraiseQL v2.0 Preparation Checklist

**Status**: Preparation Phase
**Created**: January 8, 2026
**Target**: v2.0 Release
**Goal**: Establish organizational foundation and best practices

---

## 📋 Executive Summary

FraiseQL v2.0 preparation focuses on **organizational clarity** and **sustainable patterns**. The codebase is mature (5,991+ tests, 65+ modules) but has organizational debt that will compound with growth.

This checklist tracks preparation across:
1. **Documentation** (completed ✅)
2. **Organization** (in progress)
3. **Testing** (planned)
4. **CI/CD** (planned)
5. **Code Cleanup** (planned)

---

## Phase 0: Documentation (✅ COMPLETE)

### Immediate Actions (Week 1)

#### ✅ Archive Strategy

- [x] Create `.archive/README.md` - Archive policy and structure
- [x] Define what gets archived (deprecated, experimental, legacy)
- [x] Document archive resurrection process
- [x] Plan legacy directory migration

**Result**: Clear deprecation path for v2.0+

#### ✅ Deprecation Policy

- [x] Create `docs/DEPRECATION_POLICY.md` - Comprehensive policy
- [x] Document modular HTTP architecture (v2.0 approach)
- [x] Document HTTP server adapters (Axum, Actix, Hyper, custom)
- [x] Document legacy Python servers (FastAPI, Starlette archived)
- [x] Define 3-phase lifecycle (announcement, maintenance, removal)
- [x] Create deprecation timeline
- [x] Document middleware modularity and composability

**Result**: Users understand v2.0 HTTP architecture and migration path

#### ✅ Organization Documentation

- [x] Create `docs/ORGANIZATION.md` - Complete architecture guide
  - [x] Directory structure (root level)
  - [x] Python framework organization (9 tiers)
  - [x] Rust extension structure (including modular HTTP)
  - [x] Modular HTTP architecture (framework-agnostic)
  - [x] Middleware modularity and composition
  - [x] Framework adapters (Axum, Actix, Hyper, custom)
  - [x] Test suite organization
  - [x] Naming conventions
  - [x] Design patterns

**Result**: Clear navigation for contributors, detailed HTTP architecture explanation

#### ✅ Code Organization Standards

- [x] Create `docs/CODE_ORGANIZATION_STANDARDS.md` - Enforcement rules
  - [x] File organization rules
  - [x] File size guidelines (1,500 lines max)
  - [x] Naming conventions (Python, test, Rust)
  - [x] Module documentation requirements
  - [x] Test organization rules
  - [x] CI/CD checks definition

**Result**: Consistent standards across codebase

#### ✅ Module-Specific Structure Guides

- [x] `src/fraiseql/core/STRUCTURE.md` - Core module detail
  - [x] Description of each component
  - [x] Dependencies and relationships
  - [x] When to modify
  - [x] Refactoring roadmap (graphql_type.py is 45KB candidate)

- [x] `src/fraiseql/types/STRUCTURE.md` - Type system detail
  - [x] Decorator documentation
  - [x] Scalar type organization (standard, network, financial, contact, geographic)
  - [x] Adding new scalars template
  - [x] 40+ scalar inventory

- [x] `src/fraiseql/sql/STRUCTURE.md` - SQL generation detail
  - [x] WHERE/ORDER BY generation
  - [x] Operator strategy pattern
  - [x] Adding new operators
  - [x] Performance guidelines

**Result**: Developers understand module structure quickly

#### ✅ Test Organization Plan

- [x] Create `docs/TEST_ORGANIZATION_PLAN.md` - 4-week migration plan
  - [x] Current state analysis (30 root-level test files)
  - [x] Target state design
  - [x] Phased migration (categorize → classify → move → verify)
  - [x] File-by-file migration details
  - [x] Marker assignment
  - [x] Import path updates
  - [x] Verification checklist

**Result**: Clear roadmap to organize test suite

### Deliverables

| Document | Purpose | Status |
|----------|---------|--------|
| `.archive/README.md` | Archive policy | ✅ Complete |
| `docs/DEPRECATION_POLICY.md` | Feature deprecation | ✅ Complete |
| `docs/ORGANIZATION.md` | Architecture guide (350+ lines) | ✅ Complete |
| `docs/CODE_ORGANIZATION_STANDARDS.md` | Enforcement rules | ✅ Complete |
| `src/fraiseql/*/STRUCTURE.md` | Module details (3 files) | ✅ Complete |
| `docs/TEST_ORGANIZATION_PLAN.md` | Test reorganization (4-week plan) | ✅ Complete |

---

## Phase 1: Archive & Cleanup (📋 PLANNED)

### Archive Legacy Code (Week 2-3)

- [ ] **Move `.phases/` directory**
  - [ ] Move `.phases/` → `.archive/phases/`
  - [ ] Update `.gitignore` to exclude `.archive/`
  - [ ] Create `phases/README.md` explaining historical context
  - [ ] Git commit: "chore: archive development phases documentation"

- [ ] **Move `tests/archived_tests/` directory**
  - [ ] Move `tests/archived_tests/` → `.archive/test_archive/`
  - [ ] Document what each contains
  - [ ] Git commit: "chore: archive old test code"

- [ ] **Move `tests/prototype/` directory**
  - [ ] Move `tests/prototype/` → `.archive/experimental/prototype/`
  - [ ] Document what was being prototyped
  - [ ] Git commit: "chore: archive prototype code"

- [ ] **Archive v2 parallel development**
  - [ ] Move `fraiseql_v2/` → `.archive/experimental/v2/` (if exists)
  - [ ] Move `tests/v2_*/` → `.archive/experimental/v2_tests/`
  - [ ] Document v2 experiment status
  - [ ] Git commit: "chore: archive v2 experimental development"

### Result

```
.archive/
├── README.md                    # Archive policy
├── phases/                      # Old development phases
│   ├── README.md
│   └── [phase directories]
├── deprecated/                  # Removed features
├── experimental/                # Proof-of-concepts
│   ├── prototype/
│   ├── v2/
│   └── v2_tests/
└── test_archive/                # Old test code
```

---

## Phase 2: Test Suite Organization (📋 PLANNED)

### Consolidate Root Test Files (Week 4-5)

Implement `TEST_ORGANIZATION_PLAN.md`:

- [ ] **Week 1: Categorize**
  - [ ] Audit all root-level test files
  - [ ] Create categorization spreadsheet
  - [ ] Determine unit vs integration for each

- [ ] **Week 2: Create Directories**
  ```bash
  mkdir -p tests/unit/{apq,array_filtering,subscriptions,dataloader,mutations}
  mkdir -p tests/integration/{apq,subscriptions,caching}
  ```

- [ ] **Week 3: Move Files (with history)**
  ```bash
  git mv tests/test_apq_*.py tests/unit/apq/
  # ... 30 total file moves
  ```

- [ ] **Week 4: Verify & Test**
  - [ ] Run full test suite
  - [ ] Verify no test failures
  - [ ] Update test documentation

### Result

```
tests/
├── unit/
│   ├── apq/                     # 5 APQ tests
│   ├── array_filtering/         # 3 array tests
│   ├── subscriptions/           # 4 subscription tests
│   ├── dataloader/              # 3 dataloader tests
│   ├── mutations/               # 4 mutation tests
│   └── ...
├── integration/
│   ├── apq/
│   ├── subscriptions/
│   ├── caching/
│   └── ...
└── [system, regression, chaos, fixtures]
```

---

## Phase 3: Code Organization Validation (📋 PLANNED)

### Implement CI/CD Checks (Week 6)

- [ ] **File Structure Validation**
  - [ ] No new test files at `tests/` root
  - [ ] Proper directory structure
  - [ ] No deeply nested modules (>3 levels)

- [ ] **File Size Validation**
  - [ ] Source files < 1,500 lines (warn at 1,200)
  - [ ] Test files < 500 lines (warn at 400)
  - [ ] __init__.py < 100 lines

- [ ] **Naming Validation**
  - [ ] Files: `snake_case.py` (Python)
  - [ ] Classes: `PascalCase`
  - [ ] Functions: `snake_case`
  - [ ] Test classes: `Test*`
  - [ ] Test files: `test_*.py`

- [ ] **Documentation Validation**
  - [ ] Module has docstring
  - [ ] Public API exported in `__init__.py`
  - [ ] Type hints on public functions

- [ ] **Test Organization Validation**
  - [ ] All tests have `@pytest.mark.{type}`
  - [ ] All tests have `@pytest.mark.{feature}`
  - [ ] No tests in wrong directories

### Scripts to Create

```bash
scripts/
├── check_file_structure.py      # Directory structure validation
├── check_file_sizes.py          # File size enforcement
├── check_naming.py              # Naming convention validation
├── check_documentation.py       # Docstring/export validation
├── check_test_organization.py   # Test marker/location validation
└── check_all_organization.py    # Run all checks
```

---

## Phase 4: Core Module Refactoring (📋 PLANNED)

### Address Large Files (Week 7-8)

- [ ] **Evaluate graphql_type.py (45KB)**
  - [ ] Analyze into subcomponents
  - [ ] Design refactoring (likely → `graphql_type/` package)
  - [ ] Plan migration (parallel both versions initially)
  - [ ] Write tests for refactoring
  - [ ] **Decision**: Refactor in v2.0 or defer to v2.1

- [ ] **Monitor other large files**
  - [ ] Identify any other files approaching 1,500 lines
  - [ ] Flag for refactoring roadmap

### Result

Clear refactoring priorities for v2.1+

---

## Phase 5: Enterprise Module Consolidation (📋 PLANNED)

### Clarify Feature Boundaries (Week 9)

- [ ] **Audit enterprise features**
  - [ ] `enterprise/` - RBAC, audit, security
  - [ ] `security/` - Field-level constraints
  - [ ] `auth/` - Authentication

- [ ] **Identify overlaps**
  - [ ] Both `enterprise/security/` and `security/`?
  - [ ] Auth split between locations?

- [ ] **Consolidation options**
  - [ ] Option A: Merge all under `enterprise/`
  - [ ] Option B: Keep separate with clear boundaries
  - [ ] Option C: Reorganize by concern (auth, permissions, validation)

- [ ] **Document decision**
  - [ ] Update `docs/ORGANIZATION.md`
  - [ ] Create `enterprise/STRUCTURE.md` if consolidated
  - [ ] Add cross-reference in module docs

---

## Phase 6: HTTP Server Status (📋 PLANNED)

### Document Server Tiers (Week 9)

- [ ] **FastAPI (Primary)**
  - [x] Mark as production-ready
  - [ ] Document feature coverage (100%)
  - [ ] Link to FastAPI examples
  - [ ] Performance benchmarks

- [ ] **Starlette (Deprecated)**
  - [x] Mark as deprecated (v1.9.0)
  - [x] Document removal timeline (v2.0.0)
  - [ ] Create migration guide to FastAPI
  - [ ] Link in server selection docs

- [ ] **Axum (Experimental)**
  - [x] Mark as experimental/proof-of-concept
  - [x] Document non-production status
  - [ ] Document future decision timeline
  - [ ] Link issues/discussions

### Result

Users understand which server to use for their needs

---

## Phase 7: Documentation Review (📋 PLANNED)

### Complete Documentation Set (Week 10)

- [ ] **Update main README**
  - [ ] Link to new organization documentation
  - [ ] Add "v2.0 Organization" section

- [ ] **Create quick-start navigation**
  - [ ] New contributor guide (→ `docs/ORGANIZATION.md`)
  - [ ] Code standards guide (→ `docs/CODE_ORGANIZATION_STANDARDS.md`)
  - [ ] Module structure guides (→ `src/[module]/STRUCTURE.md`)

- [ ] **Update contributing guide**
  - [ ] Link to organization standards
  - [ ] Add checklist for new code
  - [ ] Reference CI/CD checks

- [ ] **Add version-specific docs**
  - [ ] Create `docs/v2.0/` directory
  - [ ] Document what changed in v2.0
  - [ ] Migration guide from v1.8.x

---

## Phase 8: CI/CD Integration (📋 PLANNED)

### Enforce Standards in Pipeline (Week 11)

- [ ] **GitHub Actions workflow**
  - [ ] Add organization check job
  - [ ] Run on every PR
  - [ ] Fail on violations
  - [ ] Report detailed errors

- [ ] **Pre-commit hooks**
  - [ ] Add file size check
  - [ ] Add naming check
  - [ ] Add documentation check

- [ ] **Make target**
  - [ ] `make check-organization` - Run all checks
  - [ ] `make check-organization-fix` - Auto-fix where possible

---

## Phase 9: Testing & Validation (📋 PLANNED)

### Verify v2.0 Readiness (Week 12)

- [ ] **Run full test suite**
  - [ ] 5,991+ tests pass
  - [ ] No regressions
  - [ ] All markers present

- [ ] **Validate organization**
  - [ ] Run all CI/CD checks
  - [ ] No organization violations
  - [ ] Documentation complete

- [ ] **Performance verification**
  - [ ] Run benchmarks
  - [ ] No performance regression
  - [ ] Rust pipeline working

- [ ] **Documentation review**
  - [ ] Navigation works
  - [ ] Examples accurate
  - [ ] Links functional

---

## Phase 10: v2.0 Release Preparation (📋 PLANNED)

### Final Steps (Week 13)

- [ ] **Update version**
  - [ ] Bump to v2.0.0
  - [ ] Update CHANGELOG
  - [ ] Create git tag

- [ ] **Release notes**
  - [ ] Document breaking changes (if any)
  - [ ] Highlight organization improvements
  - [ ] Link to migration guides

- [ ] **Announce v2.0**
  - [ ] GitHub release
  - [ ] Documentation site update
  - [ ] Announcement email/discussion

---

## Summary Dashboard

### Completion Status

```
Phase 0: Documentation          ✅ COMPLETE
├── Archive strategy            ✅ Done
├── Deprecation policy          ✅ Done
├── Organization documentation  ✅ Done (350+ lines)
├── Code standards              ✅ Done
├── Module structures (3)        ✅ Done
└── Test plan                   ✅ Done

Phase 1: Archive & Cleanup      📋 PLANNED (Week 2-3)
Phase 2: Test Organization      📋 PLANNED (Week 4-5)
Phase 3: CI/CD Validation       📋 PLANNED (Week 6)
Phase 4: Large File Refactoring 📋 PLANNED (Week 7-8)
Phase 5: Enterprise Consolidation 📋 PLANNED (Week 9)
Phase 6: HTTP Server Status     📋 PLANNED (Week 9)
Phase 7: Documentation Review   📋 PLANNED (Week 10)
Phase 8: CI/CD Integration      📋 PLANNED (Week 11)
Phase 9: Testing & Validation   📋 PLANNED (Week 12)
Phase 10: Release Preparation   📋 PLANNED (Week 13)
```

### Key Metrics

| Metric | Current | v2.0 Target | Status |
|--------|---------|-------------|--------|
| Documentation | 0 org guides | 6 core docs | ✅ Done |
| Test files at root | 30 | 0 | 📋 Planned |
| Largest source file | 45KB | < 20KB | 📋 Planned |
| CI/CD checks | 0 org checks | 5+ checks | 📋 Planned |
| Deprecation clarity | Unclear | Clear policy | ✅ Done |
| Archive strategy | None | Defined | ✅ Done |

---

## Immediate Next Steps (This Week)

1. **Review these documents** in your project
   - Read `docs/ORGANIZATION.md` (350+ lines)
   - Review `docs/CODE_ORGANIZATION_STANDARDS.md`
   - Check `docs/TEST_ORGANIZATION_PLAN.md`

2. **Begin Phase 1 archival**
   - Move `.phases/` → `.archive/phases/`
   - Move `tests/archived_tests/` → `.archive/test_archive/`
   - Create git commit

3. **Plan Phase 2 test reorganization**
   - Audit root-level test files
   - Create migration spreadsheet
   - Schedule 4-week test consolidation

4. **Set up CI/CD checks**
   - Create `scripts/check_*.py` files
   - Integrate with GitHub Actions
   - Document enforcement

---

## Success Criteria for v2.0

- [x] Clear organizational documentation (350+ lines)
- [x] Deprecation policy established
- [ ] Archive strategy implemented
- [ ] Test suite reorganized (30 → 0 root files)
- [ ] CI/CD organization checks active
- [ ] Code standards enforced
- [ ] Module structure documented
- [ ] All tests passing (5,991+)
- [ ] No organizational violations
- [ ] Benchmarks maintained/improved

---

## Communication Plan

### v2.0 Announcement

When complete, announce to users:

> FraiseQL v2.0 focuses on **organizational clarity and sustainable patterns**. Significant improvements:
>
> - 📚 Comprehensive documentation (architecture, code standards, test organization)
> - 🏗️ Clear module structure guides (7 tier-based design)
> - 🧪 Reorganized test suite (5,991+ tests, clearer structure)
> - 🚫 Deprecated Starlette server (migration guide provided)
> - 📋 Archive strategy for legacy code
> - ⚙️ CI/CD organization enforcement
>
> See [Migration Guide](docs/migration/v1.8-to-v2.0.md) for upgrade path.

---

## FAQ

**Q: Will v2.0 have breaking API changes?**
A: Primarily organizational. Few API changes. See `DEPRECATION_POLICY.md`.

**Q: When should I upgrade?**
A: v2.0 is backwards compatible with v1.8.x. Upgrade at your pace.

**Q: How long does migration take?**
A: Depends on usage. FastAPI users: minimal changes. Starlette users: see migration guide.

**Q: Can I use both v1.8.x and v2.0?**
A: Yes, temporarily. Long-term support follows SemVer policy.

---

## See Also

- Main documentation: `docs/ORGANIZATION.md`
- Deprecation policy: `docs/DEPRECATION_POLICY.md`
- Code standards: `docs/CODE_ORGANIZATION_STANDARDS.md`
- Test plan: `docs/TEST_ORGANIZATION_PLAN.md`
- Archive policy: `.archive/README.md`

---

**Last Updated**: January 8, 2026
**Status**: Phase 0 Complete, Phase 1-10 Planned
**Next Review**: After Phase 1 (2 weeks)
