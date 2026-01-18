# Fraisier Action Items

## Summary

Fraisier (deployment orchestrator) currently exists in two locations. This document lists specific actions needed to consolidate it into the FraiseQL monorepo and complete its integration.

---

## PHASE 0: Immediate Consolidation (Do This First)

### Action 0.1: Remove Duplication ⚠️ **CRITICAL**

**Status:** ⏳ TODO
**Priority:** P0 (Blocker)
**Effort:** 5 minutes

**Current Problem:**

- `/home/lionel/code/fraisier/` - standalone repository (original)
- `/home/lionel/code/fraiseql/fraisier/` - copied into monorepo during Phase 1

This creates maintenance burden and confusion about which is authoritative.

**Decision Required:**

**Option A: RECOMMENDED - Keep in Monorepo**

```bash
# Delete standalone (make monorepo the single source of truth)
rm -rf /home/lionel/code/fraisier

# Keep /home/lionel/code/fraiseql/fraisier/ as authoritative
# Update all documentation to point to monorepo location
```

**Option B: Keep Both (Requires CI Sync)**

```bash
# Keep both, but use GitHub Actions to sync changes:
# /home/lionel/code/fraiseql/fraisier/ (source) → /home/lionel/code/fraisier/ (mirror)

# TODO: Set up sync workflow (not recommended, adds complexity)
```

**Recommended Action:** **Option A** - Delete `/home/lionel/code/fraisier/`

### Action 0.2: Verify Monorepo Copy is Complete

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 10 minutes

**Checklist:**

- [ ] `/home/lionel/code/fraiseql/fraisier/README.md` exists ✅
- [ ] `/home/lionel/code/fraiseql/fraisier/pyproject.toml` exists ✅
- [ ] `/home/lionel/code/fraiseql/fraisier/fraises.example.yaml` exists ✅
- [ ] `/home/lionel/code/fraiseql/fraisier/fraisier/` package exists ✅
- [ ] All modules present:
  - [ ] `cli.py` ✅
  - [ ] `webhook.py` ✅
  - [ ] `config.py` ✅
  - [ ] `database.py` ✅
  - [ ] `deployers/*.py` ✅
  - [ ] `git/*.py` ✅

**Action:** If any files missing, copy from `/home/lionel/code/fraisier/`

---

## PHASE 1: Monorepo Integration (This Week)

### Action 1.1: Update Top-Level README

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 15 minutes
**File:** `/home/lionel/code/fraiseql/README.md`

**Changes:**

1. Add section about Fraisier in README
2. Point to `/home/lionel/code/fraiseql/fraisier/` as canonical location
3. Add "Reference Implementation" badge

**Example addition:**

```markdown
## Fraisier - Reference Implementation

Fraisier is the canonical FraiseQL application that demonstrates production deployment patterns.

**Location:** `./fraisier/`
**Type:** Python package (deployment orchestrator)
**Purpose:** Deploy services from Git webhooks

See `fraisier/README.md` for complete documentation.
```

### Action 1.2: Create Integration Documentation

**Status:** ✅ DONE
**Priority:** P1
**Effort:** 2 hours

**Files Created:**

- ✅ `.claude/FRAISIER_INTEGRATION_ANALYSIS.md` - Detailed analysis
- ✅ `.claude/FRAISIER_QUICK_REFERENCE.md` - Quick start guide
- ✅ `.claude/FRAISIER_ACTION_ITEMS.md` - This file

### Action 1.3: Code Quality Verification

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 30 minutes
**Location:** `/home/lionel/code/fraiseql/fraisier/`

```bash
# Run linting
cd /home/lionel/code/fraiseql/fraisier
ruff check .

# Run tests
pytest tests/

# Check type hints
pyright .  # if configured

# Verify package structure
python -m fraisier --help
```

**Expected Results:**

- [ ] Zero linting errors
- [ ] All tests passing
- [ ] CLI working correctly

### Action 1.4: Add Fraisier to .gitignore (if needed)

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 5 minutes

**Check:** Does `/home/lionel/code/fraiseql/.gitignore` exclude fraisier artifacts?

```bash
# Expected in .gitignore:
fraisier/venv/
fraisier/.venv/
fraisier/__pycache__/
fraisier/*.pyc
fraisier/.pytest_cache/
fraisier/fraisier.db
```

---

## PHASE 2: Documentation (Weeks 2-3)

### Action 2.1: Git Provider Setup Guides

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 4 hours (1 hour per provider)
**Location:** `/home/lionel/code/fraiseql/fraisier/docs/`

**Create files:**

#### `docs/setup-github.md` - GitHub & GitHub Enterprise

- [ ] Webhook configuration steps
- [ ] Secret management
- [ ] Self-signed certificate support
- [ ] Testing webhook delivery
- [ ] Screenshots

#### `docs/setup-gitlab.md` - GitLab.com & Self-hosted

- [ ] Webhook configuration steps
- [ ] Self-hosted GitLab setup
- [ ] Secret token configuration
- [ ] Testing webhook delivery

#### `docs/setup-gitea.md` - Gitea / Forgejo

- [ ] Webhook configuration
- [ ] Self-hosted setup
- [ ] Forgejo compatibility
- [ ] Testing

#### `docs/setup-bitbucket.md` - Bitbucket Cloud & Server

- [ ] Webhook configuration
- [ ] Bitbucket Server vs Data Center
- [ ] Bitbucket Cloud setup
- [ ] Testing

### Action 2.2: Deployment Guides

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 3 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/docs/`

**Create files:**

#### `docs/deployment-api.md` - API Service Deployment

- [ ] Systemd configuration
- [ ] Health check setup
- [ ] Database migration integration
- [ ] Rollback procedures
- [ ] Troubleshooting

#### `docs/deployment-etl.md` - ETL Pipeline Deployment

- [ ] Script execution
- [ ] Logging setup
- [ ] Error notifications
- [ ] Monitoring

#### `docs/deployment-scheduled.md` - Scheduled Jobs

- [ ] Systemd timer configuration
- [ ] Cron scheduling
- [ ] Log rotation
- [ ] Monitoring

#### `docs/deployment-backup.md` - Backup Jobs

- [ ] Database backup strategy
- [ ] Remote sync (S3, rsync, etc.)
- [ ] Retention policies
- [ ] Verification

### Action 2.3: Troubleshooting Guide

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 2 hours
**File:** `/home/lionel/code/fraiseql/fraisier/docs/troubleshooting.md`

**Common Issues:**

- [ ] Webhook not triggering
- [ ] Deployment failing
- [ ] Health check timeout
- [ ] Database migration errors
- [ ] Secret/credential issues
- [ ] Network connectivity

---

## PHASE 3: Testing Infrastructure (Weeks 4-5)

### Action 3.1: Create Test Fixtures

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 2 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/tests/fixtures/`

**Create webhook payload examples:**

```python
tests/fixtures/
├── github_webhook_payload.json      # GitHub push event
├── gitlab_webhook_payload.json      # GitLab push event
├── gitea_webhook_payload.json       # Gitea push event
├── bitbucket_webhook_payload.json   # Bitbucket push event
├── fraises_dev.yaml                 # Dev config
├── fraises_prod.yaml                # Production config
└── database_schema.sql              # Test database schema
```

### Action 3.2: Unit Tests

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 4 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/tests/unit/`

**Test Coverage:**

```python
tests/unit/
├── test_config.py              # Config parsing & validation
├── test_database.py            # SQLite operations (CQRS)
├── test_git_github.py          # GitHub provider
├── test_git_gitlab.py          # GitLab provider
├── test_git_gitea.py           # Gitea provider
├── test_git_bitbucket.py       # Bitbucket provider
├── test_deployers_api.py       # API deployment strategy
├── test_deployers_etl.py       # ETL strategy
├── test_deployers_scheduled.py # Scheduled job strategy
└── test_cli.py                 # CLI commands
```

### Action 3.3: Integration Tests

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 6 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/tests/integration/`

**Test Scenarios:**

```python
tests/integration/
├── test_webhook_github.py          # Full webhook→deploy flow (GitHub)
├── test_webhook_gitlab.py          # Full webhook→deploy flow (GitLab)
├── test_webhook_gitea.py           # Full webhook→deploy flow (Gitea)
├── test_webhook_bitbucket.py       # Full webhook→deploy flow (Bitbucket)
├── test_deployment_flow_api.py     # End-to-end API deployment
├── test_deployment_flow_etl.py     # End-to-end ETL deployment
├── test_health_checks.py           # Health check verification
├── test_branch_mapping.py          # Branch→fraise mapping logic
└── test_concurrent_deployments.py  # Multiple deployments in parallel
```

### Action 3.4: Load/Stress Tests

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 2 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/tests/load/`

**Test Scenarios:**

- [ ] 100 concurrent webhook requests
- [ ] Large payload handling
- [ ] Database connection pool under load
- [ ] Memory leak detection
- [ ] Response time under stress

---

## PHASE 4: Schema & Database (Weeks 6-7)

### Action 4.1: Define Fraisier Schema Models

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 3 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/schema/py/`

**Create file:** `models.py`

```python
from fraiseql import type as fraiseql_type
from datetime import datetime
from enum import Enum

class FraiseType(str, Enum):
    API = "api"
    ETL = "etl"
    SCHEDULED = "scheduled"
    BACKUP = "backup"

class DeploymentStatus(str, Enum):
    PENDING = "pending"
    RUNNING = "running"
    SUCCESS = "success"
    FAILED = "failed"

@fraiseql_type
class Fraise:
    id: str
    name: str
    type: FraiseType
    description: str | None

@fraiseql_type
class Environment:
    name: str
    branch: str
    app_path: str
    systemd_service: str | None

@fraiseql_type
class Deployment:
    id: str
    fraise_id: str
    environment: str
    status: DeploymentStatus
    started_at: datetime
    completed_at: datetime | None
    error_message: str | None

@fraiseql_type
class FraiseStatus:
    fraise_id: str
    environment: str
    status: DeploymentStatus
    last_deployed: datetime | None
```

### Action 4.2: Create Database Schema

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 2 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/db/`

**Create files:**

#### `db/schema.sql` - Database structure

```sql
-- Write tables (append-only)
CREATE TABLE IF NOT EXISTS tb_deployments (...)
CREATE TABLE IF NOT EXISTS tb_webhook_events (...)
CREATE TABLE IF NOT EXISTS tb_deployment_attempts (...)

-- Read views (CQRS pattern)
CREATE VIEW IF NOT EXISTS v_fraise_status AS (...)
CREATE VIEW IF NOT EXISTS v_deployment_history AS (...)
CREATE VIEW IF NOT EXISTS v_deployment_stats AS (...)
```

#### `db/functions.sql` - PostgreSQL functions (future)

```sql
-- When using PostgreSQL instead of SQLite
CREATE FUNCTION fn_request_deployment(...) RETURNS TABLE (...)
CREATE FUNCTION fn_complete_deployment(...) RETURNS void
```

### Action 4.3: Document Schema Design

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 1 hour
**File:** `/home/lionel/code/fraiseql/fraisier/docs/schema.md`

**Content:**

- [ ] CQRS pattern explanation
- [ ] Write tables (append-only)
- [ ] Read views (query optimization)
- [ ] Example queries
- [ ] Migration strategy

---

## PHASE 5: API & GraphQL (Weeks 8-9)

### Action 5.1: Create GraphQL Resolvers

**Status:** ⏳ TODO (Depends on Phase 6: fraiseql-server)
**Priority:** P1
**Effort:** 4 hours
**Location:** `/home/lionel/code/fraiseql/fraisier/schema/py/`

**Create file:** `resolvers.py`

```python
# Query resolvers
def fraise(id: str) -> Fraise
def fraise_status(fraise_id: str, environment: str) -> FraiseStatus
def deployment_history(fraise_id: str, limit: int = 10) -> list[Deployment]

# Mutation resolvers
def deploy(fraise_id: str, environment: str, force: bool = False) -> Deployment
def cancel_deployment(deployment_id: str) -> Deployment

# Subscription resolvers
def deployment_status_changed() -> DeploymentStatus
def webhook_received() -> WebhookEvent
```

### Action 5.2: Write GraphQL Queries

**Status:** ⏳ TODO (Depends on Phase 6)
**Priority:** P1
**Effort:** 2 hours

**Example queries:**

```graphql
# Get deployment status
query {
  fraiseStatus(fraiseId: "my_api", environment: "production") {
    status
    lastDeployed
  }
}

# Get deployment history
query {
  deploymentHistory(fraiseId: "my_api", limit: 5) {
    id
    status
    startedAt
    completedAt
    errorMessage
  }
}

# Subscribe to deployment changes
subscription {
  deploymentStatusChanged {
    fraiseId
    environment
    status
  }
}
```

---

## PHASE 6: Integration Testing (Weeks 10-11)

### Action 6.1: E2E Tests with fraiseql-server

**Status:** ⏳ TODO (Depends on Phase 6: fraiseql-server completion)
**Priority:** P1
**Effort:** 6 hours

**Test Scenario:**

1. Start fraiseql-server with Fraisier schema
2. Send webhook event
3. Trigger deployment
4. Query status via GraphQL
5. Verify deployment completed

### Action 6.2: Docker Compose Test Environment

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 2 hours
**File:** `/home/lionel/code/fraiseql/fraisier/docker-compose.test.yml`

**Services:**

- [ ] PostgreSQL (for schema storage)
- [ ] fraiseql-server (running Fraisier schema)
- [ ] Fraisier webhook server
- [ ] Test harness

### Action 6.3: CI/CD Integration

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 3 hours
**File:** `.github/workflows/fraisier-tests.yml`

**Test Pipeline:**

- [ ] Lint with ruff
- [ ] Run unit tests
- [ ] Run integration tests (Docker Compose)
- [ ] Generate coverage report
- [ ] Deploy to staging

---

## PHASE 7: Production Hardening (Weeks 12+)

### Action 7.1: Error Handling & Logging

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 3 hours

**Improvements:**

- [ ] Better error messages
- [ ] Structured logging
- [ ] Error recovery mechanisms
- [ ] Alerting/notifications

### Action 7.2: Security Hardening

**Status:** ⏳ TODO
**Priority:** P1
**Effort:** 3 hours

**Checklist:**

- [ ] Secret management (HashiCorp Vault, AWS Secrets Manager)
- [ ] Rate limiting on webhook endpoint
- [ ] CSRF protection
- [ ] Input validation
- [ ] SQL injection prevention (already using parameterized queries)
- [ ] Audit logging

### Action 7.3: Performance Optimization

**Status:** ⏳ TODO
**Priority:** P2
**Effort:** 2 hours

**Improvements:**

- [ ] Connection pooling
- [ ] Query optimization
- [ ] Cache deployment status
- [ ] Async webhook processing

---

## Summary Table

| Phase | Action | Status | Priority | Effort | Files |
|-------|--------|--------|----------|--------|-------|
| **0** | Remove duplication | ⏳ TODO | P0 | 5 min | - |
| **0** | Verify monorepo copy | ⏳ TODO | P1 | 10 min | - |
| **1** | Update README | ⏳ TODO | P1 | 15 min | README.md |
| **1** | Code quality check | ⏳ TODO | P1 | 30 min | - |
| **2** | Git provider guides (4×) | ⏳ TODO | P1 | 4 hrs | docs/*.md |
| **2** | Deployment guides (4×) | ⏳ TODO | P1 | 3 hrs | docs/*.md |
| **2** | Troubleshooting | ⏳ TODO | P2 | 2 hrs | docs/troubleshooting.md |
| **3** | Test fixtures | ⏳ TODO | P1 | 2 hrs | tests/fixtures/ |
| **3** | Unit tests | ⏳ TODO | P2 | 4 hrs | tests/unit/ |
| **3** | Integration tests | ⏳ TODO | P1 | 6 hrs | tests/integration/ |
| **3** | Load tests | ⏳ TODO | P2 | 2 hrs | tests/load/ |
| **4** | Schema models | ⏳ TODO | P1 | 3 hrs | schema/py/models.py |
| **4** | Database schema | ⏳ TODO | P1 | 2 hrs | db/*.sql |
| **4** | Schema docs | ⏳ TODO | P2 | 1 hr | docs/schema.md |
| **5** | GraphQL resolvers | ⏳ TODO | P1 | 4 hrs | schema/py/resolvers.py |
| **5** | GraphQL queries | ⏳ TODO | P1 | 2 hrs | schema/py/queries.gql |
| **6** | E2E tests | ⏳ TODO | P1 | 6 hrs | tests/e2e/ |
| **6** | Docker Compose | ⏳ TODO | P2 | 2 hrs | docker-compose.test.yml |
| **6** | CI/CD | ⏳ TODO | P2 | 3 hrs | .github/workflows/ |
| **7** | Error handling | ⏳ TODO | P1 | 3 hrs | - |
| **7** | Security | ⏳ TODO | P1 | 3 hrs | - |
| **7** | Performance | ⏳ TODO | P2 | 2 hrs | - |

**Total Effort:** ~60 hours (distributed over 12 weeks)

---

## Quick Start: What to Do First

**This Week (3 hours):**

1. Decide on Option A vs B (consolidation strategy)
2. Execute Action 0.1 (remove duplication)
3. Execute Action 0.2 (verify completeness)
4. Execute Action 1.3 (code quality check)

**Next Week (5 hours):**

1. Action 1.1 (update README)
2. Action 1.4 (update gitignore)
3. Action 2.1 (start provider guides)

**By end of Month 1 (20 hours):**

- Complete Phase 0 & 1 (consolidation & integration)
- Complete Phase 2 (documentation)
- Start Phase 3 (testing)

---

## Dependencies

```
Phase 0 (Consolidation)
  ↓
Phase 1 (Integration)
  ↓
Phase 2 (Documentation) — Can run in parallel with Phase 3
Phase 3 (Testing)       — Depends on Phase 2
  ↓
Phase 4 (Schema & DB) — Depends on Phase 3
  ↓
Phase 5 (API & GraphQL) — Depends on Phase 4 + fraiseql-server (Phase 6)
  ↓
Phase 6 (E2E Testing) — Depends on Phase 5 + fraiseql-server completion
  ↓
Phase 7 (Hardening) — Depends on Phase 6
```

---

**Document Version:** 1.0
**Last Updated:** 2026-01-15
**Maintain this as:** Living document (update as work completes)
