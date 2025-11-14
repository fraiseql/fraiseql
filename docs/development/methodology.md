# Development Methodology Guide

## 🏗️ Phased Development Approach

### Task Complexity Assessment

**Simple Tasks** (Single file, config, basic changes):
- Direct execution
- Minimal planning required
- Quick validation

**Complex Tasks** (Multi-file, architecture, new features):
- **Phased TDD Approach**
- Structured planning
- Disciplined execution cycles

## 🔄 TDD Cycle Methodology

### Phase Structure
Each development phase follows disciplined TDD cycles:

```
┌─────────────────────────────────────────────────────────┐
│ PHASE N: [Phase Objective]                              │
│                                                         │
│ ┌─────────┐  ┌─────────┐  ┌─────────────┐  ┌─────────┐ │
│ │   RED   │─▶│ GREEN   │─▶│  REFACTOR   │─▶│   QA    │ │
│ │ Failing │  │ Minimal │  │ Clean &     │  │ Verify  │ │
│ │ Test    │  │ Code    │  │ Optimize    │  │ Quality │ │
│ └─────────┘  └─────────┘  └─────────────┘  └─────────┘ │
└─────────────────────────────────────────────────────────┘
```

### 🔴 RED Phase
Write failing tests that define the expected behavior:
```bash
# Write specific failing test
uv run pytest path/to/test.py::TestClass::test_new_feature -v

# Expected output: FAILED (expected behavior not implemented)
```

**Focus:**
- Clear test case for specific behavior
- Minimal test scope per cycle
- Document expected failure reason

### 🟢 GREEN Phase
Implement minimal code to make the test pass:
```bash
# Run the specific test
uv run pytest path/to/test.py::TestClass::test_new_feature -v

# Expected output: PASSED (minimal implementation working)
```

**Focus:**
- Simplest possible implementation
- No optimization or cleanup yet
- Just make the test pass

### 🔧 REFACTOR Phase
Clean up and optimize the working code:
```bash
# Run broader test suite to ensure no regressions
uv run pytest path/to/related_tests/ -v

# Full test suite for confidence
uv run pytest
```

**Focus:**
- Improve code structure
- Follow project patterns
- Maintain all passing tests
- Performance optimization

### ✅ QA Phase
Verify overall quality and integration:
```bash
# Run complete test suite
uv run pytest --tb=short

# Run linting and type checking
uv run ruff check
uv run mypy

# Integration verification
make test
```

**Focus:**
- All tests passing
- Code quality standards met
- Integration working correctly
- Ready for next phase or completion

## 📋 Phase Planning Template

### Complex Task Structure
```markdown
# [Task Title] - COMPLEX

**Complexity**: Complex | **Phased TDD Approach**

## Executive Summary
[2-3 sentence overview of the feature/change]

## PHASES

### Phase 1: [Phase Name]
**Objective**: [Clear phase goal]

#### TDD Cycle:
1. **RED**: Write failing test for [specific behavior]
   - Test file: [path]
   - Expected failure: [what should fail]

2. **GREEN**: Implement minimal code to pass
   - Files to modify: [paths]
   - Minimal implementation: [what to add]

3. **REFACTOR**: Clean up and optimize
   - Code improvements: [what to clean]
   - Pattern compliance: [follow project conventions]

4. **QA**: Verify phase completion
   - [ ] All tests pass
   - [ ] Code quality maintained
   - [ ] Integration working

### Phase 2: [Next Phase]
[Same TDD cycle structure]

## Success Criteria
- [ ] All tests pass
- [ ] Follows project patterns
- [ ] Performance acceptable
- [ ] Integration complete
- [ ] Documentation updated
```

## 🎯 Development Principles

### Discipline Over Speed
- **Never skip phases** - Each phase builds confidence
- **One cycle at a time** - Complete RED/GREEN/REFACTOR/QA before moving
- **Test-driven decisions** - Tests guide implementation choices
- **Refactor with confidence** - Comprehensive test coverage enables safe changes

### Quality Gates
- **RED**: Test fails as expected (validates test logic)
- **GREEN**: Minimal implementation passes (validates approach)
- **REFACTOR**: Code improved without breaking tests (validates architecture)
- **QA**: Full integration works (validates completion)

### Iteration Strategy
- **Small cycles** - Each RED/GREEN/REFACTOR cycle should be < 30 minutes
- **Clear objectives** - Each phase has specific, measurable goals
- **Continuous validation** - Tests run at every step
- **Progressive complexity** - Build from simple to complex functionality

## 🚀 Benefits of This Methodology

1. **Confidence**: Every change is validated by tests
2. **Speed**: Structured approach prevents waste and rework
3. **Quality**: Refactoring phase ensures clean, maintainable code
4. **Predictability**: Phases provide clear progress milestones
5. **Risk Reduction**: Early validation prevents late-stage surprises

## 🧪 Testing Strategy

### Test Categories
```bash
uv run pytest --tb=short -v                    # Standard test run
uv run pytest --cov=src                        # Coverage verification
uv run pytest -k "test_specific_feature"       # Targeted testing
uv run pytest tests/unit/                      # Unit tests only
uv run pytest tests/integration/               # Integration tests only
```

### Quality Verification
- Run tests at every phase transition
- Maintain test coverage above project standards
- Use tests to document expected behavior
- Refactor tests along with implementation code

---

# 📊 Maestro Analytics Database

## 🎯 Purpose
The Maestro project includes a comprehensive SQLite analytics database that tracks development iterations, assessments, and progress toward the $100M+ multi-language code generation vision.

## 🗄️ Database Location
**Path**: `database/maestro_analytics.db`
**Schema**: `database/maestro_analytics.sql`
**API**: `database/analytics_db.py`
**CLI**: `database/analytics_cli.py`

## 📈 Quick Dashboard Access
```bash
# Show current status dashboard (either command works)
./analytics dashboard
# OR: python database/analytics_cli.py dashboard

# Example output:
📊 Maestro Analytics Dashboard
🎯 Active Assessments: 1
  • Multi-Language Code Generation Implementation Analysis (65% complete, priority: 10)
🚀 Current Iteration: Universal AST Foundation
📝 Action Items: 3 todo (Domain Parser, AST Bridge, Validation)
📈 Recent Progress: 6,173 lines, 65% completion, 1 language supported
```

## 🔍 Efficient Context Retrieval

### **For Claude Sessions**: Use these queries to quickly understand project context instead of reading multiple files:

#### Current Status Overview
```bash
sqlite3 database/maestro_analytics.db "
SELECT
    title,
    current_completion_percentage,
    priority_score,
    findings,
    gaps_identified
FROM assessments
WHERE status = 'active'
ORDER BY priority_score DESC;"
```

#### Active Development Focus
```bash
sqlite3 database/maestro_analytics.db "
SELECT
    phase_name,
    objectives,
    status,
    start_time
FROM iterations
WHERE status = 'active';"
```

#### Outstanding Action Items
```bash
sqlite3 database/maestro_analytics.db "
SELECT
    title,
    description,
    priority,
    estimated_hours,
    status
FROM action_items
WHERE status != 'completed'
ORDER BY priority DESC;"
```

#### Recent Progress Metrics
```bash
sqlite3 database/maestro_analytics.db "
SELECT
    metric_name,
    metric_value,
    metric_unit,
    timestamp
FROM progress_metrics
ORDER BY timestamp DESC
LIMIT 10;"
```

#### Strategic Decisions History
```bash
sqlite3 database/maestro_analytics.db "
SELECT
    title,
    chosen_path,
    reasoning,
    confidence_level,
    timestamp
FROM decisions
ORDER BY timestamp DESC
LIMIT 5;"
```

## 🎯 Key Tables for Context

### **assessments** - Strategic vision and gap analysis
- `current_completion_percentage` - Overall project completion (currently 65%)
- `gaps_identified` - What's missing for multi-language generation
- `priority_score` - Strategic importance (1-10)

### **iterations** - Development cycles
- `phase_name` - Current development phase
- `objectives` - What this iteration aims to achieve
- `status` - 'active', 'completed', 'planning'

### **action_items** - Specific tasks
- `title` - Task description
- `estimated_hours` - Time estimate
- `status` - 'todo', 'in_progress', 'completed', 'blocked'

### **progress_metrics** - Quantitative progress
- `codebase_lines` - Lines of code (currently 6,173)
- `completion_percentage` - Feature completion
- `languages_supported` - Target languages implemented

### **decisions** - Architectural choices
- `chosen_path` - What was decided
- `reasoning` - Why this choice was made
- `confidence_level` - How confident in decision (1-10)

## 💡 Usage Tips for Claude

1. **Start sessions with**: `./analytics dashboard`
2. **Check specific context**: Use the SQL queries above
3. **Record new findings**: Use `./analytics assess` or `./analytics record-decision`
4. **Track progress**: `./analytics complete-action --id N` as work completes

This database provides much more efficient context retrieval than reading multiple markdown files, and maintains a living history of the project's evolution toward the multi-language generation moat.

---
*Phased TDD Development Methodology*
*Focus: Discipline • Quality • Predictable Progress*
