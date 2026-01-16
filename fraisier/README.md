# Fraisier

**Deployment orchestrator for the FraiseQL ecosystem.**

*Any language, any database, any Git provider, any deployment target.*

---

A **fraisier** (French for strawberry plant) manages **fraises** (services).
Just as a strawberry plant produces strawberries, Fraisier orchestrates
the deployment of your services (fraises).

## The Reference Implementation

Fraisier is **THE canonical FraiseQL application** - the official example of how to build production software with FraiseQL. It lives inside the FraiseQL monorepo:

```
github.com/fraiseql/fraiseql
├── crates/               → Rust engine (CLI, server, core)
├── fraiseql-python/      → Python schema authoring
├── fraiseql-typescript/  → TypeScript schema authoring
├── fraiseql-go/          → Go schema authoring
│
├── fraisier/             → REFERENCE IMPLEMENTATION
│   ├── db/               → Database (THE source of truth)
│   │   ├── views/        → v_fraise_status, v_deployment_history
│   │   └── functions/    → fn_request_deployment, fn_complete_deployment
│   ├── schema/           → Schema authoring (all languages)
│   │   ├── py/           → Python (@fraiseql decorators)
│   │   ├── ts/           → TypeScript
│   │   └── yaml/         → YAML
│   └── cli/              → CLI tools
│
└── tests/                → E2E tests using Fraisier
```

**FraiseQL architecture:**
1. **Author schema** in any language (Python, TypeScript, YAML, etc.)
2. **Compile** with `fraiseql-cli compile` → `CompiledSchema.json`
3. **Run** with `fraiseql-server` (Rust runtime)
4. **Database** is the source of truth (views for queries, functions for mutations)

**Fraisier is the E2E test suite for FraiseQL** - if Fraisier works, FraiseQL works.

**Learn FraiseQL by studying Fraisier.**

## Quick Start

```bash
# Install
pip install fraisier

# List all fraises
fraisier list

# Deploy a fraise
fraisier deploy my_api production

# Check status
fraisier status my_api production

# View deployment history
fraisier history
```

## Configuration

Create a `fraises.yaml` file:

```yaml
# Git provider configuration
git:
  provider: github  # or gitlab, gitea, bitbucket
  github:
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}
  # Self-hosted GitLab example:
  gitlab:
    base_url: https://gitlab.mycompany.com
    secret_token: ${FRAISIER_WEBHOOK_SECRET}

fraises:
  my_api:
    type: api
    description: My awesome API
    environments:
      development:
        name: api.myapp.dev
        branch: dev
        app_path: /var/www/api.myapp.dev
        systemd_service: api.myapp.dev.service
        health_check:
          url: https://api.myapp.dev/health
      production:
        name: api.myapp.io
        branch: main
        app_path: /var/www/api.myapp.io
        systemd_service: api.myapp.io.service
        health_check:
          url: https://api.myapp.io/health

  # Use different Git provider per fraise
  internal_api:
    type: api
    git:
      provider: gitlab  # override default
      base_url: https://gitlab.mycompany.com
    environments:
      production:
        branch: main
        app_path: /var/www/internal-api

# Webhook routing
branch_mapping:
  dev:
    fraise: my_api
    environment: development
  main:
    fraise: my_api
    environment: production
```

## Fraise Types

| Type | Description |
|------|-------------|
| `api` | Web services and APIs (systemd, health checks) |
| `etl` | Data pipelines (scripts, shared code) |
| `scheduled` | Cron jobs and timers (systemd timers) |
| `backup` | Backup jobs (retention, remote sync) |

## Git Providers

Fraisier supports any Git hosting platform:

| Provider | Description | Self-hosted |
|----------|-------------|-------------|
| `github` | GitHub.com or GitHub Enterprise | Yes |
| `gitlab` | GitLab.com or self-hosted GitLab | Yes |
| `gitea` | Gitea / Forgejo | Yes |
| `bitbucket` | Bitbucket Cloud or Server | Yes |

The webhook endpoint auto-detects the provider from headers, or you can specify it explicitly.

## Webhook Server

Fraisier includes a webhook server for event-driven deployments:

```bash
# Start webhook server
fraisier-webhook

# Environment variables
export FRAISIER_WEBHOOK_SECRET=your-webhook-secret
export FRAISIER_GIT_PROVIDER=github  # optional, auto-detected
export FRAISIER_HOST=0.0.0.0
export FRAISIER_PORT=8080
```

**Webhook endpoints:**

| Endpoint | Description |
|----------|-------------|
| `POST /webhook` | Universal endpoint (auto-detects provider) |
| `POST /webhook?provider=gitlab` | Explicit provider |
| `GET /providers` | List supported providers |
| `GET /health` | Health check |

Configure your Git server to send push events to `https://your-server/webhook`.

## Architecture

```
fraises.yaml          →  Configuration (what fraises exist)
fraisier.db (SQLite)  →  State & History (what's deployed)
```

Fraisier follows the CQRS pattern with clear separation:
- **Write tables** (`tb_*`): Record deployments, webhooks, state changes
- **Read views** (`v_*`): Query deployment history, statistics, status

## Custom Git Providers

Add support for any Git platform by implementing the `GitProvider` interface:

```python
from fraisier.git import GitProvider, WebhookEvent, register_provider

class MyGitProvider(GitProvider):
    name = "mygit"

    def verify_webhook_signature(self, payload, headers):
        # Your signature verification logic
        pass

    def parse_webhook_event(self, headers, payload):
        # Return normalized WebhookEvent
        pass

    # ... other methods

register_provider(MyGitProvider)
```

## Part of the FraiseQL Ecosystem

Fraisier integrates with the entire FraiseQL stack:

| Tool | Purpose | Fraisier uses it for |
|------|---------|---------------------|
| **confiture** | PostgreSQL migrations | `confiture build` - schema deployment |
| **pgGit** | Database version control | Track schema changes |
| **fraiseql** | Compiled GraphQL engine | API runtime |
| **pg_tviews** | Incremental materialized views | Performance |
| **fraiseql-data** | Seed data generation | Dev/test environments |

**Deployment flow:**
```bash
# What fraisier deploy does under the hood:
confiture build                    # 1. Build database schema
pggit commit                       # 2. Version control changes
fraiseql-cli compile schema.json   # 3. Compile GraphQL
fraiseql-server --schema ...       # 4. Start runtime
```

**One clone, everything you need:**
```bash
git clone https://github.com/fraiseql/fraiseql
cd fraiseql/fraisier
```

## License

MIT
