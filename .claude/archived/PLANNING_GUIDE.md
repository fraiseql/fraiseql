# FraiseQL Planning Guide: Which Document to Read

**Updated**: January 25, 2026 | **Token Efficiency Focus**: Token-minimal planning

---

## 🎯 Quick Start: Which Document to Read?

| Goal | Read This | Time | Tokens |
|------|-----------|------|--------|
| **I need the roadmap NOW** | `FRAISEQL_V2_IMPLEMENTATION_PLAN.md` | 5 min | ⭐ |
| **I'm implementing Phase X** | `.claude/PHASE_X_PLAN.md` | 20 min | ⭐⭐ |
| **I need all technical details** | `FRAISEQL_V2_UNIFIED_ROADMAP.md` | 30 min | ⭐⭐⭐ |
| **I'm starting a new session** | `WORK_STATUS.md` | 10 min | ⭐⭐ |
| **I want cross-language SDK design** | `.claude/PHASE_9_10_PLAN.md` | 15 min | ⭐⭐ |
| **I need to plan implementation details** | Phase-specific plan + Implementation docs | Variable | Variable |

**⭐ = Token cost: ⭐ = <5K tokens, ⭐⭐ = 5-15K tokens, ⭐⭐⭐ = >15K tokens**

---

## 📋 Document Catalog

### Master Documents

#### 1. **FRAISEQL_V2_IMPLEMENTATION_PLAN.md** ⭐ START HERE
- **Purpose**: High-level roadmap, quick reference
- **Size**: ~300 lines, ~12 KB
- **Content**:
  - Current status at a glance
  - Phase quick reference table
  - Current phases (8, 9, 10) summary
  - Key decisions and rationale
  - Immediate next steps
- **When to read**: Every session start, team meetings, sprint planning
- **Best for**: Getting oriented, understanding dependencies

#### 2. **FRAISEQL_V2_UNIFIED_ROADMAP.md** ⭐⭐⭐ HISTORICAL REFERENCE
- **Purpose**: Comprehensive technical reference, decision rationale
- **Size**: ~832 lines, ~35 KB
- **Content**:
  - Detailed architecture diagrams
  - Complete Phase 8, 9 decision history
  - Comparative analysis (Arrow vs alternatives)
  - Question log and discussion notes
  - Timeline with weekly breakdowns
- **When to read**: Understanding design decisions, architectural discussions
- **Best for**: Onboarding, architectural decisions, comprehensive reference

#### 3. **WORK_STATUS.md** ⭐⭐ SESSION CHECKPOINT
- **Purpose**: Current session progress, immediate priorities
- **Size**: ~450 lines, ~20 KB
- **Content**:
  - What's been completed this session
  - Current blockers and priorities
  - Next actions with effort estimates
  - Test results and quality metrics
  - Links to active phase plans
- **When to read**: Start of session, checking progress
- **Best for**: Tracking what's done, understanding blockers

---

### Phase-Specific Documents

These are read when actively implementing a phase.

| Phase | Document | Status | Size | Read When |
|-------|----------|--------|------|-----------|
| **8.6** | `PHASE_8_6_PLAN.md` | Ready to start | 18 KB | Implementing job queue |
| **8.7** | `PHASE_8_7_PLAN.md` | Complete | 14 KB | Understanding metrics setup |
| **9.1-9.8** | `PHASE_9_*.md` | Complete (summaries) | ~100 KB total | Verifying implementation status |
| **9.9** | `PHASE_9_PRERELEASE_TESTING.md` | Current focus | 11 KB | Running pre-release tests |
| **9.10 (NEW)** | `PHASE_9_10_PLAN.md` | Planned | 12 KB | Planning cross-language SDK |

---

## 🔄 Token Optimization Strategy

### Problem
- Large planning documents consume tokens on every read
- 832-line roadmap = ~40-50K tokens per context
- Planning changes → updating everywhere → more tokens

### Solution
- **Compact master plan**: 300 lines (60% smaller)
- **Detailed plans**: Separate files per phase
- **Architecture decisions**: Keep in unified roadmap (historical)
- **Session tracking**: Minimal status file (WORK_STATUS)

### Result
- Typical session reads: 5-15K tokens for planning
- Deep dives: 15-30K tokens when needed
- Savings: 40-50% reduction in planning token costs

---

## 📖 Reading Paths

### 🏃 Fast Path (5 minutes)
1. **FRAISEQL_V2_IMPLEMENTATION_PLAN.md** (Overview)
2. **WORK_STATUS.md** (What changed since last session)
3. **Relevant Phase Plan** (If implementing)

**Token cost**: ~10K

### 🚶 Standard Path (20 minutes)
1. **FRAISEQL_V2_IMPLEMENTATION_PLAN.md** (Overview)
2. **WORK_STATUS.md** (Session progress)
3. **PHASE_X_PLAN.md** (Current phase details)
4. **Skim**: FRAISEQL_V2_UNIFIED_ROADMAP.md (decisions as needed)

**Token cost**: ~20K

### 🧗 Deep Path (45 minutes - Onboarding)
1. **FRAISEQL_V2_IMPLEMENTATION_PLAN.md** (Overview)
2. **WORK_STATUS.md** (Current state)
3. **FRAISEQL_V2_UNIFIED_ROADMAP.md** (Detailed architecture)
4. **All Phase Plans** (Understanding full scope)
5. **Implementation docs** (How things actually work)

**Token cost**: ~50K

### 🔍 Decision Review (30 minutes - Architecture discussion)
1. **FRAISEQL_V2_IMPLEMENTATION_PLAN.md** (High level)
2. **FRAISEQL_V2_UNIFIED_ROADMAP.md** - Section 6: Key Decisions
3. **Decision Log** section (Why this choice vs alternatives)
4. **Relevant Phase Plan** (If affecting phase design)

**Token cost**: ~30K

---

## 🎓 Recommended Document Order

### For Developers
1. Start: FRAISEQL_V2_IMPLEMENTATION_PLAN.md
2. Current work: PHASE_X_PLAN.md
3. Deep understanding: FRAISEQL_V2_UNIFIED_ROADMAP.md (as needed)

### For Architects
1. Start: FRAISEQL_V2_IMPLEMENTATION_PLAN.md
2. Deep dive: FRAISEQL_V2_UNIFIED_ROADMAP.md (full context)
3. Implementation: Relevant PHASE_X_PLAN.md

### For New Team Members
1. First session: FRAISEQL_V2_IMPLEMENTATION_PLAN.md
2. Deep dive: FRAISEQL_V2_UNIFIED_ROADMAP.md
3. Hands-on: Relevant PHASE_X_PLAN.md

### For Project Managers
1. FRAISEQL_V2_IMPLEMENTATION_PLAN.md (roadmap)
2. WORK_STATUS.md (current progress)
3. Relevant PHASE_X_PLAN.md (effort estimates)

---

## 🚀 Special Topics

### Cross-Language Arrow Flight Support (New!)
**Read**: `PHASE_9_10_PLAN.md` (12 KB)
- Design of language-agnostic Arrow SDK
- Code generators for 5 languages
- Timeline and implementation steps

### Pre-Release Testing (Current Priority)
**Read**: `PHASE_9_PRERELEASE_TESTING.md` (11 KB)
- 10-phase comprehensive testing
- Go/no-go criteria
- How to verify production-readiness

### Performance Optimization
**Read**: FRAISEQL_V2_UNIFIED_ROADMAP.md → "Success Metrics" section
- Arrow Flight performance targets
- Benchmarking strategy
- Performance comparison (Arrow vs HTTP/JSON)

### Phase Dependencies
**Read**: FRAISEQL_V2_IMPLEMENTATION_PLAN.md → Phase Quick Reference Table
- Which phases block which phases
- Can phases run in parallel?
- Critical path to production

---

## 📊 Document Statistics

| Document | Lines | Size | Read Time | Key Use |
|----------|-------|------|-----------|---------|
| FRAISEQL_V2_IMPLEMENTATION_PLAN.md | ~300 | 12 KB | 5 min | Quick reference ⭐ |
| WORK_STATUS.md | ~450 | 20 KB | 10 min | Session tracking ⭐⭐ |
| FRAISEQL_V2_UNIFIED_ROADMAP.md | ~832 | 35 KB | 30 min | Deep reference ⭐⭐⭐ |
| PHASE_8_6_PLAN.md | ~500 | 18 KB | 20 min | Implementation guide |
| PHASE_9_10_PLAN.md | ~400 | 12 KB | 15 min | Cross-language design |
| PHASE_9_PRERELEASE_TESTING.md | ~400 | 11 KB | 15 min | Testing guide |
| **Total Planning Docs** | **~3,000** | **~108 KB** | **90 min** | All phases |

---

## 🎯 Key Changes in This Reorganization

### What Changed
1. **Created FRAISEQL_V2_IMPLEMENTATION_PLAN.md**
   - 60% smaller than unified roadmap
   - Focus on tables, quick references
   - Links to detailed docs instead of full content
   - Saves ~25K tokens per read

2. **Created PHASE_9_10_PLAN.md**
   - Addresses "any programming language" requirement
   - Schema IDL, code generators, examples
   - 2-week implementation roadmap

3. **Marked FRAISEQL_V2_UNIFIED_ROADMAP.md as Historical**
   - Still contains all decisions and rationale
   - Reference for architecture discussions
   - Kept for historical record

4. **Updated WORK_STATUS.md**
   - Links to new planning structure
   - Clearer navigation
   - Session-specific tracking

### Why This Matters
- **Token savings**: 40-50% reduction in planning document reading costs
- **Faster onboarding**: Quick reference plan is immediately accessible
- **Better organization**: Separate concerns (overview vs details)
- **Scalability**: Easy to add new phases without bloating master document

---

## ❓ FAQ

### Q: Which document should I read first?
**A**: Always start with **FRAISEQL_V2_IMPLEMENTATION_PLAN.md**. It's the quick reference and tells you what to read next.

### Q: I need to understand a design decision. Where do I look?
**A**: **FRAISEQL_V2_UNIFIED_ROADMAP.md** section "Key Architectural Decisions" + "Decision Log". It has the "why" behind choices.

### Q: I'm implementing Phase 8.6 right now. What do I read?
**A**:
1. Quick reference: FRAISEQL_V2_IMPLEMENTATION_PLAN.md
2. Implementation guide: PHASE_8_6_PLAN.md
3. Context: WORK_STATUS.md (any blockers?)

### Q: How do I know what's blocking what?
**A**: FRAISEQL_V2_IMPLEMENTATION_PLAN.md has a "Phase Quick Reference" table showing dependencies.

### Q: Why is there a new Phase 9.10?
**A**: User requested "ability to express the arrow plane in any programming language". Phase 9.10 adds schema IDL + code generators for 5 languages (Go, Java, C#, Node.js, C++).

### Q: Can I skip the unified roadmap?
**A**: Yes for daily work. No for:
- Architecture discussions
- Understanding historical decisions
- Design reviews
- Onboarding deep dives

---

## 🔗 Quick Links

- 📋 **Quick Reference**: `FRAISEQL_V2_IMPLEMENTATION_PLAN.md`
- 📊 **Session Status**: `WORK_STATUS.md`
- 📖 **Deep Details**: `FRAISEQL_V2_UNIFIED_ROADMAP.md`
- 🔧 **Phase 8.6**: `PHASE_8_6_PLAN.md`
- 🧪 **Phase 9.9 Testing**: `PHASE_9_PRERELEASE_TESTING.md`
- 🌍 **Phase 9.10 Cross-Language**: `PHASE_9_10_PLAN.md`

---

**Start here**: Open `FRAISEQL_V2_IMPLEMENTATION_PLAN.md` and pick your next phase.
