# Fraisier Documentation Index

Quick navigation for all Fraisier development resources.

---

## Essential Reading (Read in Order)

1. **[00_START_HERE.md](00_START_HERE.md)** ← Begin here
   - Overview, current status, quick links
   - ~500 words, 5 min read

2. **[PHASE_1_PROGRESS.md](PHASE_1_PROGRESS.md)**
   - What's been completed this session
   - Metrics and test coverage
   - ~1000 words, 10 min read

3. **[PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md)**
   - What needs to be done for Phase 1 completion
   - Specific task breakdown
   - ~2000 words, 20 min read

4. **[CLAUDE.md](CLAUDE.md)**
   - Development standards and workflow
   - Architecture principles
   - Testing strategy
   - ~2500 words, 25 min read

5. **[TRINITY_PATTERNS.md](TRINITY_PATTERNS.md)**
   - Database schema explanation
   - Multi-database reconciliation
   - ~3500 words, 30 min read

---

## By Topic

### Planning & Status
| Document | Purpose | Length |
|----------|---------|--------|
| [00_START_HERE.md](00_START_HERE.md) | Navigation & overview | 5 min |
| [PHASE_1_PROGRESS.md](PHASE_1_PROGRESS.md) | Session progress report | 10 min |
| [PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md) | Phase 1 task breakdown | 20 min |
| [INDEX.md](INDEX.md) | This file | 2 min |

### Development & Architecture
| Document | Purpose | Length |
|----------|---------|--------|
| [CLAUDE.md](CLAUDE.md) | Development standards | 25 min |
| [TRINITY_PATTERNS.md](TRINITY_PATTERNS.md) | Database architecture | 30 min |
| [../../docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md) | System architecture | 30 min |
| [../../docs/DEPLOYMENT_GUIDE.md](../../docs/DEPLOYMENT_GUIDE.md) | Deployment walkthrough | 20 min |

### Testing & Quality
| Document | Purpose | Length |
|----------|---------|--------|
| [../../docs/TESTING.md](../../docs/TESTING.md) | Testing strategy | 20 min |
| [CLAUDE.md](CLAUDE.md) section on Testing | Testing patterns | 15 min |

---

## By Role

### If You're Starting Fresh

1. Read: [00_START_HERE.md](00_START_HERE.md) (5 min)
2. Read: [PHASE_1_PROGRESS.md](PHASE_1_PROGRESS.md) (10 min)
3. Check: [CLAUDE.md](CLAUDE.md) Development Workflow section (10 min)
4. Start implementing based on [PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md)

### If You're Implementing a Feature

1. Check: [PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md) for task
2. Read: [CLAUDE.md](CLAUDE.md) relevant section (Architecture, Testing, etc.)
3. Reference: [TRINITY_PATTERNS.md](TRINITY_PATTERNS.md) if database-related
4. Look at existing tests in `tests/` for examples
5. Follow: [CLAUDE.md](CLAUDE.md) Code Review Checklist before committing

### If You're Debugging

1. Check: [CLAUDE.md](CLAUDE.md) Troubleshooting section
2. Reference: [TRINITY_PATTERNS.md](TRINITY_PATTERNS.md) Database Inspection section
3. Look at: Relevant test in `tests/` for expected behavior
4. Check: Git history with `git log --oneline`

### If You're Reviewing Code

1. Use: [CLAUDE.md](CLAUDE.md) Code Review Checklist
2. Verify: Test coverage with `pytest --cov`
3. Check: Linting with `ruff check`
4. Reference: [CLAUDE.md](CLAUDE.md) Architecture Principles

---

## Phase 1 Task Tracker

### ✅ Completed

- [x] APIDeployer implementation (100%)
- [x] ETLDeployer implementation (100%)
- [x] ScheduledDeployer implementation (100%)
- [x] Deployer tests (26 tests, 100%)
- [x] FraisierDB implementation (95%+)
- [x] Database tests (24 tests, 100%)
- [x] Git providers (GitHub, GitLab, Gitea, Bitbucket)
- [x] Git provider tests (22 tests, 100%)
- [x] Trinity pattern documentation
- [x] Development guide (CLAUDE.md)

### 🔄 In Progress

- [ ] Webhook handler (FastAPI routes)
- [ ] Webhook handler tests (10 tests planned)

### ⏳ Pending

- [ ] CLI status commands implementation
- [ ] Final Phase 1 verification

See [PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md) for detailed breakdown.

---

## Quick Lookup

### "How do I...?"

| Question | Answer |
|----------|--------|
| ...run tests? | `pytest -v` (see CLAUDE.md Testing Strategy) |
| ...format code? | `ruff format fraisier/` (see CLAUDE.md Code Quality) |
| ...check linting? | `ruff check fraisier/` (see CLAUDE.md Code Quality) |
| ...write a test? | See CLAUDE.md Testing Strategy section |
| ...add a deployer? | See CLAUDE.md → Adding a New Deployer Type |
| ...add a git provider? | See CLAUDE.md → Adding a New Git Provider |
| ...understand the database? | See TRINITY_PATTERNS.md |
| ...deploy something? | See ../../docs/DEPLOYMENT_GUIDE.md |
| ...understand architecture? | See ../../docs/ARCHITECTURE.md |
| ...commit changes? | See CLAUDE.md Code Review Checklist |

---

## Files Explained

### In `.claude/` (Development Guide)

- **00_START_HERE.md** - Navigation and overview (this is your entry point)
- **INDEX.md** - This file (documentation map)
- **CLAUDE.md** - Development standards, architecture, testing
- **PHASE_1_IMPLEMENTATION_PLAN.md** - Detailed task breakdown for Phase 1
- **PHASE_1_PROGRESS.md** - Session progress report
- **TRINITY_PATTERNS.md** - Database architecture guide

### In `docs/` (User Documentation)

- **INDEX.md** - Documentation overview
- **ARCHITECTURE.md** - System architecture
- **DEPLOYMENT_GUIDE.md** - How to deploy
- **TESTING.md** - Testing strategy
- **DEVELOPMENT.md** - Development guide

### In `fraisier/` (Source Code)

- **cli.py** - Command-line interface
- **config.py** - Configuration loading (fraises.yaml)
- **database.py** - SQLite layer with trinity pattern
- **deployers/base.py** - Deployer interface
- **deployers/api.py** - APIDeployer implementation
- **deployers/etl.py** - ETLDeployer implementation
- **deployers/scheduled.py** - ScheduledDeployer implementation
- **deployers/__init__.py** - Deployer exports
- **git/base.py** - Git provider interface
- **git/{github,gitlab,gitea,bitbucket}.py** - Provider implementations
- **git/registry.py** - Provider registry
- **webhook.py** - Webhook handler (in progress)

### In `tests/` (Test Suite)

- **conftest.py** - Shared fixtures
- **test_deployers.py** - Deployer unit tests (26 tests)
- **test_database.py** - Database integration tests (24 tests)
- **test_config.py** - Configuration tests (11 tests)
- **test_git_providers.py** - Git provider tests (22 tests)

---

## Statistics

| Metric | Value |
|--------|-------|
| **Total Tests** | 83+ |
| **Test Coverage** | ~90% |
| **Lines of Code** | ~2000 (implementation) |
| **Lines of Test Code** | ~1300 |
| **Documentation** | ~8000 lines across 6 files |
| **Phase 1 Completion** | 85% |

---

## Navigation

- **Where should I start?** → [00_START_HERE.md](00_START_HERE.md)
- **What's the current status?** → [PHASE_1_PROGRESS.md](PHASE_1_PROGRESS.md)
- **What needs to be done?** → [PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md)
- **How do I develop?** → [CLAUDE.md](CLAUDE.md)
- **How does the database work?** → [TRINITY_PATTERNS.md](TRINITY_PATTERNS.md)
- **Need a quick answer?** → This INDEX.md file

---

**Created**: 2026-01-22
**Updated**: 2026-01-22
**Status**: Documentation system mirrors FraiseQL structure at Fraisier scale
