# FraiseQL Python Refactoring Plans - January 11, 2026

## 📂 Contents

This directory contains the complete refactoring plan for transforming FraiseQL's Python layer from "Python does everything" to "Python authors, Rust executes."

### 📄 Documents (Read in This Order)

1. **REFACTORING_PLAN_INDEX.md** ⭐ START HERE
   - Master index and reading guide
   - Quick reference for all topics
   - Navigation helper for different audiences

2. **ARCHITECTURAL_REFACTORING_ANALYSIS.md**
   - Analysis of current architecture
   - Why previous proposal was wrong
   - FFI boundaries clarified
   - PrintOptim compatibility
   - Safe vs unsafe refactoring targets

3. **PYTHON_REFACTORING_EXECUTIVE_SUMMARY.md**
   - High-level overview (30 min read)
   - The opportunity (83% code reduction)
   - Timeline & effort (4-5 months)
   - Risk assessment
   - **Recommendation: Option B (Incremental Deprecation)**

4. **PYTHON_REFACTORING_PLAN.md**
   - Complete strategic roadmap (2 hour read)
   - 6 refactoring phases in detail
   - Module-by-module analysis
   - Implementation strategy options
   - Success criteria
   - Risk mitigation

5. **PHASE_1_DETAILED_ACTION_PLAN.md**
   - Week-by-week execution plan (3 hour read)
   - Daily tasks for first 3 weeks
   - Specific files to audit
   - Deliverables checklist
   - Definition of Done

---

## 🎯 Quick Start

### For Decision Makers (30 minutes)
→ Read: PYTHON_REFACTORING_EXECUTIVE_SUMMARY.md

### For Architects (2 hours)
→ Read: REFACTORING_PLAN_INDEX.md + ARCHITECTURAL_REFACTORING_ANALYSIS.md + PYTHON_REFACTORING_PLAN.md (Parts 1-4)

### For Implementers (3+ hours)
→ Read: All documents, starting with REFACTORING_PLAN_INDEX.md

### For Project Managers (1 hour)
→ Read: PYTHON_REFACTORING_EXECUTIVE_SUMMARY.md + PYTHON_REFACTORING_PLAN.md (Parts 1, 4, 5)

---

## 📊 The Plan Summary

### Current State
- Python: 13MB (467 files) handling both schema AND execution
- Duplication with Rust layer
- Mixed responsibilities

### Target State
- Python: 2.2MB (~100 files) for schema authoring only
- Rust: All execution, compilation, HTTP serving
- Clear separation of concerns

### Recommendation
**Option B: Incremental Deprecation** ⭐
- Timeline: 4-5 months (1 developer, 10-15 hrs/week)
- Risk: Low (gradual, can rollback)
- Start: Week of January 20, 2026

### 6 Phases
1. **Phase 1** (Weeks 1-3): Schema authoring layer
2. **Phase 2** (Weeks 4-9): Eliminate SQL generation
3. **Phase 3** (Weeks 10-13): Eliminate core execution
4. **Phase 4** (Weeks 14-17): Enterprise features
5. **Phase 5** (Weeks 18-19): Integration layers
6. **Phase 6** (Weeks 20-22): Testing & release

---

## ✅ Expected Outcomes

### Code Quality
- 83% code reduction (10.8MB eliminated)
- Zero duplication with Rust layer
- Clear, well-documented APIs

### Performance
- 7-10x faster query execution
- 50% less memory usage
- Zero FFI calls per-request

### Compatibility
- PrintOptim tests: 100% pass
- Gradual migration path
- Can pause at any phase

---

## 🚀 Next Steps

1. **This Week**
   - [ ] Read REFACTORING_PLAN_INDEX.md
   - [ ] Review appropriate documents for your role
   - [ ] Understand the architecture

2. **Next Week**
   - [ ] Approve Option B approach
   - [ ] Schedule Phase 1 kickoff
   - [ ] Begin Phase 1 audit tasks

3. **Week of Jan 20**
   - [ ] Begin Phase 1 implementation
   - [ ] Start Week 1 tasks (audit types/, decorators)
   - [ ] Document current architecture

---

## 📍 Context

These plans are based on:
- **Confirmed Architecture**: Python author → Rust execute model (documented in ADR-001)
- **Verified Design**: CompiledSchema JSON at startup, zero FFI per-request
- **Actual Analysis**: 467 Python files audited and categorized
- **PrintOptim Compatibility**: Verified integration points and migration paths

---

## 💡 Key Principle

> *"Python defines schemas at startup. Rust serves all requests. After start(), Python is irrelevant."*

This refactoring makes that principle manifest in the code structure.

---

## 📞 Questions?

Refer to the specific document:
- Architecture questions → ARCHITECTURAL_REFACTORING_ANALYSIS.md
- Timeline questions → PYTHON_REFACTORING_EXECUTIVE_SUMMARY.md
- Implementation details → PYTHON_REFACTORING_PLAN.md
- Week 1 tasks → PHASE_1_DETAILED_ACTION_PLAN.md
- Navigation help → REFACTORING_PLAN_INDEX.md

---

**Date Created**: January 10, 2026
**Status**: Complete and ready for review
**Recommendation**: Proceed with Option B (Incremental Deprecation)
**Next Action**: Schedule team discussion
