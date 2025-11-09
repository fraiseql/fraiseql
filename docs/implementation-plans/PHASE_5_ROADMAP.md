# Phase 5: Visual Implementation Roadmap

**Quick Navigation**: [Summary](./PHASE_5_SUMMARY.md) | [Detailed Plan](./PHASE_5_DETAILED_IMPLEMENTATION_PLAN.md) | [Checklist](./PHASE_5_PROGRESS_CHECKLIST.md)

---

## 🗺️ Implementation Journey

```
START HERE
    ↓
┌─────────────────────────────────────────────────────────────┐
│ 📖 PREPARATION (30 minutes)                                 │
│                                                             │
│ □ Read PHASE_5_SUMMARY.md                                  │
│ □ Read PHASE_5_DETAILED_IMPLEMENTATION_PLAN.md             │
│ □ Setup test database with SpecQL schema                   │
│ □ Verify Phases 1-4 complete                               │
│ □ Understand: READ-ONLY introspection                      │
└─────────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────────┐
│ 🔴🟢🔧✅ PHASE 5.1: Composite Type Introspection (2-3 hrs) │
│                                                             │
│ Objective: Query PostgreSQL for composite types            │
│                                                             │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔴 RED (15-20 min)                                      │ │
│ │ • Write test_discover_composite_type()                  │ │
│ │ • Verify FAILURE                                        │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🟢 GREEN (30-40 min)                                    │ │
│ │ • Add CompositeAttribute dataclass                      │ │
│ │ • Add CompositeTypeMetadata dataclass                   │ │
│ │ • Implement discover_composite_type()                   │ │
│ │ • Verify PASS                                           │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔧 REFACTOR (20-30 min)                                 │ │
│ │ • Run linters                                           │ │
│ │ • Add docstrings                                        │ │
│ │ • Add logging                                           │ │
│ │ • Tests still PASS                                      │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ ✅ QA (15-20 min)                                       │ │
│ │ • All introspection tests pass                          │ │
│ │ • Manual test with real database                        │ │
│ │ • No breaking changes                                   │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ ✓ Deliverables: discover_composite_type() method           │
│ ✓ Test: test_postgres_introspector.py                      │
└─────────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────────┐
│ 🔴🟢🔧✅ PHASE 5.2: Field Metadata Parsing (1-2 hrs)       │
│                                                             │
│ Objective: Parse @fraiseql:field annotations               │
│                                                             │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔴 RED (10-15 min)                                      │ │
│ │ • Write test_parse_field_annotation_basic()             │ │
│ │ • Write edge case tests                                 │ │
│ │ • Verify FAILURE                                        │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🟢 GREEN (25-35 min)                                    │ │
│ │ • Add FieldMetadata dataclass                           │ │
│ │ • Implement parse_field_annotation()                    │ │
│ │ • Verify PASS                                           │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔧 REFACTOR (15-20 min)                                 │ │
│ │ • Improve parsing for edge cases                        │ │
│ │ • Add error handling                                    │ │
│ │ • Tests still PASS                                      │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ ✅ QA (10-15 min)                                       │ │
│ │ • All metadata tests pass                               │ │
│ │ • Handles malformed annotations                         │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ ✓ Deliverables: parse_field_annotation() method            │
│ ✓ Test: test_metadata_parser.py                            │
└─────────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────────┐
│ 🔴🟢🔧✅ PHASE 5.3: Input Generation (2-3 hrs)             │
│                                                             │
│ Objective: Generate GraphQL inputs from composite types    │
│                                                             │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔴 RED (15-20 min)                                      │ │
│ │ • Write test_generate_input_from_composite_type()       │ │
│ │ • Write test_generate_input_from_parameters_legacy()    │ │
│ │ • Verify FAILURE                                        │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🟢 GREEN (40-50 min)                                    │ │
│ │ • Implement _find_jsonb_input_parameter()               │ │
│ │ • Implement _extract_composite_type_name()              │ │
│ │ • Implement _generate_from_composite_type()             │ │
│ │ • Update generate_input_type() signature               │ │
│ │ • Verify PASS                                           │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔧 REFACTOR (20-30 min)                                 │ │
│ │ • Extract magic strings to constants                    │ │
│ │ • Add comprehensive error handling                      │ │
│ │ • Tests still PASS                                      │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ ✅ QA (15-20 min)                                       │ │
│ │ • All input generator tests pass                        │ │
│ │ • Falls back to parameter-based                         │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ ✓ Deliverables: Composite type-based input generation      │
│ ✓ Test: test_input_generator.py                            │
└─────────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────────┐
│ 🔴🟢🔧✅ PHASE 5.4: Context Param Detection (1-2 hrs)      │
│                                                             │
│ Objective: Auto-detect context parameters from function    │
│                                                             │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔴 RED (10-15 min)                                      │ │
│ │ • Write test_extract_context_params_new_convention()    │ │
│ │ • Write test_extract_context_params_legacy()            │ │
│ │ • Verify FAILURE                                        │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🟢 GREEN (20-30 min)                                    │ │
│ │ • Implement _extract_context_params()                   │ │
│ │ • Update generate_mutation_for_function()               │ │
│ │ • Update AutoDiscovery to pass introspector             │ │
│ │ • Verify PASS                                           │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔧 REFACTOR (15-20 min)                                 │ │
│ │ • Run linters and type checking                         │ │
│ │ • Tests still PASS                                      │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ ✅ QA (10-15 min)                                       │ │
│ │ • All mutation generator tests pass                     │ │
│ │ • Context params correctly detected                     │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ ✓ Deliverables: Auto context parameter detection           │
│ ✓ Test: test_mutation_generator.py                         │
└─────────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────────┐
│ 🔴🟢🔧✅ PHASE 5.5: E2E Testing (2-3 hrs)                  │
│                                                             │
│ Objective: Verify end-to-end with real SpecQL schema       │
│                                                             │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔴 RED (20-30 min)                                      │ │
│ │ • Create tests/fixtures/specql_test_schema.sql          │ │
│ │ • Apply to test database                                │ │
│ │ • Run integration test                                  │ │
│ │ • Verify FAILURE (or skip)                              │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🟢 GREEN (30-40 min)                                    │ │
│ │ • Fix integration issues                                │ │
│ │ • Ensure async/await consistency                        │ │
│ │ • Verify PASS                                           │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 🔧 REFACTOR (20-30 min)                                 │ │
│ │ • Add caching (optional)                                │ │
│ │ • Improve error messages                                │ │
│ │ • Tests still PASS                                      │ │
│ └─────────────────────────────────────────────────────────┘ │
│    ↓                                                         │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ ✅ QA (30-40 min)                                       │ │
│ │ • Run full test suite                                   │ │
│ │ • Run linting and type checking                         │ │
│ │ • Manual validation against PrintOptim                  │ │
│ │ • Performance acceptable                                │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ ✓ Deliverables: Full E2E integration working               │
│ ✓ Test: test_composite_type_generation_integration.py      │
└─────────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────────┐
│ ✅ FINAL VALIDATION                                         │
│                                                             │
│ Run this command:                                           │
│ uv run pytest --tb=short && \                              │
│ uv run ruff check && \                                     │
│ uv run mypy && \                                           │
│ DATABASE_URL="postgresql://localhost/printoptim" \          │
│   python examples/test_phase_5_complete.py                 │
│                                                             │
│ Expected: All green ✅                                      │
└─────────────────────────────────────────────────────────────┘
    ↓
🎉 PHASE 5 COMPLETE! 🎉
    ↓
┌─────────────────────────────────────────────────────────────┐
│ 🚀 PRODUCTION DEPLOYMENT                                    │
│                                                             │
│ □ Update CHANGELOG.md                                      │
│ □ Update README.md                                         │
│ □ Merge to development branch                              │
│ □ Deploy to staging                                        │
│ □ Monitor performance                                      │
│ □ Celebrate! 🎊                                            │
└─────────────────────────────────────────────────────────────┘
```

---

## 📊 Time Distribution

```
Phase 5.1: Composite Type Introspection      ████████░░ 2-3 hours (25%)
Phase 5.2: Field Metadata Parsing            ████░░░░░░ 1-2 hours (15%)
Phase 5.3: Input Generation                  ████████░░ 2-3 hours (25%)
Phase 5.4: Context Parameter Detection       ████░░░░░░ 1-2 hours (15%)
Phase 5.5: E2E Testing                       ████████░░ 2-3 hours (20%)
                                             ──────────
                                             Total: 8-12 hours
```

**Spread over 2-3 weeks** for disciplined TDD development with proper testing and validation.

---

## 🎯 Success Checkpoints

```
☐ Phase 5.1 Complete
    ├─ ✅ discover_composite_type() works
    ├─ ✅ Tests pass
    ├─ ✅ Linting passes
    └─ ✅ Manual test succeeds

☐ Phase 5.2 Complete
    ├─ ✅ parse_field_annotation() works
    ├─ ✅ Handles edge cases
    ├─ ✅ Tests pass
    └─ ✅ Linting passes

☐ Phase 5.3 Complete
    ├─ ✅ Composite type detection works
    ├─ ✅ Falls back to parameter-based
    ├─ ✅ Tests pass
    └─ ✅ Linting passes

☐ Phase 5.4 Complete
    ├─ ✅ Context params auto-detected
    ├─ ✅ Supports legacy patterns
    ├─ ✅ Tests pass
    └─ ✅ Linting passes

☐ Phase 5.5 Complete
    ├─ ✅ E2E tests pass
    ├─ ✅ PrintOptim validation succeeds
    ├─ ✅ All tests pass
    └─ ✅ Performance acceptable

☑ PHASE 5 COMPLETE ✅
```

---

## 🔄 Daily Development Flow

### Morning Session (2-3 hours)
```
9:00 AM  - Review progress from previous day
9:15 AM  - Choose next phase
9:20 AM  - 🔴 RED: Write failing test
9:40 AM  - 🟢 GREEN: Implement minimal code
10:20 AM - Break (10 min)
10:30 AM - 🔧 REFACTOR: Clean up code
11:00 AM - ✅ QA: Verify quality
11:30 AM - Commit phase
11:45 AM - Update progress checklist
12:00 PM - Lunch
```

### Afternoon Session (Optional - 1-2 hours)
```
2:00 PM  - Review morning work
2:15 PM  - Start next phase or continue refactoring
3:00 PM  - Integration testing
3:30 PM  - Documentation updates
4:00 PM  - End of day
```

**Key Principle**: One phase per session. Never rush.

---

## 🚨 Stop Points - When to Pause

**STOP and review if**:
- ❌ Tests not passing after 30 minutes of debugging
- ❌ Don't understand what the code is doing
- ❌ Unsure about architecture decision
- ❌ Breaking existing functionality
- ❌ Performance degrades significantly

**Action**:
1. Review detailed plan
2. Check related documentation
3. Ask questions (create GitHub issue)
4. Take a break and come back fresh

**Never proceed if you're stuck** - quality over speed.

---

## 📚 Quick Reference

### Documentation
- **Overview**: [PHASE_5_SUMMARY.md](./PHASE_5_SUMMARY.md) (5 min read)
- **Detailed**: [PHASE_5_DETAILED_IMPLEMENTATION_PLAN.md](./PHASE_5_DETAILED_IMPLEMENTATION_PLAN.md) (30 min read)
- **Checklist**: [PHASE_5_PROGRESS_CHECKLIST.md](./PHASE_5_PROGRESS_CHECKLIST.md) (track progress)

### Architecture
- [Rich Type System](../architecture/README_RICH_TYPES.md)
- [SpecQL Boundaries](../architecture/SPECQL_FRAISEQL_BOUNDARIES.md)

### Testing
```bash
# Unit tests
uv run pytest tests/unit/introspection/ -v

# Integration tests
uv run pytest tests/integration/introspection/ -v

# Full suite
uv run pytest --tb=short

# Linting
uv run ruff check

# Type checking
uv run mypy
```

---

## 💡 Tips for Success

### Planning
1. ✅ Read all documentation before starting
2. ✅ Understand the full picture
3. ✅ Break work into small chunks
4. ✅ One phase at a time

### Implementation
1. ✅ Write test first (RED)
2. ✅ Simplest code to pass (GREEN)
3. ✅ Clean up after (REFACTOR)
4. ✅ Verify quality (QA)

### Testing
1. ✅ Run tests frequently
2. ✅ Test at every step
3. ✅ Don't skip integration tests
4. ✅ Validate with real data

### Quality
1. ✅ Lint continuously
2. ✅ Type check always
3. ✅ Document thoroughly
4. ✅ Commit after each phase

---

## 🎓 Learning Outcomes

After completing Phase 5, you will have mastered:

- ✅ PostgreSQL system catalog introspection
- ✅ Composite type discovery and metadata extraction
- ✅ Dynamic Python class generation
- ✅ GraphQL schema auto-generation
- ✅ Context parameter pattern recognition
- ✅ Disciplined TDD methodology
- ✅ Integration testing with real databases
- ✅ Production-quality code development

**Skills gained**: Database introspection, meta-programming, TDD, production engineering

---

## 🏁 Final Destination

**You are here**: 📍 START

**You will be here**: 🎯 FINISH

```
Phase 5.1 ──┐
Phase 5.2 ──┼──> Production-Ready
Phase 5.3 ──┤    Composite Type
Phase 5.4 ──┤    Introspection
Phase 5.5 ──┘    System
```

**Impact**:
- ✅ Zero manual code for SpecQL mutations
- ✅ Rich semantic types auto-discovered
- ✅ Context params auto-detected
- ✅ 100x faster development
- ✅ **Competitive moat established**

---

**Ready to start?** → [Begin with Phase 5.1](./PHASE_5_DETAILED_IMPLEMENTATION_PLAN.md#phase-51-composite-type-introspection)

**Questions?** → [See FAQ in Detailed Plan](./PHASE_5_DETAILED_IMPLEMENTATION_PLAN.md#common-issues-and-solutions)

**Track progress** → [Use Progress Checklist](./PHASE_5_PROGRESS_CHECKLIST.md)

---

**Discipline • Quality • Predictable Progress**
