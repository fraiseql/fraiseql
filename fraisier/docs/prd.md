# Fraisier - Product Requirements Document

> **Build GraphQL APIs with any language, any database, any Git provider. Deploy anywhere.**

---

## Executive Summary

**Fraisier** is an open-source deployment orchestrator for FraiseQL applications. It manages "fraises" (services) across any deployment target - from bare metal to cloud providers to Coolify.

**Fraisier is THE reference FraiseQL implementation** - the canonical example of how to build a production FraiseQL application. It exists in every language FraiseQL supports, demonstrating best practices for each ecosystem.

Part of the FraiseQL ecosystem:

| Project | Role | Status |
|---------|------|--------|
| **FraiseQL** | Compiled GraphQL engine (Rust runtime) | Stable |
| **confiture** | PostgreSQL migrations (build-from-scratch) | Stable |
| **pgGit** | Database version control (Git for PostgreSQL) | Stable |
| **pg_tviews** | Incremental materialized views | Beta |
| **jsonb_delta** | JSONB surgical updates | Stable |
| **fraiseql-data** | Seed data generation | Phase 6 |
| **graphql-cascade** | Client-side cache invalidation | Stable |
| **Fraisier** | Deployment orchestrator + **Reference Implementation** | Alpha |
| **FraiseQL Cloud** | Hosted platform | Future |

**Fraisier uses all of these** - it's the reference implementation that demonstrates the entire stack working together.

### FraiseQL Ecosystem Integration

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      FRAISIER DEPLOYMENT FLOW                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   1. DATABASE SCHEMA                                                    │
│      confiture build → Build schema from DDL (300-600× faster)          │
│      pgGit commit → Version control the schema changes                  │
│                                                                         │
│   2. GRAPHQL COMPILATION                                                │
│      fraiseql-cli compile → schema.json → CompiledSchema.json           │
│                                                                         │
│   3. SEED DATA (dev/test)                                               │
│      fraiseql-data add → Generate test data with dependencies           │
│                                                                         │
│   4. RUNTIME                                                            │
│      fraiseql-server → Rust runtime serves GraphQL                      │
│                                                                         │
│   5. DEPLOY                                                             │
│      fraisier deploy → Orchestrate the entire flow                      │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Fraisier orchestrates:**

| Step | Tool | What it does |
|------|------|--------------|
| Schema | **confiture** | `confiture build` - Build DB from DDL |
| Version | **pgGit** | `pggit.commit()` - Track schema changes |
| Views | **pg_tviews** | Incremental materialized views |
| Compile | **fraiseql-cli** | Compile GraphQL schema |
| Seed | **fraiseql-data** | Generate test/dev data |
| Serve | **fraiseql-server** | Run GraphQL API |
| Deploy | **fraisier** | Orchestrate everything |

---

## The Vision

```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│                            F R A I S E Q L                              │
│                                                                         │
│            "Any language. Any database. Any Git. Any target."           │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   LANGUAGES      DATABASES       GIT PROVIDERS      TARGETS             │
│   ──────────     ─────────       ─────────────      ───────             │
│   Python         PostgreSQL      GitHub             Bare Metal          │
│   TypeScript     SQLite          GitLab             Coolify             │
│   Go             SQL Server      Gitea              AWS                 │
│   Rust           MySQL           Bitbucket          Scaleway            │
│   Any...         Any...          Self-hosted        OVH                 │
│                                  Any...             Docker Compose      │
│                                                     Kubernetes          │
│                                                     Any...              │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Vocabulary

| Term | Meaning |
|------|---------|
| **Fraisier** | The deployment orchestrator |
| **Fraise** | A deployable service (API, worker, scheduled job) |
| **Environment** | Where a fraise runs (dev, staging, production) |
| **Target** | Deployment provider (Coolify, AWS, bare metal, etc.) |

### Fraisier is a Fraise

Fraisier itself is a fraise - a FraiseQL API that:

- Follows CQRS patterns
- Has its own database
- Can deploy itself

```yaml
fraises:
  fraisier:                    # Fraisier manages itself
    type: api
    environments:
      production:
        target:
          provider: bare-metal
```

### Fraisier is THE Reference Implementation

Fraisier serves as the **canonical example** of how to build a FraiseQL application. It lives inside the FraiseQL monorepo and is implemented in every language FraiseQL supports:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│                    github.com/fraiseql/fraiseql                         │
│                                                                         │
│   fraiseql/                                                             │
│   ├── crates/               → Rust engine                               │
│   │   ├── fraiseql-core/    → Core library                              │
│   │   ├── fraiseql-cli/     → CLI (compile schemas)                     │
│   │   └── fraiseql-server/  → GraphQL runtime                           │
│   ├── fraiseql-python/      → Python schema authoring                   │
│   ├── fraiseql-typescript/  → TypeScript schema authoring               │
│   ├── fraiseql-go/          → Go schema authoring                       │
│   ├── fraiseql-java/        → Java schema authoring                     │
│   │                                                                     │
│   ├── fraisier/             → REFERENCE IMPLEMENTATION                  │
│   │   ├── db/               → Database (THE source of truth)            │
│   │   │   ├── schema.sql                                                │
│   │   │   ├── views/        → v_fraise_status, v_deployment_*, ...      │
│   │   │   └── functions/    → fn_request_deployment, ...                │
│   │   ├── schema/           → Schema authoring (all languages)          │
│   │   │   ├── py/           → Python (@fraiseql decorators)             │
│   │   │   ├── ts/           → TypeScript                                │
│   │   │   └── yaml/         → YAML (language-agnostic)                  │
│   │   └── cli/              → CLI implementations                       │
│   │                                                                     │
│   ├── tests/                → E2E tests using Fraisier                  │
│   └── docs/                                                             │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Why inside the monorepo?**

| Benefit | Description |
|---------|-------------|
| **Single clone** | `git clone fraiseql` gives you framework + working example |
| **Always in sync** | Framework changes immediately reflected in Fraisier |
| **E2E test suite** | Fraisier IS the E2E test for FraiseQL itself |
| **Learning path** | Study framework code, then see it applied |
| **Dogfooding** | We eat our own cooking, visibly |

**Why multiple language implementations?**

| Benefit | Description |
|---------|-------------|
| **Learning** | Study Fraisier in your preferred language to learn FraiseQL |
| **Proof** | Demonstrates FraiseQL truly works with any language |
| **Best Practices** | Each implementation shows idiomatic patterns for that ecosystem |
| **Choice** | Deploy Fraisier in the language your team knows best |
| **Cross-language E2E** | Tests verify all implementations behave identically |

**Fraisier project structure:**

```
fraisier/
├── db/                         # THE SOURCE OF TRUTH
│   ├── schema.sql              # Database schema
│   ├── views/                  # Queries (v_*)
│   │   ├── v_fraise_status.sql
│   │   ├── v_deployment_history.sql
│   │   └── v_deployment_stats.sql
│   └── functions/              # Mutations (fn_*)
│       ├── fn_request_deployment.sql
│       ├── fn_complete_deployment.sql
│       └── fn_process_webhook.sql
│
├── schema/                     # Schema authoring (pick your language)
│   ├── py/                     # Python authoring
│   │   └── schema.py           # @fraiseql.type, @fraiseql.query, etc.
│   ├── ts/                     # TypeScript authoring
│   │   └── schema.ts
│   └── yaml/                   # YAML authoring (language-agnostic)
│       └── schema.yaml
│
├── compiled/                   # Build artifacts
│   └── CompiledSchema.json     # Output of fraiseql-cli compile
│
├── cli/                        # CLI tool (separate from GraphQL)
│   ├── py/                     # Python CLI
│   ├── ts/                     # TypeScript CLI
│   └── go/                     # Go CLI
│
└── fraises.example.yaml        # Fraisier config format
```

**How it works:**

```bash
# 1. Author schema in Python (or any language)
cd schema/py && python schema.py > ../schema.json

# 2. Compile with FraiseQL
fraiseql-cli compile schema.json -o compiled/CompiledSchema.json

# 3. Run the server
fraiseql-server --schema compiled/CompiledSchema.json --database postgres://...
```

**Schema authoring example (Python):**

```python
import fraiseql

@fraiseql.type
class Fraise:
    name: str
    type: str
    description: str | None
    environments: list[Environment]

@fraiseql.query(sql_source="v_fraise_status")
def fraises() -> list[Fraise]:
    """List all fraises with their status."""
    pass

@fraiseql.mutation(sql_source="fn_request_deployment", operation="CREATE")
def deploy(fraise: str, environment: str) -> Deployment:
    """Request a deployment."""
    pass
```

**Key insight: Database is the source of truth. FraiseQL compiles GraphQL to database operations.**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         FraiseQL Architecture                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   1. SCHEMA AUTHORING (any language)                                    │
│      Python / TypeScript / Go / Java / YAML / GraphQL SDL               │
│                          │                                              │
│                          ▼                                              │
│   2. COMPILATION                                                        │
│      fraiseql-cli compile schema.json → CompiledSchema.json             │
│                          │                                              │
│                          ▼                                              │
│   3. RUST RUNTIME                                                       │
│      fraiseql-server --schema CompiledSchema.json                       │
│                          │                                              │
│                          ▼                                              │
│   4. DATABASE (THE SOURCE OF TRUTH)                                     │
│      • Tables: tb_*                                                     │
│      • Views: v_*  (queries)                                            │
│      • Functions: fn_*  (mutations)                                     │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**FraiseQL principles:**

- **Compiled, not interpreted** - All GraphQL resolved at build time
- **Deterministic** - No resolvers, no hooks, no dynamic logic
- **Database-centric** - Business logic lives in SQL
- **Multi-database** - PostgreSQL, MySQL, SQL Server, SQLite

**Fraisier as a FraiseQL project:**

- Schema authored in your preferred language (Python, TypeScript, etc.)
- Compiled to `CompiledSchema.json`
- Served by the Rust runtime (`fraiseql-server`)
- All business logic in database views and functions

### Fraisier as E2E Test Suite

Fraisier serves as the comprehensive E2E test for FraiseQL:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         FraiseQL CI Pipeline                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   1. Unit tests (core/)                                                 │
│      └─ Test framework internals                                        │
│                                                                         │
│   2. Integration tests (core/)                                          │
│      └─ Test framework with real databases                              │
│                                                                         │
│   3. E2E tests (fraisier/)                      ◄── THE REAL TEST       │
│      ├─ Build Fraisier with FraiseQL                                    │
│      ├─ Run full deployment scenarios                                   │
│      ├─ Verify all language implementations                             │
│      └─ Cross-language consistency checks                               │
│                                                                         │
│   "If Fraisier works, FraiseQL works."                                  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**E2E scenarios tested via Fraisier:**

| Scenario | What it tests |
|----------|---------------|
| Deploy a fraise | Full CQRS flow, webhooks, providers |
| Multi-environment | Configuration system, branch mapping |
| Rollback | State management, history tracking |
| Cross-language deploy | Fraisier-py deploys Fraisier-ts |
| Self-update | Fraisier deploys itself |
| Database migrations | Schema evolution, CQRS patterns |

---

## Architecture

### CQRS + Observer Pattern

Fraisier follows the FraiseQL philosophy:

1. **Everything in the database** - Business logic in SQL
2. **Observer pattern** - External processes handle what SQL can't

```
┌─────────────────────────────────────────────────────────────────┐
│                       GraphQL API                               │
│                    (thin layer over DB)                         │
├─────────────────────────────────────────────────────────────────┤
│   Queries → SELECT from views                                   │
│   Mutations → CALL stored procedures                            │
│   Subscriptions → LISTEN/NOTIFY                                 │
├─────────────────────────────────────────────────────────────────┤
│                     Fraisier Database                           │
│                  (separate from service DBs)                    │
├───────────────────────────┬─────────────────────────────────────┤
│       QUERY SIDE          │         COMMAND SIDE                │
│        (views)            │      (tables + functions)           │
├───────────────────────────┼─────────────────────────────────────┤
│  v_fraise_status          │  tb_deployment_request              │
│  v_deployment_history     │  tb_deployment                      │
│  v_deployment_stats       │  tb_fraise_state                    │
│  v_recent_webhooks        │  tb_webhook_event                   │
│                           │                                     │
│                           │  fn_request_deployment()            │
│                           │  fn_complete_deployment()           │
│                           │  fn_process_webhook()               │
└───────────────────────────┴─────────────────────────────────────┘
                                       │
                                       │ NOTIFY deployment_requested
                                       ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Observer (Python Process)                     │
│                                                                 │
│   LISTEN deployment_requested                                   │
│        │                                                        │
│        ▼                                                        │
│   1. Read tb_deployment_request WHERE status = 'pending'        │
│   2. Dispatch to appropriate Deployment Provider                │
│   3. CALL fn_complete_deployment(...)                           │
│                                                                 │
│   Handles what SQL cannot:                                      │
│     • Shell commands (git, systemctl)                           │
│     • HTTP calls (health checks, provider APIs)                 │
│     • File system operations                                    │
└─────────────────────────────────────────────────────────────────┘
```

### Deployment Providers

Fraisier dispatches deployments to pluggable providers:

```
┌─────────────────────────────────────────────────────────────────┐
│                         Fraisier                                │
│              (FraiseQL-aware orchestration)                     │
│                                                                 │
│   Knows: fraises, CQRS, schemas, migrations, health checks      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │  Deployment Providers
                              │
     ┌──────────┬─────────────┼─────────────┬──────────┬─────────┐
     ▼          ▼             ▼             ▼          ▼         ▼
┌────────┐ ┌────────┐   ┌──────────┐  ┌────────┐ ┌────────┐ ┌────────┐
│Coolify │ │  AWS   │   │ Scaleway │  │  OVH   │ │Docker  │ │  Bare  │
│        │ │ECS/EC2 │   │          │  │        │ │Compose │ │ Metal  │
└────────┘ └────────┘   └──────────┘  └────────┘ └────────┘ └────────┘
```

### Git Provider Abstraction

Fraisier works with any Git hosting platform:

```
┌─────────────────────────────────────────────────────────────────┐
│                         Fraisier                                │
│                    Webhook Handler                              │
│                                                                 │
│   POST /webhook  (auto-detects provider from headers)           │
│   POST /webhook?provider=gitlab  (explicit provider)            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │  Git Providers (pluggable)
                              │
     ┌──────────┬─────────────┼─────────────┬──────────┐
     ▼          ▼             ▼             ▼          ▼
┌────────┐ ┌────────┐   ┌──────────┐  ┌────────┐ ┌────────┐
│ GitHub │ │ GitLab │   │  Gitea   │  │Bitbuck-│ │ Custom │
│        │ │        │   │ Forgejo  │  │   et   │ │Provider│
├────────┤ ├────────┤   ├──────────┤  ├────────┤ ├────────┤
│ Cloud  │ │ Cloud  │   │Self-host │  │ Cloud  │ │  Any   │
│  or    │ │  or    │   │  only    │  │  or    │ │        │
│  GHE   │ │  Self  │   │          │  │ Server │ │        │
└────────┘ └────────┘   └──────────┘  └────────┘ └────────┘
```

| Provider | Signature | Self-hosted | Notes |
|----------|-----------|-------------|-------|
| `github` | HMAC-SHA256 | Yes (GHE) | Default provider |
| `gitlab` | Token header | Yes | Also supports self-hosted |
| `gitea` | HMAC-SHA256 | Yes | Compatible with Forgejo |
| `bitbucket` | HMAC/IP allowlist | Yes (Server) | Cloud uses IP allowlisting |

**Per-fraise provider override:**

```yaml
git:
  provider: github  # default

fraises:
  public_api:
    # Uses default (github)
    environments:
      production:
        branch: main

  internal_api:
    git:
      provider: gitlab  # override for this fraise
      base_url: https://gitlab.mycompany.com
    environments:
      production:
        branch: main
```

**Custom provider:**

```python
from fraisier.git import GitProvider, WebhookEvent, register_provider

class MyGitProvider(GitProvider):
    name = "mygit"

    def verify_webhook_signature(self, payload, headers):
        # Your verification logic
        pass

    def parse_webhook_event(self, headers, payload):
        # Return normalized WebhookEvent
        return WebhookEvent(
            provider=self.name,
            event_type="push",
            branch="main",
            commit_sha="abc123",
            ...
        )

register_provider(MyGitProvider)
```

### Database Independence

FraiseQL supports multiple databases. Fraisier inherits this:

| Database | Use Case |
|----------|----------|
| **SQLite** | Single server, simple setups, edge |
| **PostgreSQL** | Multi-server, existing PG infrastructure |
| **SQL Server** | Enterprise environments |

Each fraise has its own database. Fraisier's database is separate and never touched by deployments:

```
┌─────────────────────────────────────────────────────────────────┐
│                     Fraisier Database                           │
│                    (NEVER rebuilt/dropped)                      │
└─────────────────────────────────────────────────────────────────┘
                              │
          Fraisier deploys fraises with their OWN databases:
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│ myapp_db_dev  │     │ myapp_db_prod │     │ other_db      │
│               │     │               │     │               │
│ strategy:     │     │ strategy:     │     │ strategy:     │
│ REBUILD       │     │ APPLY         │     │ REBUILD       │
└───────────────┘     └───────────────┘     └───────────────┘
```

---

## Configuration

### fraises.yaml

```yaml
# fraises.yaml - The Fraise Registry

# ============================================================
# GIT PROVIDER CONFIGURATION
# ============================================================
git:
  provider: github  # default: github, gitlab, gitea, bitbucket

  github:
    # base_url: https://github.mycompany.com  # for GitHub Enterprise
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}

  gitlab:
    base_url: https://gitlab.com  # or self-hosted URL
    secret_token: ${FRAISIER_WEBHOOK_SECRET}

  gitea:
    base_url: https://gitea.mycompany.com
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}

  bitbucket:
    base_url: https://bitbucket.org
    server: false  # true for Bitbucket Server/Data Center

fraises:
  # ============================================================
  # FRAISIER (deploys itself)
  # ============================================================
  fraisier:
    type: api
    description: Fraisier deployment orchestrator
    database:
      type: sqlite                    # or postgresql, sqlserver
      path: /opt/fraisier/fraisier.db
      strategy: apply                 # never rebuild
    environments:
      production:
        target:
          provider: bare-metal
          host: deploy.example.com
          systemd_service: fraisier.service

  # ============================================================
  # YOUR API
  # ============================================================
  my_api:
    type: api
    description: My FraiseQL API

    database:
      type: postgresql
      strategy: apply                 # production: migrations only

    health_check:
      path: /health
      timeout: 30

    environments:
      development:
        database:
          name: myapi_db_dev
          strategy: rebuild           # dev: drop + create OK
        target:
          provider: docker-compose
          file: docker-compose.dev.yml

      staging:
        database:
          name: myapi_db_staging
          strategy: rebuild
        target:
          provider: coolify
          endpoint: https://coolify.example.com
          project_id: abc123

      production-eu:
        database:
          name: myapi_db_prod
          strategy: apply
        target:
          provider: scaleway
          region: fr-par
          instance_type: PRO2-S

      production-us:
        database:
          name: myapi_db_prod_us
          strategy: apply
        target:
          provider: aws
          region: us-east-1
          service: ecs
          cluster: production

  # ============================================================
  # WORKER SERVICE
  # ============================================================
  my_worker:
    type: worker
    description: Background job processor

    environments:
      production:
        target:
          provider: coolify
          endpoint: https://coolify.example.com

  # ============================================================
  # SCHEDULED JOBS
  # ============================================================
  backups:
    type: scheduled
    description: Database backup jobs

    environments:
      production:
        jobs:
          daily_backup:
            schedule: "0 2 * * *"
            script: ./scripts/backup.sh
          sync_to_s3:
            schedule: "0 */6 * * *"
            script: ./scripts/sync-s3.sh
            depends_on: daily_backup

# ============================================================
# BRANCH MAPPING (for webhook routing)
# ============================================================
branch_mapping:
  dev:
    fraise: my_api
    environment: development
  staging:
    fraise: my_api
    environment: staging
  main:
    fraise: my_api
    environment: production-eu
```

---

## GraphQL API

Fraisier exposes a FraiseQL GraphQL API:

### Schema

```graphql
type Query {
  # List all fraises
  fraises: [Fraise!]!
  fraise(name: String!): Fraise

  # Deployment info
  deployments(fraise: String, environment: String, limit: Int = 20): [Deployment!]!
  deployment(id: ID!): Deployment

  # Status
  status(fraise: String!, environment: String!): FraiseStatus!

  # Stats
  stats(fraise: String, days: Int = 30): DeploymentStats!

  # Webhooks
  webhookEvents(limit: Int = 20): [WebhookEvent!]!
}

type Mutation {
  # Deploy a fraise
  deploy(
    fraise: String!
    environment: String!
    dryRun: Boolean = false
    force: Boolean = false
  ): DeploymentResult!

  # Rollback
  rollback(
    fraise: String!
    environment: String!
    toVersion: String
  ): DeploymentResult!

  # Process webhook
  processWebhook(
    provider: WebhookProvider!
    payload: String!
    signature: String
  ): WebhookResult!
}

type Subscription {
  # Real-time deployment progress
  deploymentProgress(deploymentId: ID!): DeploymentProgress!

  # Watch deployments
  deploymentStarted: Deployment!
  deploymentCompleted: Deployment!
}

# Types
type Fraise {
  name: String!
  type: FraiseType!
  description: String
  database: DatabaseConfig
  environments: [Environment!]!
}

type Environment {
  name: String!
  target: DeploymentTarget!
  currentVersion: String
  lastDeployedAt: DateTime
  status: FraiseStatus!
}

type DeploymentTarget {
  provider: String!
  config: JSON
}

type Deployment {
  id: ID!
  fraise: String!
  environment: String!
  startedAt: DateTime!
  completedAt: DateTime
  durationSeconds: Float
  oldVersion: String
  newVersion: String
  status: DeploymentStatus!
  triggeredBy: TriggerType!
  target: DeploymentTarget!
  gitCommit: String
  gitBranch: String
  errorMessage: String
}

type DeploymentProgress {
  deploymentId: ID!
  step: String!
  progress: Int!
  message: String
  timestamp: DateTime!
}

type DeploymentStats {
  total: Int!
  successful: Int!
  failed: Int!
  rolledBack: Int!
  avgDurationSeconds: Float
  successRate: Float!
}

enum FraiseType { API, WORKER, SCHEDULED, CUSTOM }
enum DeploymentStatus { PENDING, IN_PROGRESS, SUCCESS, FAILED, ROLLED_BACK }
enum TriggerType { WEBHOOK, MANUAL, SCHEDULED, API }
enum WebhookProvider { GITHUB, GITLAB, GITEA, BITBUCKET, CUSTOM }
enum FraiseStatus { HEALTHY, DEGRADED, DOWN, UNKNOWN }
```

### Example Queries

```graphql
# List all fraises with their environments
query {
  fraises {
    name
    type
    environments {
      name
      currentVersion
      status
      target { provider }
    }
  }
}

# Deploy to production
mutation {
  deploy(fraise: "my_api", environment: "production-eu") {
    success
    deployment {
      id
      oldVersion
      newVersion
      target { provider }
    }
  }
}

# Watch deployment progress
subscription {
  deploymentProgress(deploymentId: "123") {
    step
    progress
    message
  }
}
```

---

## CLI

```bash
# List fraises
fraisier list
fraisier list --flat

# Deploy
fraisier deploy my_api production-eu
fraisier deploy my_api production-eu --dry-run
fraisier deploy my_api production-us --force

# Status
fraisier status my_api production-eu
fraisier status-all
fraisier status-all --environment production

# History
fraisier history
fraisier history --fraise my_api
fraisier history --limit 50

# Stats
fraisier stats
fraisier stats --fraise my_api --days 7

# Webhooks
fraisier webhooks

# Config
fraisier config validate
fraisier config show

# Self-update
fraisier deploy fraisier production
```

### Output Examples

```
$ fraisier list

Fraisier - Fraise Registry
├── fraisier (api) - Fraisier deployment orchestrator
│   └── production → bare-metal (deploy.example.com)
├── my_api (api) - My FraiseQL API
│   ├── development → docker-compose
│   ├── staging → coolify
│   ├── production-eu → scaleway (fr-par)
│   └── production-us → aws (us-east-1)
├── my_worker (worker) - Background job processor
│   └── production → coolify
└── backups (scheduled) - Database backup jobs
    └── production
        ├── daily_backup (0 2 * * *)
        └── sync_to_s3 (0 */6 * * *)
```

```
$ fraisier deploy my_api production-eu

Deploying my_api → production-eu (scaleway/fr-par)

  ✓ Pre-flight checks passed
  ✓ Database backup created
  ⟳ Pulling latest code...
  ✓ Code updated (abc1234)
  ⟳ Running migrations...
  ✓ Database migrated
  ⟳ Deploying to Scaleway...
  ✓ Instance updated
  ⟳ Health check...
  ✓ Health check passed

✓ Deployment successful!
  Version: 1.2.3 → 1.2.4
  Duration: 47.3s
  Target: scaleway/fr-par
```

---

## Deployment Providers

### Provider Interface

```python
class DeploymentProvider(ABC):
    """Each provider implements this interface."""

    @abstractmethod
    def deploy(self, fraise: FraiseConfig, version: str) -> DeploymentResult:
        """Deploy a fraise to this provider."""
        pass

    @abstractmethod
    def rollback(self, fraise: FraiseConfig, to_version: str) -> bool:
        """Rollback to previous version."""
        pass

    @abstractmethod
    def health_check(self, fraise: FraiseConfig) -> HealthStatus:
        """Check if fraise is healthy."""
        pass

    @abstractmethod
    def logs(self, fraise: FraiseConfig, lines: int = 100) -> str:
        """Fetch recent logs."""
        pass

    @abstractmethod
    def stop(self, fraise: FraiseConfig) -> bool:
        """Stop the fraise."""
        pass

    @abstractmethod
    def start(self, fraise: FraiseConfig) -> bool:
        """Start the fraise."""
        pass
```

### Built-in Providers

| Provider | Description | Config |
|----------|-------------|--------|
| `bare-metal` | Direct systemd on VPS | `host`, `systemd_service` |
| `docker-compose` | Local Docker Compose | `file`, `service` |
| `coolify` | Coolify PaaS | `endpoint`, `project_id` |
| `aws` | AWS ECS/EC2 | `region`, `service`, `cluster` |
| `scaleway` | Scaleway instances | `region`, `instance_type` |
| `ovh` | OVH Cloud | `region`, `project` |
| `kubernetes` | K8s deployments | `context`, `namespace` |

### Coolify Integration

Coolify is the recommended provider for most setups:

```yaml
environments:
  production:
    target:
      provider: coolify
      endpoint: https://coolify.example.com
      api_key: ${COOLIFY_API_KEY}
      project_id: abc123
      # Coolify handles:
      # - Docker builds
      # - SSL/TLS
      # - Zero-downtime deploys
      # - Server management
```

**Why Coolify?**

- Open source
- Self-hostable
- Handles infrastructure complexity
- Fraisier focuses on FraiseQL-specific orchestration

---

## Database Schema (CQRS)

### Write Side (Tables)

```sql
-- Deployment requests (command queue)
CREATE TABLE tb_deployment_request (
    id INTEGER PRIMARY KEY,
    fraise TEXT NOT NULL,
    environment TEXT NOT NULL,
    requested_at TEXT NOT NULL,
    requested_by TEXT,
    trigger_type TEXT NOT NULL,  -- webhook, manual, api, scheduled
    status TEXT DEFAULT 'pending',  -- pending, processing, completed, failed
    git_branch TEXT,
    git_commit TEXT,
    target_provider TEXT,
    target_config TEXT  -- JSON
);

-- Deployment history
CREATE TABLE tb_deployment (
    id INTEGER PRIMARY KEY,
    fk_request INTEGER REFERENCES tb_deployment_request(id),
    fraise TEXT NOT NULL,
    environment TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_seconds REAL,
    old_version TEXT,
    new_version TEXT,
    status TEXT NOT NULL,
    target_provider TEXT,
    git_commit TEXT,
    git_branch TEXT,
    error_message TEXT,
    details TEXT  -- JSON
);

-- Fraise state
CREATE TABLE tb_fraise_state (
    id INTEGER PRIMARY KEY,
    fraise TEXT NOT NULL,
    environment TEXT NOT NULL,
    current_version TEXT,
    last_deployed_at TEXT,
    last_deployed_by TEXT,
    status TEXT DEFAULT 'unknown',
    target_provider TEXT,
    UNIQUE(fraise, environment)
);

-- Webhook events
CREATE TABLE tb_webhook_event (
    id INTEGER PRIMARY KEY,
    received_at TEXT NOT NULL,
    provider TEXT NOT NULL,
    event_type TEXT NOT NULL,
    branch TEXT,
    commit_sha TEXT,
    sender TEXT,
    payload TEXT,
    processed INTEGER DEFAULT 0,
    fk_deployment INTEGER REFERENCES tb_deployment(id)
);
```

### Read Side (Views)

```sql
-- Fraise status with latest deployment
CREATE VIEW v_fraise_status AS
SELECT
    fs.fraise,
    fs.environment,
    fs.current_version,
    fs.status,
    fs.target_provider,
    fs.last_deployed_at,
    d.duration_seconds as last_deploy_duration,
    d.git_commit as last_git_commit
FROM tb_fraise_state fs
LEFT JOIN tb_deployment d ON d.id = (
    SELECT id FROM tb_deployment
    WHERE fraise = fs.fraise AND environment = fs.environment
    ORDER BY started_at DESC LIMIT 1
);

-- Deployment history
CREATE VIEW v_deployment_history AS
SELECT
    id,
    fraise,
    environment,
    started_at,
    completed_at,
    duration_seconds,
    old_version || ' → ' || new_version as version_change,
    status,
    target_provider,
    git_commit,
    error_message
FROM tb_deployment
ORDER BY started_at DESC;

-- Stats per fraise/environment
CREATE VIEW v_deployment_stats AS
SELECT
    fraise,
    environment,
    COUNT(*) as total,
    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as successful,
    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed,
    AVG(duration_seconds) as avg_duration,
    ROUND(100.0 * SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) / COUNT(*), 1) as success_rate
FROM tb_deployment
WHERE started_at >= date('now', '-30 days')
GROUP BY fraise, environment;
```

---

## Installation

### Quick Start

```bash
# Clone FraiseQL (includes Fraisier)
git clone https://github.com/fraiseql/fraiseql
cd fraiseql/fraisier

# Install Python implementation
cd py
pip install -e .

# Or install from PyPI
pip install fraisier

# Initialize
fraisier init

# Edit configuration
vim fraises.yaml

# Deploy your first fraise
fraisier deploy my_api production
```

### Monorepo Structure

```
fraiseql/                       # github.com/fraiseql/fraiseql
├── crates/                     # Rust engine
│   ├── fraiseql-core/          # Core compilation & execution
│   ├── fraiseql-cli/           # CLI: fraiseql-cli compile
│   └── fraiseql-server/        # GraphQL runtime server
│
├── fraiseql-python/            # Python schema authoring
├── fraiseql-typescript/        # TypeScript schema authoring
├── fraiseql-go/                # Go schema authoring
├── fraiseql-java/              # Java schema authoring
│
├── fraisier/                   # REFERENCE IMPLEMENTATION
│   ├── db/                     # THE SOURCE OF TRUTH
│   │   ├── schema.sql          # Database tables
│   │   ├── views/              # Queries: v_fraise_status, etc.
│   │   └── functions/          # Mutations: fn_request_deployment, etc.
│   │
│   ├── schema/                 # Schema authoring (pick your language)
│   │   ├── py/schema.py        # Python decorators
│   │   ├── ts/schema.ts        # TypeScript
│   │   └── yaml/schema.yaml    # YAML (language-agnostic)
│   │
│   ├── compiled/               # Build output
│   │   └── CompiledSchema.json
│   │
│   ├── cli/                    # CLI tool implementations
│   │   ├── py/                 # Python CLI
│   │   ├── ts/                 # TypeScript CLI
│   │   └── go/                 # Go CLI
│   │
│   └── fraises.example.yaml    # Configuration reference
│
├── tests/                      # E2E tests using Fraisier
│   └── e2e/
│
└── docs/
```

**Build & Run:**

```bash
# Compile schema (from any authoring language)
cd fraisier/schema/py && python schema.py > schema.json
fraiseql-cli compile schema.json -o ../compiled/CompiledSchema.json

# Run server
fraiseql-server --schema fraisier/compiled/CompiledSchema.json
```

### Deployed Structure

```
/opt/fraisier/
├── bin/
│   └── fraisier                # CLI
├── config/
│   ├── fraises.yaml            # Fraise registry
│   └── secrets.env             # API keys, tokens
├── data/
│   └── fraisier.db             # Fraisier's own database
├── logs/
│   └── fraisier.log
└── providers/                  # Custom providers
    └── my_provider.py
```

---

## FraiseQL Cloud (Future)

Hosted FraiseQL platform, powered by Fraisier:

```
┌─────────────────────────────────────────────────────────────────┐
│                      FraiseQL Cloud                             │
│                  "Push your schema, get an API"                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │                      Fraisier                           │   │
│   │            (orchestrates all customer fraises)          │   │
│   └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│        ┌─────────────────────┼─────────────────────┐            │
│        ▼                     ▼                     ▼            │
│   ┌─────────┐          ┌─────────┐          ┌─────────┐         │
│   │Customer │          │Customer │          │Customer │         │
│   │   A     │          │   B     │          │   C     │         │
│   │ fraise  │          │ fraise  │          │ fraise  │         │
│   │   +DB   │          │   +DB   │          │   +DB   │         │
│   └─────────┘          └─────────┘          └─────────┘         │
│                                                                 │
│   https://a.fraiseql.cloud/graphql                              │
│   https://b.fraiseql.cloud/graphql                              │
│   https://c.fraiseql.cloud/graphql                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Tiers

| Tier | Description |
|------|-------------|
| **Free** | 1 fraise, shared infra, 10k requests/month |
| **Pro** | Multiple fraises, dedicated DB, custom domains |
| **Enterprise** | Private cloud, SLA, support |

---

## Roadmap

### v0.1.0 - MVP

- [x] Fraise registry (fraises.yaml)
- [x] SQLite database (CQRS)
- [x] CLI: list, deploy, status, history
- [x] Git provider abstraction (GitHub, GitLab, Gitea, Bitbucket)
- [x] Universal webhook endpoint with auto-detection
- [ ] Providers: bare-metal, docker-compose

### v0.2.0 - Providers

- [ ] Coolify provider
- [ ] AWS provider (ECS)
- [ ] Scaleway provider
- [ ] PostgreSQL support

### v0.3.0 - Polish

- [ ] Custom Git provider support (plugin API)
- [ ] Deployment locks
- [ ] Slack/Discord notifications
- [ ] Web UI (optional)

### v1.0.0 - Production Ready

- [ ] All major deployment providers
- [ ] Full documentation
- [ ] Comprehensive tests
- [ ] Battle-tested

### v1.1.0 - Multi-Language

- [ ] fraisier-ts (TypeScript implementation)
- [ ] fraisier-go (Go implementation)
- [ ] Implementation guide for new languages

### Future

- [ ] fraisier-rs (Rust implementation)
- [ ] FraiseQL Cloud (hosted platform)
- [ ] SQL Server support
- [ ] Kubernetes provider
- [ ] Multi-region deployments

---

## Summary

**Fraisier** completes the FraiseQL ecosystem:

| Layer | Solution |
|-------|----------|
| **Query Language** | FraiseQL (any language, any DB) |
| **Source Control** | Any Git provider (GitHub, GitLab, Gitea, Bitbucket, self-hosted) |
| **Deployment** | Fraisier (any target) |
| **Hosting** | FraiseQL Cloud (future) |

```
                              ┌─ GitHub
                              ├─ GitLab
Any Language  ──►  FraiseQL  ─┼─ Gitea      ──►  Fraisier  ──►  Any Target
   │                  │       ├─ Bitbucket           │              │
   ├─ Python          ├─ PostgreSQL                  ├─ Bare Metal  ├─ Your VPS
   ├─ TypeScript      ├─ SQLite                      ├─ Coolify     ├─ AWS
   ├─ Go              ├─ SQL Server                  ├─ AWS         ├─ Scaleway
   ├─ Rust            └─ MySQL                       ├─ Scaleway    ├─ OVH
   └─ Any                                            ├─ OVH         └─ Anywhere
                                                     └─ Kubernetes
```

> **Build GraphQL APIs with any language, any database, any Git provider. Deploy anywhere.** 🍓

---

*Document Version: 1.0.0*
*Last Updated: 2026-01-15*
*Author: FraiseQL Team*
