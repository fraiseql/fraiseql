# Fraisier: START HERE

**Fraisier** is the canonical reference implementation of a FraiseQL application. It demonstrates deployment orchestration with support for multiple fraise types (API, ETL, Scheduled).

**Version**: v0.1.0-phase1 (In Development)

---

## Quick Links

- **What am I working on?** → See [PHASE_1_PROGRESS.md](PHASE_1_PROGRESS.md)
- **What needs to be done?** → See [PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md)
- **How do I develop?** → See [CLAUDE.md](CLAUDE.md)
- **Architecture overview?** → See [ARCHITECTURE.md](../../docs/ARCHITECTURE.md)
- **Database patterns?** → See [TRINITY_PATTERNS.md](TRINITY_PATTERNS.md)

---

## Where Am I?

```
fraiseql/                          ← FraiseQL Framework (Phase 7 - Rust)
├── crates/                        ← Rust engine
├── fraisier-python/               ← Python schema authoring (future)
│
└── fraisier/                       ← YOU ARE HERE (Reference Implementation)
    ├── fraisier/                  ← Python deployment orchestrator
    ├── tests/                     ← Test suite (83+ tests)
    ├── docs/                      ← User documentation
    ├── .claude/                   ← Development guide (this directory)
    └── pyproject.toml
```

**Key Distinction**: Fraisier is **APPLICATION code** using FraiseQL, not framework code.

---

## Current Status

| Phase | Component | Status | Notes |
|-------|-----------|--------|-------|
| **1.1** | APIDeployer | ✅ Complete | Git ops, migrations, rollback, health checks |
| **1.1** | ETLDeployer | ✅ Complete | Script validation, shared code deployment |
| **1.1** | ScheduledDeployer | ✅ Complete | Systemd timer management |
| **1.1** | Deployer Tests | ✅ Complete | 26 comprehensive tests |
| **1.2** | FraisierDB | ✅ Complete | Trinity pattern, CQRS views |
| **1.2** | Database Tests | ✅ Complete | 24 integration tests |
| **1.3** | Git Providers | ✅ Complete | GitHub, GitLab, Gitea, Bitbucket |
| **1.3** | Webhook Handler | 🔄 In Progress | FastAPI routes needed |
| **1.4** | CLI Commands | ⏳ Pending | Status checking implementation |

**Overall**: Phase 1 is **85% complete**. Core infrastructure done, webhook handler + CLI finalization needed.

---

## Getting Started

### 1. Install for Development

```bash
cd fraisier
pip install -e ".[dev]"
```

### 2. Run Tests

```bash
pytest -v                   # All tests
pytest tests/test_deployers.py -v   # Specific test file
```

### 3. Read Key Files

```
In priority order:

1. PHASE_1_PROGRESS.md      ← What's been done this session
2. PHASE_1_IMPLEMENTATION_PLAN.md ← What's left
3. CLAUDE.md                ← How to code/test/commit
4. TRINITY_PATTERNS.md      ← Database schema explanation
```

---

## What's Been Done (This Session)

### Deployers (Phase 1.1)

- ✅ **APIDeployer**: Complete with migrations, rollback, health checks
- ✅ **ETLDeployer**: Complete with rollback via git
- ✅ **ScheduledDeployer**: Complete with systemd timer management
- ✅ **26 tests**: Full coverage for all deployer types

### Database (Phase 1.2)

- ✅ **FraisierDB**: Trinity pattern implementation with UUID, business keys, audit trail
- ✅ **Three views**: v_fraise_status, v_deployment_history, v_webhook_event_history
- ✅ **24 tests**: All CRUD operations verified
- ✅ **Multi-database support**: Prepared for SQLite + PostgreSQL reconciliation

### Git Providers (Phase 1.3)

- ✅ **4 Providers**: GitHub, GitLab, Gitea, Bitbucket
- ✅ **22 tests**: Signature verification + event parsing
- ✅ **Full coverage**: Push, PR, ping events supported

### Documentation

- ✅ **CLAUDE.md**: 400-line development guide
- ✅ **PHASE_1_PROGRESS.md**: Session progress report
- ✅ **TRINITY_PATTERNS.md**: 540-line database architecture guide
- ✅ **PHASE_1_IMPLEMENTATION_PLAN.md**: Detailed implementation plan

---

## What's Next (Immediate)

### Phase 1.3: Webhook Handler

- [ ] Implement FastAPI `/webhook` route
- [ ] Implement `/providers` endpoint (list supported providers)
- [ ] Implement `/health` endpoint
- [ ] Add webhook event → deployer routing logic
- [ ] Write 10+ webhook handler tests

### Phase 1.4: CLI Status Commands

- [ ] Fix `fraisier status` implementation
- [ ] Implement `fraisier deploy` verification
- [ ] Add output formatting with Rich

### Phase 1.5: Final Verification

- [ ] Run full test suite with coverage
- [ ] Ensure all tests pass with no warnings
- [ ] Verify ruff linting passes
- [ ] Commit Phase 1 completion

---

## Key Architecture

```
┌─────────────────┐
│   CLI Commands  │  (fraisier/cli.py)
│ deploy, status, │
│ history, etc.   │
└────────┬────────┘
         │
    ┌────┴────┐
    │          │
┌───▼────┐ ┌──▼─────┐
│Deployer │ │Database │  (fraisier/deployers/, fraisier/database.py)
│Interface│ │(CQRS)   │
└───┬────┘ └──┬──────┘
    │         │
┌───▼─────────▼─┐
│ subprocess,   │  (subprocess, systemd, git, health checks)
│ git, systemd  │
└───────────────┘
```

### Database Pattern: CQRS

```
Write Side (tb_*):

- tb_fraise_state      ← Current state
- tb_deployment        ← History log
- tb_webhook_event     ← Webhook log

Read Side (v_*):

- v_fraise_status      ← Computed state
- v_deployment_history ← Filtered history
- v_webhook_event_history ← Linked webhooks
```

### Trinity Pattern (Column Order)

```sql
-- Every table follows: id → identifier → pk_* → fk_* → domain → audit
CREATE TABLE tb_deployment (
    id TEXT NOT NULL UNIQUE,                    -- 1. Public UUID
    identifier TEXT NOT NULL UNIQUE,            -- 2. Business key
    pk_deployment INTEGER PRIMARY KEY,          -- 3. Internal key
    fk_fraise_state INTEGER REFERENCES ...,    -- 4. Foreign keys
    fraise_name TEXT,                          -- 5. Domain columns
    environment_name TEXT,
    ...,
    created_at TEXT,                           -- 6. Audit trail
    updated_at TEXT
);
```

**Why?** Enables multi-database sync (SQLite local + PostgreSQL cloud).

---

## Development Workflow

### Adding a Feature

```bash
# 1. Create feature branch
git checkout -b feature/description

# 2. Implement feature
# Follow CLAUDE.md for code style
# Write tests alongside code

# 3. Verify quality
pytest -v
ruff check fraisier/
ruff format fraisier/

# 4. Commit
git commit -m "feat(fraisier): Description"

# 5. Push
git push -u origin feature/description
```

### Code Quality Standards

- **Type hints**: Python 3.10+ style (`str | None`, `list[str]`)
- **Docstrings**: All public functions
- **Tests**: 100% coverage for new code
- **Linting**: `ruff check` and `ruff format`
- **Commits**: `feat()`, `fix()`, `test()`, `docs()`, `refactor()` prefixes

---

## Quick Reference

| Task | Command |
|------|---------|
| Run all tests | `pytest -v` |
| Run specific test | `pytest tests/test_deployers.py::test_api_deployer_execute -v` |
| Format code | `ruff format fraisier/` |
| Check linting | `ruff check fraisier/` |
| View test coverage | `pytest --cov=fraisier --cov-report=html` |
| Install dev dependencies | `pip install -e ".[dev]"` |
| View database | `sqlite3 fraisier.db "SELECT * FROM v_fraise_status;"` |

---

## Files to Know

| File | Purpose |
|------|---------|
| `.claude/00_START_HERE.md` | This file - navigation |
| `.claude/CLAUDE.md` | Development guide + code standards |
| `.claude/PHASE_1_PROGRESS.md` | What's been done |
| `.claude/PHASE_1_IMPLEMENTATION_PLAN.md` | What's left |
| `.claude/TRINITY_PATTERNS.md` | Database architecture |
| `fraisier/cli.py` | CLI commands |
| `fraisier/deployers/base.py` | Deployer interface |
| `fraisier/deployers/{api,etl,scheduled}.py` | Deployer implementations |
| `fraisier/database.py` | Database layer |
| `fraisier/git/` | Git provider implementations |
| `tests/test_*.py` | Test suite (83+ tests) |

---

## Questions?

- **"How do I write tests?"** → See CLAUDE.md → Testing Strategy
- **"What code style should I use?"** → See CLAUDE.md → Development Standards
- **"How does the database work?"** → See TRINITY_PATTERNS.md
- **"What needs to be done?"** → See PHASE_1_IMPLEMENTATION_PLAN.md
- **"How's Phase 1 progressing?"** → See PHASE_1_PROGRESS.md

---

**Next**: Read [PHASE_1_PROGRESS.md](PHASE_1_PROGRESS.md) to see what's been accomplished.
