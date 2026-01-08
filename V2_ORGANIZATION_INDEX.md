# FraiseQL v2.0 Organization - Complete Index

**📍 START HERE**

This file provides a navigation guide to all v2.0 preparation documentation and resources.

---

## 📋 Quick Navigation

### 🚀 New to FraiseQL?
1. **Read first**: `docs/ORGANIZATION.md` (350+ lines - complete architecture)
2. **Then read**: `docs/CODE_ORGANIZATION_STANDARDS.md` (code guidelines)
3. **Learn modules**: `src/fraiseql/[module]/STRUCTURE.md` (specific areas)

### 📊 Managing the Project?
1. **Read first**: `V2_PREPARATION_CHECKLIST.md` (10-phase roadmap)
2. **Then read**: `V2_PREP_SUMMARY.md` (status and next steps)
3. **For tests**: `docs/TEST_ORGANIZATION_PLAN.md` (4-week migration)

### 🔧 Contributing Code?
1. **Read first**: `docs/CODE_ORGANIZATION_STANDARDS.md` (what's required)
2. **Check module**: `src/fraiseql/[module]/STRUCTURE.md` (how to extend)
3. **Add tests**: Follow patterns in `tests/unit/` or `tests/integration/`

### ⚠️ Using Deprecated Features?
1. **Check status**: `docs/DEPRECATION_POLICY.md` (server tiers)
2. **Migrate**: Reference migration guides (linked in policy)

---

## 📚 All Documentation Files

### Main Architecture (1,000+ lines)

| File | Lines | Purpose | Audience |
|------|-------|---------|----------|
| `docs/ORGANIZATION.md` | 350+ | Complete architecture guide | Everyone |
| `docs/CODE_ORGANIZATION_STANDARDS.md` | 250+ | Enforceable standards | Developers |
| `V2_PREPARATION_CHECKLIST.md` | 300+ | 10-phase roadmap | Project leads |
| `V2_PREP_SUMMARY.md` | 200+ | Executive summary | Decision makers |

### Module Guides (650+ lines)

| File | Lines | Module | Purpose |
|------|-------|--------|---------|
| `src/fraiseql/core/STRUCTURE.md` | 200+ | Core | Execution pipeline |
| `src/fraiseql/types/STRUCTURE.md` | 200+ | Types | Type system (40+ scalars) |
| `src/fraiseql/sql/STRUCTURE.md` | 250+ | SQL | Query generation |

### Plans & Policies (500+ lines)

| File | Lines | Purpose | Audience |
|------|-------|---------|----------|
| `docs/TEST_ORGANIZATION_PLAN.md` | 250+ | 4-week test migration | QA/Dev leads |
| `docs/DEPRECATION_POLICY.md` | 200+ | Feature lifecycle | Everyone |
| `.archive/README.md` | 50+ | Archive strategy | Dev leads |

### Reference
- `V2_ORGANIZATION_INDEX.md` - This file

---

## 🎯 Documentation by Role

### For New Contributors

**Goal**: Understand codebase, contribute effectively

**Read (in order)**:
1. `docs/ORGANIZATION.md` - Architecture overview (30 min)
2. `src/fraiseql/core/STRUCTURE.md` - Core module (15 min)
3. `docs/CODE_ORGANIZATION_STANDARDS.md` - Standards (20 min)

**Then**: Pick module, read its STRUCTURE.md, look at tests

**Time**: ~1 hour to understand architecture

### For Core Developers

**Goal**: Extend features, maintain modules

**Read**:
1. `src/fraiseql/[your-module]/STRUCTURE.md` - Module overview (20 min)
2. `docs/CODE_ORGANIZATION_STANDARDS.md` - Guidelines (15 min)
3. `tests/unit/[your-module]/` - Test patterns (30 min)

**Reference**: Module STRUCTURE when adding features

**Time**: Varies by complexity

### For Code Reviewers

**Goal**: Enforce standards, guide contributions

**Read once**:
1. `docs/CODE_ORGANIZATION_STANDARDS.md` - All requirements (20 min)
2. `docs/DEPRECATION_POLICY.md` - Status of features (15 min)

**Checklist when reviewing**:
- [ ] File in correct directory?
- [ ] Proper naming (`snake_case.py`)?
- [ ] Has docstring + public API exports?
- [ ] Type hints on public functions?
- [ ] Tests in proper location?
- [ ] File < 1,500 lines?
- [ ] Test file < 500 lines?

### For Project Managers

**Goal**: Track progress, plan releases

**Read**:
1. `V2_PREP_SUMMARY.md` - Current status (20 min)
2. `V2_PREPARATION_CHECKLIST.md` - Full roadmap (30 min)
3. `docs/TEST_ORGANIZATION_PLAN.md` - Test timeline (15 min)

**Track**: Progress against phases 1-10 in checklist

### For Release Managers

**Goal**: Manage versions, communicate changes

**Read**:
1. `docs/DEPRECATION_POLICY.md` - Feature status (20 min)
2. `docs/migration/` - Migration guides (as needed)
3. `V2_PREPARATION_CHECKLIST.md` - Phase 10 section (15 min)

**When releasing**: Reference checklist phase 10 tasks

### For End Users

**Goal**: Understand features, migrate code

**Read**:
1. `docs/DEPRECATION_POLICY.md` - Server status (15 min)
2. `docs/migration/v1.8-to-v2.0.md` - How to upgrade (varies)
3. `docs/ORGANIZATION.md` - Architecture reference (as needed)

---

## 📁 File Structure Reference

### Documentation Locations

```
fraiseql/
├── docs/
│   ├── ORGANIZATION.md                    ← Architecture
│   ├── CODE_ORGANIZATION_STANDARDS.md     ← Standards
│   ├── DEPRECATION_POLICY.md              ← Feature status
│   ├── TEST_ORGANIZATION_PLAN.md          ← Test migration
│   ├── migration/                         ← Migration guides
│   └── [other existing docs]
│
├── src/fraiseql/
│   ├── core/STRUCTURE.md                  ← Core module
│   ├── types/STRUCTURE.md                 ← Types module
│   ├── sql/STRUCTURE.md                   ← SQL module
│   └── [modules]
│
├── V2_PREP_SUMMARY.md                     ← Executive summary
├── V2_PREPARATION_CHECKLIST.md            ← Full checklist
├── V2_ORGANIZATION_INDEX.md               ← This file
│
└── .archive/
    └── README.md                          ← Archive policy
```

---

## 📊 Documentation Statistics

### Content Created
- **Total files**: 10
- **Total lines**: 3,000+ lines
- **Architecture docs**: 1,000+ lines
- **Module guides**: 650+ lines
- **Plans & policies**: 500+ lines

### Coverage
- **Modules documented**: 65+ modules described
- **Tiers documented**: 9 organizational tiers
- **Scalars documented**: 40+ custom types
- **Test categories**: Unit, integration, system, regression, chaos

---

## 🔍 Finding Information

### "Where do I put new X?"

| Question | Answer |
|----------|--------|
| New HTTP endpoint? | `src/fraiseql/fastapi/` |
| New scalar type? | `src/fraiseql/types/scalars/[category]/` |
| New SQL operator? | `src/fraiseql/sql/operators/[category]/` |
| Enterprise feature? | `src/fraiseql/enterprise/[feature]/` |
| Test for feature? | `tests/unit/[feature]/` or `tests/integration/[feature]/` |

See `docs/ORGANIZATION.md` for complete module map.

### "How do I X?"

| Question | Find in |
|----------|---------|
| Navigate codebase? | `docs/ORGANIZATION.md` (architecture) |
| Add new code? | `docs/CODE_ORGANIZATION_STANDARDS.md` |
| Extend a module? | `src/fraiseql/[module]/STRUCTURE.md` |
| Name something? | `docs/CODE_ORGANIZATION_STANDARDS.md` (conventions) |
| Organize tests? | `docs/TEST_ORGANIZATION_PLAN.md` |
| Check feature status? | `docs/DEPRECATION_POLICY.md` |

### "What's the status of X?"

| Feature | Status Location |
|---------|-----------------|
| FastAPI server | `docs/DEPRECATION_POLICY.md` (✅ primary) |
| Starlette server | `docs/DEPRECATION_POLICY.md` (🟡 deprecated) |
| Axum server | `docs/DEPRECATION_POLICY.md` (⚠️ experimental) |
| Test organization | `docs/TEST_ORGANIZATION_PLAN.md` |
| v2.0 preparation | `V2_PREPARATION_CHECKLIST.md` |

---

## ✅ Checklist for Different Tasks

### Adding New Code

- [ ] Read `docs/CODE_ORGANIZATION_STANDARDS.md`
- [ ] Read `src/fraiseql/[module]/STRUCTURE.md`
- [ ] Check directory structure matches guide
- [ ] Follow naming conventions
- [ ] Add docstring to module
- [ ] Export public API in `__init__.py`
- [ ] Add type hints to functions
- [ ] Write tests in `tests/unit/` or `tests/integration/`
- [ ] Mark tests with pytest markers
- [ ] Run `make check-organization` before commit

### Code Review

- [ ] Check against `docs/CODE_ORGANIZATION_STANDARDS.md`
- [ ] Verify file location
- [ ] Verify naming conventions
- [ ] Check file size < 1,500 lines
- [ ] Check test size < 500 lines
- [ ] Verify test markers present
- [ ] Reference module STRUCTURE.md if unclear

### Extending a Module

- [ ] Read `src/fraiseql/[module]/STRUCTURE.md`
- [ ] Understand module dependencies
- [ ] Follow component patterns
- [ ] Check "When to modify" section
- [ ] Add tests before implementing
- [ ] Follow code structure template

---

## 🚀 Getting Started (5-Minute Quickstart)

1. **See full architecture** (5 min):
   ```bash
   head -50 docs/ORGANIZATION.md
   ```

2. **Find your module** (2 min):
   - Locate in `docs/ORGANIZATION.md`
   - Read specific `STRUCTURE.md`

3. **Check standards** (3 min):
   ```bash
   head -100 docs/CODE_ORGANIZATION_STANDARDS.md
   ```

4. **Ready to code**:
   - Follow standards
   - Reference module guide
   - Write tests

---

## 📞 Support

### Questions about...

| Topic | Reference |
|-------|-----------|
| Architecture | `docs/ORGANIZATION.md` |
| Code standards | `docs/CODE_ORGANIZATION_STANDARDS.md` |
| Specific module | `src/fraiseql/[module]/STRUCTURE.md` |
| Feature status | `docs/DEPRECATION_POLICY.md` |
| Test organization | `docs/TEST_ORGANIZATION_PLAN.md` |
| v2.0 timeline | `V2_PREPARATION_CHECKLIST.md` |
| Archive policy | `.archive/README.md` |

### Can't find something?

1. Check `docs/ORGANIZATION.md` (architecture map)
2. Search for module in STRUCTURE.md files
3. Review `CODE_ORGANIZATION_STANDARDS.md` (might answer question)
4. Check GitHub issues/discussions

---

## 📅 Important Dates

| Milestone | Target | Status |
|-----------|--------|--------|
| Phase 0: Documentation | ✅ Complete | Done (Jan 8) |
| Phase 1: Archive cleanup | Week 2-3 | Planned |
| Phase 2: Test organization | Week 4-5 | Planned |
| Phase 3: CI/CD integration | Week 6 | Planned |
| Phases 4-10: Implementation | Weeks 7-13 | Planned |
| **v2.0 Release** | **Week 13-14** | **Target** |

---

## 🎓 Learning Path

### To understand FraiseQL structure (1-2 hours)

1. **Architecture** (30 min)
   - `docs/ORGANIZATION.md` - Complete overview
   - Focus on: tiers, modules, design patterns

2. **Your area of interest** (20 min)
   - `src/fraiseql/[module]/STRUCTURE.md`
   - Focus on: components, dependencies, extending

3. **Code standards** (20 min)
   - `docs/CODE_ORGANIZATION_STANDARDS.md`
   - Focus on: what's required, what's forbidden

4. **Tests** (15 min)
   - `docs/TEST_ORGANIZATION_PLAN.md` - Big picture
   - `tests/[type]/[feature]/` - Specific examples

5. **Ready to code** (varies)
   - Follow standards
   - Reference module guide
   - Contribute!

---

## 📝 Version Information

| Item | Details |
|------|---------|
| Documentation Created | January 8, 2026 |
| Total Lines | 3,000+ |
| Files | 10 |
| Phase | 0 (Complete) |
| Target Release | v2.0 |
| Status | ✅ Ready for implementation |

---

## 🙏 Acknowledgments

Created to support FraiseQL v2.0 preparation. Provides:
- Clear architecture navigation
- Consistent code organization
- Sustainable growth patterns
- Professional standards

Used by: Developers, reviewers, project leads, release managers

---

**Last Updated**: January 8, 2026
**Status**: Phase 0 Complete
**Next**: Phase 1 (Week 2-3)

**Start with `docs/ORGANIZATION.md` - it's your map.**
