# Fraisier Integration Analysis

## Executive Summary

**Fraisier** is a deployment orchestration platform for the FraiseQL ecosystem. Currently existing as a standalone repository at `/home/lionel/code/fraisier`, it should be integrated into the main FraiseQL monorepo as the **canonical reference implementation** of FraiseQL.

**Current Status:**
- ✅ Fraisier exists as a separate Python project with full CLI and webhook infrastructure
- ✅ Fraisier is referenced in the FraiseQL README as "THE canonical FraiseQL application"
- ⏳ Fraisier code is not yet integrated into the main `/home/lionel/code/fraiseql` monorepo
- ⚠️ Both repositories have identical copies of the same Fraisier code

**Integration Type:** Reference Implementation (should live in `fraiseql/fraisier/` as the primary example)

---

## What is Fraisier?

### Vision

Fraisier is a **deployment orchestrator** for any service deployed from any Git provider to any target. It's the "strawberry plant" that manages the growth and deployment of "fraises" (services).

### Core Responsibilities

| Responsibility | Details |
|---|---|
| **Configuration Management** | YAML-based `fraises.yaml` defining all deployable services |
| **Git Integration** | Support for GitHub, GitLab, Gitea, Bitbucket (self-hosted or cloud) |
| **Webhook Handling** | Automatic deployment triggers on push events |
| **Deployment Orchestration** | Execute deployment workflows (build, migrate, deploy, health check) |
| **State Management** | SQLite database tracking deployment history |
| **Status Monitoring** | Health checks and deployment status reporting |
| **Multi-environment** | Development, staging, production with branch mapping |

### Fraise Types Supported

| Type | Purpose | Example |
|------|---------|---------|
| `api` | Web services & APIs | GraphQL servers, REST APIs |
| `etl` | Data pipelines | Batch processing, data transformation |
| `scheduled` | Cron jobs & timers | Reports, cleanup, maintenance |
| `backup` | Backup jobs | Database backups, cloud sync |

### Technology Stack

```toml
# Core dependencies
fastapi           # Web framework for webhook server
uvicorn           # ASGI server
pyyaml            # Configuration parsing
click             # CLI framework
rich              # Terminal UI
requests          # HTTP client for Git APIs
strawberry-graphql # Optional: GraphQL support for schema authoring
```

---

## Current Repository Structure

### Standalone Fraisier (existing at `/home/lionel/code/fraisier/`)

```
fraisier/
├── README.md                # Comprehensive documentation
├── pyproject.toml           # Python package config (uv)
├── fraises.example.yaml     # Complete example configuration
├── docs/                    # Additional documentation
├── fraisier/                # Python package
│   ├── __init__.py
│   ├── cli.py              # Click CLI implementation
│   ├── config.py           # YAML configuration parsing
│   ├── database.py         # SQLite state management
│   ├── webhook.py          # Git webhook server (FastAPI)
│   ├── deployers/          # Deployment strategy implementations
│   │   ├── api_deployer.py
│   │   ├── etl_deployer.py
│   │   ├── scheduled_deployer.py
│   │   └── backup_deployer.py
│   └── git/                # Git provider abstractions
│       ├── github.py
│       ├── gitlab.py
│       ├── gitea.py
│       └── bitbucket.py
```

### FraiseQL Monorepo Structure (at `/home/lionel/code/fraiseql/`)

```
fraiseql/
├── crates/                 # Rust implementation
│   ├── fraiseql-cli/       # Phase 9: Compiler CLI
│   ├── fraiseql-core/      # Phase 1-5: Engine core
│   └── fraiseql-server/    # Phase 6: HTTP server
├── fraiseql-python/        # Phase 8: Python schema authoring
├── fraiseql-typescript/    # Phase 8: TypeScript schema authoring
├── fraiseql-php/           # Phase 6: PHP schema authoring
├── fraiseql-go/            # Phase 5: Go schema authoring
├── fraiseql-java/          # Phase 7: Java schema authoring
└── fraisier/               # ← CURRENTLY EMPTY (copies placed here during Phase 1)
    ├── README.md           # ← Placed but identical to standalone
    ├── pyproject.toml      # ← Placed but identical to standalone
    ├── fraises.example.yaml # ← Placed but identical to standalone
    └── fraisier/           # ← Placed Python package
```

### Current Issue

**There are now TWO copies of identical Fraisier code:**

1. **Standalone:** `/home/lionel/code/fraisier/` (original)
2. **Monorepo copy:** `/home/lionel/code/fraiseql/fraisier/` (duplicated during Phase 1)

**This is problematic because:**
- Changes to one won't be reflected in the other
- It's unclear which is the "source of truth"
- Maintenance burden is doubled
- The monorepo integration is incomplete

---

## Integration Plan

### Phase: Consolidate Fraisier into FraiseQL Monorepo

**Objective:** Make `/home/lionel/code/fraiseql/fraisier/` the authoritative reference implementation, with the standalone directory becoming optional/deprecated.

#### Integration Strategy

**Option A: Keep Fraisier in Monorepo (RECOMMENDED)**

```bash
# Current state after Phase 1
/home/lionel/code/fraiseql/fraisier/      # ← Keep as source of truth
/home/lionel/code/fraisier/               # ← Can be removed or kept as mirror
```

**Advantages:**
- Single source of truth
- Fraisier dependencies managed in monorepo workspace
- Easy to update with Rust engine changes
- E2E tests can run against both fraiseql-server and fraisier

**Disadvantages:**
- Breaking change for anyone using `/home/lionel/code/fraisier/` directly
- Need to update docs to point to monorepo location

**Option B: Mirror Both Repositories**

Keep both repositories in sync via CI/CD:
- Monorepo as source of truth
- Standalone as published package
- GitHub Actions to sync changes

**Option C: Make Standalone the Primary**

Keep `/home/lionel/code/fraisier/` as the primary, symlink in monorepo:
- Avoids duplication
- Preserves existing workflow
- But contradicts "monorepo as single source"

---

## Integration Points with FraiseQL Core

### 1. **Schema Authoring Integration**

Fraisier uses FraiseQL schemas to define its own API surface:

```python
# fraiseql/fraisier/schema/py/fraisier_schema.py (NOT YET CREATED)
@fraiseql.type
class FraiseStatus:
    """Status of a deployed fraise"""
    fraise_id: str
    environment: str
    status: str  # running, stopped, error
    last_deployed: datetime
    version: str

@fraiseql.type
class DeploymentEvent:
    """A deployment event in the history"""
    id: str
    fraise_id: str
    environment: str
    status: str
    started_at: datetime
    completed_at: datetime | None
    error_message: str | None
```

**TODO:** Create Fraisier schema definitions using `@fraiseql` decorators

### 2. **Deployment Workflow Integration**

Fraisier's deployment process:

```bash
1. Git Webhook Event
   ↓
2. Parse webhook (github.py, gitlab.py, etc.)
   ↓
3. Load config from fraises.yaml
   ↓
4. Record event to SQLite database
   ↓
5. Execute deployment:
   a. Build/compile phase
   b. Database migration (confiture)
   c. Compile GraphQL (fraiseql-cli compile)
   d. Start service (fraiseql-server)
   e. Health check
   ↓
6. Update deployment history
```

**Integration points:**
- Uses `fraiseql-cli` for schema compilation
- Uses `fraiseql-server` for runtime
- Orchestrates `confiture` for database migrations
- Stores results in SQLite (CQRS pattern)

### 3. **Configuration Management**

**Current:** YAML-based (`fraises.yaml`)

**Possible enhancement:** Could use FraiseQL schema as config schema:

```python
@fraiseql.type
class FraisesConfig:
    """Fraisier configuration schema"""
    git: GitConfig
    fraises: dict[str, Fraise]
    environments: dict[str, EnvironmentConfig]
```

But this is **optional** - YAML is fine and simpler for non-developers.

### 4. **Database Schema**

Fraisier uses SQLite with CQRS pattern:

```
Write Tables:
- tb_deployment_events     # Record every deployment
- tb_webhook_events        # Record webhook invocations
- tb_git_commits           # Track commits per fraise

Read Views:
- v_fraise_status          # Current status of each fraise
- v_deployment_history     # Full history with filtering
- v_deployment_stats       # Statistics and aggregates
```

**Integration point:** Fraisier could use FraiseQL to serve its own deployment status API!

```sql
-- fraiseql/fraisier/db/functions/fn_get_fraise_status.sql
CREATE FUNCTION fn_get_fraise_status(fraise_id TEXT, env TEXT)
RETURNS TABLE AS (
    SELECT id, fraise_id, environment, status, last_deployed
    FROM v_fraise_status
    WHERE fraise_id = $1 AND environment = $2
) LANGUAGE SQL;

-- Then expose via FraiseQL schema:
@fraiseql.type
class FraiseStatus:
    def resolve_current_status(self) -> Status:
        # Calls v_fraise_status view via fraiseql-server
        ...
```

### 5. **Testing Integration**

FraiseQL test suite should include:
- Unit tests for Fraisier components (pytest)
- E2E tests that deploy via Fraisier to local fraiseql-server
- Load tests of the webhook server

```bash
# E2E test example
tests/
├── integration/
│   └── fraisier/
│       ├── test_github_webhook.py
│       ├── test_gitlab_webhook.py
│       ├── test_deployment_flow.py
│       └── test_api_status.py
```

---

## Current Phase Dependencies

### What Fraisier Needs from FraiseQL Core

| FraiseQL Component | Required | Status | Notes |
|---|---|---|---|
| **fraiseql-cli** | ✅ YES | ⏳ Phase 9 | Compiler: `fraiseql-cli compile schema.json` |
| **fraiseql-server** | ✅ YES | ⏳ Phase 6 | Runtime: HTTP server for compiled schema |
| **fraiseql-python** | ✅ YES | ✅ Phase 8 | Schema authoring with `@fraiseql` decorators |
| **fraiseql-core** | ✅ YES | ✅ Phase 1-5 | Engine foundation |

**Current blocker:** Fraisier can't fully function until **Phase 6 (fraiseql-server)** is complete.

### What FraiseQL Needs from Fraisier

| Need | What Fraisier Provides |
|---|---|
| **E2E Testing** | Complete integration test suite (webhook→deploy→run) |
| **Real-world example** | Shows how to structure a real FraiseQL app |
| **Documentation** | Deployment guides, environment setup, troubleshooting |
| **Ecosystem proof** | Demonstrates FraiseQL's multi-language support |

---

## Implementation Roadmap

### Step 1: Consolidation (Immediate)

**Action:** Remove duplication by establishing `/home/lionel/code/fraiseql/fraisier/` as the single source of truth.

```bash
# Option A: Delete standalone, keep monorepo version
rm -rf /home/lionel/code/fraisier

# Option B: Keep standalone as mirror (requires CI sync setup)
# (Keep as-is, set up GitHub Actions to sync)
```

**Recommendation:** **Option A** - Delete the standalone and consolidate completely.

**TODO:**
- [ ] Update `/home/lionel/code/fraiseql/fraisier/` to be complete
- [ ] Delete `/home/lionel/code/fraisier/` (or make it a symlink)
- [ ] Update README to reference monorepo location
- [ ] Update imports if fraisier needs to reference other monorepo components

### Step 2: Enhanced Configuration (Phase 2-3)

Add support for environment variables and secret management:

```yaml
# fraises.yaml
git:
  github:
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}  # Currently works

fraises:
  my_api:
    environments:
      production:
        health_check:
          auth_token: ${HEALTH_CHECK_TOKEN}  # Add secret support
```

**TODO:**
- [ ] Add `python-dotenv` support for `.env` files
- [ ] Implement secret manager abstraction
- [ ] Support AWS Secrets Manager, Vault, etc.

### Step 3: Database Schema (Phase 4)

Define Fraisier's database schema as FraiseQL types:

```python
# fraiseql/fraisier/schema/py/models.py
@fraiseql.type
class Fraise:
    id: str
    name: str
    type: FraiseType  # api, etl, scheduled, backup
    description: str

@fraiseql.type
class Deployment:
    id: str
    fraise_id: str
    environment: str
    status: DeploymentStatus
    started_at: datetime
    completed_at: datetime | None
    error: str | None
```

**TODO:**
- [ ] Create `fraiseql/fraisier/schema/py/models.py`
- [ ] Add database views and functions
- [ ] Document schema structure

### Step 4: API Exposure (Phase 6+) - **FRAISIER AS A GRAPHQL API**

**Key Vision:** Fraisier will eventually be a **GraphQL API** using FraiseQL.

This means Fraisier will:
1. **Host its own deployment status** via a FraiseQL server
2. **Accept queries** about deployment history, status, and statistics
3. **Accept mutations** to trigger deployments, cancel jobs, etc.
4. **Send subscriptions** for real-time deployment updates

**Architecture:**

```
Fraisier Components:
├── Python CLI + Webhook Server (current)
└── GraphQL API (built on FraiseQL - future)
    ├── Queries: fraise, deployment_history, statistics
    ├── Mutations: deploy, cancel, retry
    └── Subscriptions: deployment_status_changed, webhook_received
```

**Example GraphQL Schema (using @fraiseql decorators):**

```python
# fraiseql/fraisier/schema/py/models.py
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
    """A deployable service"""
    id: str
    name: str
    type: FraiseType
    description: str

@fraiseql_type
class Environment:
    """An environment configuration for a fraise"""
    name: str
    branch: str
    app_path: str
    last_deployed: datetime | None

@fraiseql_type
class Deployment:
    """A single deployment event"""
    id: str
    fraise_id: str
    environment: str
    status: DeploymentStatus
    started_at: datetime
    completed_at: datetime | None
    error_message: str | None
    commit_sha: str

@fraiseql_type
class DeploymentStatistics:
    """Deployment statistics"""
    total_deployments: int
    successful: int
    failed: int
    success_rate: float
    average_duration_seconds: float

@fraiseql_type
class Query:
    """Root query type"""

    @fraiseql_field
    def fraise(self, id: str) -> Fraise:
        """Get a fraise by ID"""
        # Resolves to: SELECT * FROM tb_fraises WHERE id = $1
        ...

    @fraiseql_field
    def deployment(self, id: str) -> Deployment:
        """Get a deployment by ID"""
        # Resolves to: SELECT * FROM tb_deployments WHERE id = $1
        ...

    @fraiseql_field
    def deployment_history(
        self,
        fraise_id: str | None = None,
        environment: str | None = None,
        status: DeploymentStatus | None = None,
        limit: int = 50,
        offset: int = 0
    ) -> list[Deployment]:
        """Get deployment history with optional filtering"""
        # Resolves to: SELECT * FROM v_deployment_history WHERE ...
        ...

    @fraiseql_field
    def deployment_statistics(
        self,
        fraise_id: str | None = None,
        time_range_days: int = 30
    ) -> DeploymentStatistics:
        """Get deployment statistics"""
        # Resolves to: SELECT ... FROM v_deployment_stats WHERE ...
        ...

@fraiseql_type
class Mutation:
    """Root mutation type"""

    @fraiseql_field
    def deploy(
        self,
        fraise_id: str,
        environment: str,
        force: bool = False
    ) -> Deployment:
        """Trigger a deployment"""
        # Calls: fn_request_deployment(fraise_id, environment, force)
        ...

    @fraiseql_field
    def cancel_deployment(self, deployment_id: str) -> Deployment:
        """Cancel a running deployment"""
        # Calls: fn_cancel_deployment(deployment_id)
        ...

    @fraiseql_field
    def retry_deployment(self, deployment_id: str) -> Deployment:
        """Retry a failed deployment"""
        # Calls: fn_retry_deployment(deployment_id)
        ...

@fraiseql_type
class Subscription:
    """Root subscription type"""

    @fraiseql_field
    def deployment_status_changed(self, fraise_id: str) -> Deployment:
        """Subscribe to deployment status changes"""
        # Emits when: INSERT INTO tb_deployments (status changed)
        ...

    @fraiseql_field
    def webhook_received(self) -> WebhookEvent:
        """Subscribe to webhook events"""
        # Emits when: INSERT INTO tb_webhook_events
        ...
```

**Example Queries:**

```graphql
# Get current status of all environments for a fraise
query GetFraiseStatus {
  fraise(id: "my_api") {
    id
    name
    type
    environments {
      name
      branch
      lastDeployed
    }
    deploymentHistory(limit: 5) {
      id
      status
      startedAt
      completedAt
      errorMessage
    }
  }
}

# Get deployment statistics
query GetStats {
  deploymentStatistics(fraiseId: "my_api", timeRangeDays: 30) {
    totalDeployments
    successful
    failed
    successRate
    averageDurationSeconds
  }
}

# Get all failed deployments in the last 7 days
query GetFailures {
  deploymentHistory(
    status: FAILED,
    limit: 100
  ) {
    id
    fraiseId
    environment
    startedAt
    errorMessage
  }
}
```

**Example Mutations:**

```graphql
# Trigger a deployment
mutation Deploy {
  deploy(fraiseId: "my_api", environment: "production") {
    id
    status
    startedAt
  }
}

# Force deployment (skip certain checks)
mutation ForceDeployment {
  deploy(fraiseId: "my_api", environment: "production", force: true) {
    id
    status
  }
}

# Cancel a running deployment
mutation Cancel {
  cancelDeployment(deploymentId: "dep_123") {
    id
    status
  }
}

# Retry a failed deployment
mutation Retry {
  retryDeployment(deploymentId: "dep_123") {
    id
    status
  }
}
```

**Example Subscriptions:**

```graphql
# Real-time deployment updates
subscription DeploymentUpdates {
  deploymentStatusChanged(fraiseId: "my_api") {
    id
    status
    completedAt
    errorMessage
  }
}

# All webhook events
subscription WebhookEvents {
  webhookReceived {
    id
    provider
    eventType
    branch
    commitSha
    receivedAt
  }
}
```

**Implementation Steps:**

1. **Phase 4:** Define FraiseQL schema with @fraiseql decorators
2. **Phase 4:** Create database schema and views
3. **Phase 6:** Implement GraphQL resolvers
4. **Phase 6:** Connect Python Fraisier CLI to GraphQL API
5. **Phase 6+:** Add GraphQL mutations and subscriptions

**Benefits of GraphQL API:**

✅ **Standardized interface** - Language-agnostic queries
✅ **Flexibility** - Clients request exactly what they need
✅ **Real-time updates** - Subscriptions for live status
✅ **Self-documenting** - Schema introspection
✅ **Ecosystem integration** - FraiseQL demonstrates itself
✅ **CLI and Web UI** - Both can query same API
✅ **Automation** - Scripts can trigger deployments via API

**Database Backing:**

The GraphQL API will query Fraisier's SQLite database:

```sql
-- Write tables (append-only)
tb_deployments       → Query via Deployment type
tb_webhook_events    → Query via WebhookEvent type
tb_fraises           → Query via Fraise type

-- Read views (optimized for queries)
v_fraise_status      → Fast status lookups
v_deployment_history → Historical queries
v_deployment_stats   → Statistics aggregation

-- Functions (mutations)
fn_request_deployment()  → deploy mutation
fn_cancel_deployment()   → cancel mutation
fn_retry_deployment()    → retry mutation
```

**TODO:**
- [ ] Create `fraiseql/fraisier/schema/py/models.py` with @fraiseql types
- [ ] Define all queries, mutations, subscriptions
- [ ] Implement GraphQL resolvers
- [ ] Connect database views to resolvers
- [ ] Write GraphQL tests
- [ ] Create example queries for documentation
- [ ] Update CLI to use GraphQL API (optional, but clean)

### Step 5: Testing & E2E (Phase 7+)

Create comprehensive E2E test suite:

```bash
tests/fraisier/
├── integration/
│   ├── test_webhook_github.py
│   ├── test_webhook_gitlab.py
│   ├── test_deployment_flow.py
│   └── test_health_checks.py
├── fixtures/
│   ├── webhook_payloads/
│   └── test_configs/
└── conftest.py             # Shared test setup
```

**TODO:**
- [ ] Set up Docker Compose with PostgreSQL for testing
- [ ] Create webhook payload fixtures
- [ ] Write E2E deployment tests
- [ ] Add load testing for webhook server

---

## Workspace Integration

### Current Cargo.toml Structure

```toml
[workspace]
members = [
    "crates/fraiseql-cli",
    "crates/fraiseql-core",
    "crates/fraiseql-server",
]
```

**Note:** Fraisier is NOT a Rust crate, so it stays outside the `[workspace]` section.

### New Structure (Recommended)

```
fraiseql/                      # Monorepo root
├── Cargo.toml               # Rust workspace (crates only)
├── Cargo.lock
├── crates/                  # Rust crates
│   ├── fraiseql-cli/
│   ├── fraiseql-core/
│   └── fraiseql-server/
├── fraiseql-python/         # Language SDKs
├── fraiseql-typescript/
├── fraiseql-php/
├── fraiseql-go/
├── fraiseql-java/
├── fraisier/                # ← Reference implementation (Python)
│   ├── pyproject.toml      # uv package
│   ├── fraisier/           # Package code
│   └── tests/              # E2E tests
└── tests/                   # Shared E2E tests (integration)
    └── fraisier/           # Fraisier-specific E2E tests
```

### Dependency Management

**Fraisier's Python dependencies** (in `/home/lionel/code/fraiseql/fraisier/pyproject.toml`):

```toml
[project]
name = "fraisier"
version = "0.1.0"
requires-python = ">=3.11"

dependencies = [
    "fastapi>=0.109.0",        # Webhook server
    "uvicorn>=0.27.0",         # ASGI
    "pyyaml>=6.0",             # Config parsing
    "requests>=2.31.0",        # HTTP client
    "click>=8.1.0",            # CLI
    "rich>=13.0.0",            # Terminal UI
]

[project.scripts]
fraisier = "fraisier.cli:main"
fraisier-webhook = "fraisier.webhook:run_server"
```

**No changes needed** - Fraisier's dependencies are independent of Rust workspace.

---

## Git Integration Details

### Supported Providers

| Provider | Webhook Type | Verification | Self-hosted |
|---|---|---|---|
| **GitHub** | `X-Hub-Signature-256` | HMAC-SHA256 | ✅ GitHub Enterprise |
| **GitLab** | `X-Gitlab-Token` | Plain token | ✅ Self-hosted |
| **Gitea** | `X-Gitea-Signature` | HMAC-SHA256 | ✅ Always self-hosted |
| **Bitbucket** | `X-Hub-Signature` | HMAC-SHA1 | ✅ Server/Data Center |

### Webhook Flow

```
1. Push to Git repo (main branch)
   ↓
2. Git provider sends webhook to `https://fraisier.example.com/webhook`
   ↓
3. fraisier/webhook.py receives request
   ↓
4. Auto-detect provider (from headers) or use ?provider=github parameter
   ↓
5. Verify signature (HMAC)
   ↓
6. Parse webhook event → normalized WebhookEvent
   ↓
7. Look up branch mapping in fraises.yaml
   ↓
8. Queue deployment task
   ↓
9. Execute deployment workflow
```

### Implementation Files

- **GitHub:** `fraisier/git/github.py`
- **GitLab:** `fraisier/git/gitlab.py`
- **Gitea:** `fraisier/git/gitea.py`
- **Bitbucket:** `fraisier/git/bitbucket.py`
- **Webhook Server:** `fraisier/webhook.py`

---

## Architecture Patterns

### CQRS (Command Query Responsibility Segregation)

Fraisier database schema:

```sql
-- Write tables (command side)
CREATE TABLE tb_deployments (
    id TEXT PRIMARY KEY,
    fraise_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    error_message TEXT
);

CREATE TABLE tb_webhook_events (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    event_type TEXT NOT NULL,
    branch TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    received_at TIMESTAMP
);

-- Read views (query side)
CREATE VIEW v_fraise_status AS
    SELECT DISTINCT ON (fraise_id, environment)
        fraise_id,
        environment,
        status,
        started_at AS last_deployed
    FROM tb_deployments
    ORDER BY fraise_id, environment, started_at DESC;
```

**Pattern:** Append-only writes to `tb_*` tables, materialized views for queries.

### Deployment Strategy Pattern

```python
# fraisier/deployers/base.py
class DeploymentStrategy(ABC):
    @abstractmethod
    async def deploy(self, config: FraiseConfig, env: EnvironmentConfig) -> DeploymentResult:
        pass

# fraisier/deployers/api_deployer.py
class ApiDeployer(DeploymentStrategy):
    async def deploy(self, config: FraiseConfig, env: EnvironmentConfig):
        # 1. Git clone/pull
        # 2. Build artifacts
        # 3. Database migration
        # 4. fraiseql-cli compile (when available)
        # 5. fraiseql-server startup (when available)
        # 6. Health check
        # 7. Record result
```

### Provider Abstraction

```python
# fraisier/git/base.py
class GitProvider(ABC):
    name: str

    @abstractmethod
    def verify_webhook_signature(self, payload: bytes, headers: dict) -> bool:
        pass

    @abstractmethod
    def parse_webhook_event(self, headers: dict, payload: dict) -> WebhookEvent:
        pass

# Usage
def route_webhook(headers: dict, payload: bytes):
    provider = detect_provider(headers)  # GitHub, GitLab, etc.
    if provider.verify_webhook_signature(payload, headers):
        event = provider.parse_webhook_event(headers, json.loads(payload))
        return queue_deployment(event)
```

---

## Documentation Needs

### What Needs to Be Documented

1. **Setup Guide** - `docs/fraisier-setup.md`
   - Installation
   - Configuration (fraises.yaml)
   - Environment setup

2. **Webhook Configuration** - `docs/fraisier-webhooks.md`
   - GitHub setup
   - GitLab setup
   - Gitea setup
   - Bitbucket setup

3. **Deployment Workflow** - `docs/fraisier-deployment.md`
   - How deployments work
   - Systemd integration
   - Health checks
   - Rollback procedures

4. **Architecture** - `docs/fraisier-architecture.md`
   - CQRS pattern
   - Deployment strategies
   - Provider abstraction

5. **API Reference** - `docs/api/fraisier-api.md`
   - CLI commands
   - Webhook endpoints
   - Configuration schema

### Current Documentation Status

✅ **README.md exists** - Good overview of features and architecture
✅ **fraises.example.yaml exists** - Comprehensive configuration examples
⏳ **Individual setup guides needed** - per Git provider
⏳ **API documentation needed** - CLI, webhook, GraphQL

---

## Risk Assessment

### Integration Risks

| Risk | Severity | Mitigation |
|---|---|---|
| **Duplication** | HIGH | Consolidate immediately, use monorepo as source of truth |
| **Incomplete Rust core** | MEDIUM | Fraisier can work independently until fraiseql-server is ready |
| **Configuration complexity** | MEDIUM | Provide clear examples and validation tools |
| **Git provider updates** | LOW | Abstract behind GitProvider interface, easy to extend |
| **Database schema changes** | MEDIUM | Use migration system (confiture), version schemas |

### Testing Strategy

1. **Unit tests** - Per component (config, database, deployers)
2. **Integration tests** - Full webhook→deploy flow
3. **E2E tests** - With real fraiseql-server when available
4. **Load tests** - Webhook server under concurrent requests

---

## Recommendations

### Short-term (Immediate)

1. **Consolidate repositories**
   - Make `/home/lionel/code/fraiseql/fraisier/` the authoritative source
   - Consider removing `/home/lionel/code/fraisier/` standalone copy
   - Update all references to use monorepo location

2. **Verify current code quality**
   - Run `ruff check` on all Fraisier code
   - Run `pytest` suite
   - Fix any issues found

3. **Update documentation**
   - Point to monorepo location
   - Add monorepo integration guide

### Medium-term (Phases 2-4)

1. **Enhanced configuration**
   - Add secret management
   - Add environment variable substitution
   - Add configuration validation

2. **Database schema definition**
   - Create FraiseQL schema for Fraisier models
   - Add database views and functions
   - Document CQRS pattern

3. **Testing infrastructure**
   - Set up E2E test suite
   - Add integration tests with fraiseql-server
   - Add webhook provider tests

### Long-term (Phases 5+)

1. **Expose status via GraphQL API**
   - Fraisier queries deployment status via FraiseQL
   - Trigger deployments via mutations
   - Real-time status via subscriptions

2. **Advanced features**
   - Blue-green deployments
   - Canary deployments
   - Automated rollbacks
   - Multi-region deployment

3. **Ecosystem integration**
   - pgGit for database version control
   - confiture for schema migrations
   - pg_tviews for materialized views

---

## Summary Table

| Aspect | Status | Notes |
|---|---|---|
| **Code Quality** | ✅ Good | Well-structured, documented |
| **Feature Complete** | ✅ Yes | Supports all planned fraise types |
| **Integration Status** | ⏳ Partial | Duplicated in monorepo, not consolidated |
| **FraiseQL Dependencies** | ⏳ Phase 6+ | Needs fraiseql-server for full functionality |
| **Testing** | ⏳ Incomplete | Needs E2E test suite |
| **Documentation** | ✅ Good | README and examples exist, setup guides needed |
| **Next Phase** | Phase 2 | Database & Cache (establishes patterns) |

---

## Files Modified/Created

- ✅ `/home/lionel/code/fraiseql/.claude/FRAISIER_INTEGRATION_ANALYSIS.md` - This document
- ⏳ `/home/lionel/code/fraiseql/fraisier/` - Needs consolidation
- ⏳ `/home/lionel/code/fraisier/` - Consider removing (or keep as mirror)

---

**Document Version:** 1.0
**Last Updated:** 2026-01-15
**Next Review:** After Phase 2 completion
