# Fraisier Quick Reference

## What is Fraisier?

**Fraisier** = deployment orchestrator for the FraiseQL ecosystem.

- **Manages** services (called "fraises") across environments
- **Handles** Git webhooks (GitHub, GitLab, Gitea, Bitbucket)
- **Orchestrates** deployments (build → migrate → compile → deploy → health check)
- **Tracks** deployment history in SQLite database
- **Supports** APIs, ETL pipelines, scheduled jobs, and backups

**Current Status:** Complete Python implementation in `/home/lionel/code/fraiseql/fraisier/`

---

## Quick Architecture

```
Git Push → Webhook Received → Config Lookup → Deploy Strategy → Health Check → Record Status
  │                                                                                    │
  └────────────────────────────────────────────────────────────────────────────────┘
                           SQLite Database (CQRS Pattern)
```

### Key Components

| Component | File(s) | Purpose |
|-----------|---------|---------|
| **CLI** | `cli.py` | Commands: list, deploy, status, history |
| **Webhook Server** | `webhook.py` | FastAPI server: receive and route webhooks |
| **Config** | `config.py` | Parse fraises.yaml configuration |
| **Database** | `database.py` | SQLite: store deployment history (CQRS) |
| **Deployers** | `deployers/*.py` | Strategy per fraise type (api, etl, scheduled, backup) |
| **Git Providers** | `git/*.py` | Abstract GitHub, GitLab, Gitea, Bitbucket |

---

## Configuration (fraises.yaml)

### Basic Structure

```yaml
git:
  provider: github
  github:
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}

fraises:
  my_service:
    type: api                    # or: etl, scheduled, backup
    description: My GraphQL API
    environments:
      production:
        name: api.example.com
        branch: main
        app_path: /var/www/api
        systemd_service: api.service
        database:
          name: myapp_production
          strategy: apply         # or: rebuild (dev only!)
        health_check:
          url: https://api.example.com/health
```

### Fraise Types

```yaml
# API Service (systemd, health checks, database)
type: api
environments:
  production:
    systemd_service: myapp.service
    database: { name: myapp_prod }
    health_check: { url: https://api.example.com/health }

# ETL Pipeline (script execution)
type: etl
environments:
  production:
    script_path: scripts/etl/run_etl
    database: { name: myapp_prod }

# Scheduled Job (cron via systemd timer)
type: scheduled
environments:
  production:
    jobs:
      daily_stats:
        schedule: "0 2 * * *"      # cron format
        systemd_service: stats.service
        systemd_timer: stats.timer

# Backup Job (retention, remote sync)
type: backup
environments:
  production:
    jobs:
      db_backup:
        schedule: "0 */6 * * *"     # every 6 hours
        script_path: scripts/backup_db
        retention_days: 7
```

---

## CLI Commands

```bash
# List all services
fraisier list

# Deploy a service
fraisier deploy my_api production
fraisier deploy my_api staging --force

# Check status
fraisier status my_api production

# View deployment history
fraisier history
fraisier history my_api              # filter by service
fraisier history my_api production   # filter by environment

# Inspect configuration
fraisier config show

# Run webhook server
fraisier-webhook --port 8080
```

---

## Webhook Setup

### GitHub

```bash
# Repository → Settings → Webhooks → Add webhook
Payload URL: https://your-fraisier.com/webhook
Content type: application/json
Events: Push events
Secret: Your FRAISIER_WEBHOOK_SECRET
```

### GitLab

```bash
# Project → Settings → Webhooks
URL: https://your-fraisier.com/webhook
Trigger: Push events
Secret token: Your FRAISIER_WEBHOOK_SECRET
```

### Gitea

```bash
# Repository → Settings → Webhooks → Add Webhook
Target URL: https://your-fraisier.com/webhook
HTTP Method: POST
Events: push
Secret: Your FRAISIER_WEBHOOK_SECRET
```

### Bitbucket

```bash
# Repository → Repository settings → Webhooks → Create webhook
URL: https://your-fraisier.com/webhook
Trigger: Push
Secret: Your FRAISIER_WEBHOOK_SECRET
```

---

## Deployment Workflow

### What Happens on Webhook

1. **Webhook received** at `/webhook` endpoint
2. **Signature verified** using provider secret
3. **Branch mapping checked** in fraises.yaml

   ```yaml
   branch_mapping:
     main:
       fraise: my_api
       environment: production
   ```

4. **Deployment queued** for identified fraise + environment
5. **Deployer runs** (api_deployer, etl_deployer, etc.)
6. **History recorded** in SQLite database

### API Deployment Steps

```
1. Git Clone/Pull
   ↓
2. Build Phase (if build script exists)
   ↓
3. Database Migration (run schema migration)
   ↓
4. Schema Compilation (fraiseql-cli compile) [future]
   ↓
5. Service Restart (systemctl restart SERVICE)
   ↓
6. Health Check (GET /health, verify 200)
   ↓
7. Record Success/Failure
```

---

## Database Schema (CQRS)

### Write Tables (Append-only)

```sql
-- Every deployment attempt
tb_deployments (
  id TEXT PRIMARY KEY,
  fraise_id TEXT,
  environment TEXT,
  status TEXT,        -- pending, running, success, failed
  started_at TIMESTAMP,
  completed_at TIMESTAMP,
  error_message TEXT
)

-- Every webhook received
tb_webhook_events (
  id TEXT PRIMARY KEY,
  provider TEXT,      -- github, gitlab, gitea, bitbucket
  event_type TEXT,
  branch TEXT,
  commit_sha TEXT,
  received_at TIMESTAMP
)
```

### Read Views (Materialized)

```sql
-- Current status per fraise+environment
v_fraise_status AS
  SELECT DISTINCT ON (fraise_id, environment)
    fraise_id, environment, status, started_at AS last_deployed
  FROM tb_deployments
  ORDER BY fraise_id, environment, started_at DESC

-- Deployment history
v_deployment_history AS
  SELECT id, fraise_id, environment, status, started_at, completed_at, error_message
  FROM tb_deployments
  ORDER BY started_at DESC
```

---

## Git Providers

### Supported

| Provider | Self-hosted | Auth Method | Status |
|----------|---|---|---|
| GitHub | ✅ Enterprise | HMAC-SHA256 | ✅ Complete |
| GitLab | ✅ Yes | Secret token | ✅ Complete |
| Gitea | ✅ Always | HMAC-SHA256 | ✅ Complete |
| Bitbucket | ✅ Server | HMAC-SHA1 | ✅ Complete |

### Custom Provider Example

```python
from fraisier.git import GitProvider, WebhookEvent, register_provider

class MyGitProvider(GitProvider):
    name = "mygit"

    def verify_webhook_signature(self, payload: bytes, headers: dict) -> bool:
        # Your verification logic
        pass

    def parse_webhook_event(self, headers: dict, payload: dict) -> WebhookEvent:
        # Return normalized event
        return WebhookEvent(
            provider="mygit",
            event_type="push",
            branch=payload["branch"],
            commit_sha=payload["commit"],
            ...
        )

register_provider(MyGitProvider)
```

---

## Environment Variables

```bash
# Webhook server
FRAISIER_WEBHOOK_SECRET=your-secret
FRAISIER_HOST=0.0.0.0
FRAISIER_PORT=8080
FRAISIER_GIT_PROVIDER=github      # auto-detect from headers, or specify

# Deployment
FRAISIER_WORK_DIR=/tmp/fraisier   # where to clone repos
FRAISIER_SSH_KEY=/home/user/.ssh/id_rsa  # for git operations

# Database
FRAISIER_DB_PATH=/var/lib/fraisier/fraisier.db

# Logging
RUST_LOG=fraisier=debug
```

---

## Integration with FraiseQL Core

### Current Dependencies

Fraisier currently depends on:

- ✅ **fraiseql-python** - For schema authoring (not yet using it)
- ⏳ **fraiseql-cli** - For schema compilation (Phase 9)
- ⏳ **fraiseql-server** - For GraphQL runtime (Phase 6)

### Deployment Flow (when all components available)

```bash
# 1. Git webhook triggers
/webhook → branch: main

# 2. Config lookup
my_api → production → /var/www/api

# 3. Build phase
cd /var/www/api
./build.sh

# 4. Database migration
confiture build

# 5. Schema compilation
fraiseql-cli compile schema.json → CompiledSchema.json

# 6. Server restart
systemctl restart api.service

# 7. Health check
curl https://api.example.com/health

# 8. Record result
INSERT INTO tb_deployments (status='success')
```

### Fraisier Self-Hosting (Future)

Fraisier could host its own status via FraiseQL:

```python
# fraisier/schema/py/models.py
@fraiseql.type
class FraiseStatus:
    fraise_id: str
    environment: str
    status: str
    last_deployed: datetime

@fraiseql.type
class Query:
    def fraise_status(self, fraise_id: str) -> FraiseStatus:
        # Query v_fraise_status view
        ...
```

Then access via GraphQL:

```graphql
query {
  fraiseStatus(fraiseId: "my_api") {
    status
    lastDeployed
  }
}
```

---

## Common Tasks

### Deploy a Service Manually

```bash
fraisier deploy my_api production
```

### Check Deployment Status

```bash
# Current status
fraisier status my_api production

# Full history
fraisier history my_api production
```

### View Configuration

```bash
# Show effective config
fraisier config show

# Validate config
fraisier config validate --file fraises.yaml
```

### Start Webhook Server

```bash
# Default (localhost:8080)
fraisier-webhook

# Custom host/port
fraisier-webhook --host 0.0.0.0 --port 3000
```

---

## Troubleshooting

### Webhook Not Triggering Deployment

1. **Check webhook delivery** in Git provider settings
2. **Verify secret** matches `FRAISIER_WEBHOOK_SECRET`
3. **Check logs** - `journalctl -u fraisier-webhook`
4. **Verify branch mapping** - is branch listed in fraises.yaml?

### Deployment Failed

1. **Check logs** - `fraisier history my_api production`
2. **Manual test** - `fraisier deploy my_api production --verbose`
3. **Check health check** - Can you curl the health endpoint manually?
4. **Check database** - Is migration succeeding?

### Health Check Failing

1. **Test endpoint manually** - `curl https://api.example.com/health`
2. **Check service logs** - `journalctl -u api.service`
3. **Increase timeout** - Adjust `health_check.timeout` in fraises.yaml
4. **Check dependencies** - Is service able to start?

---

## File Reference

### Main Python Files

| File | Purpose | Lines |
|------|---------|-------|
| `cli.py` | Click CLI interface | ~400 |
| `webhook.py` | FastAPI webhook server | ~300 |
| `config.py` | YAML parsing and validation | ~200 |
| `database.py` | SQLite operations (CQRS) | ~400 |
| `deployers/api_deployer.py` | API deployment strategy | ~200 |
| `deployers/etl_deployer.py` | ETL deployment strategy | ~150 |
| `deployers/scheduled_deployer.py` | Scheduled job setup | ~150 |
| `deployers/backup_deployer.py` | Backup job setup | ~150 |
| `git/github.py` | GitHub provider | ~150 |
| `git/gitlab.py` | GitLab provider | ~150 |
| `git/gitea.py` | Gitea provider | ~150 |
| `git/bitbucket.py` | Bitbucket provider | ~150 |

### Configuration Files

| File | Purpose |
|------|---------|
| `pyproject.toml` | Python package config (uv) |
| `fraises.example.yaml` | Complete config example |
| `README.md` | User documentation |

### Documentation

| File | Purpose |
|------|---------|
| `/docs/` | Additional docs (setup guides, etc.) |

---

## Next Steps

1. ✅ **Consolidate** Fraisier into monorepo as source of truth
2. ⏳ **Add schema definitions** (when Phase 8 ready)
3. ⏳ **Create E2E tests** (when Phase 6 ready)
4. ⏳ **Expose status via GraphQL** (when fraiseql-server ready)
5. ⏳ **Advanced deployments** (blue-green, canary, rollback)

---

**For detailed integration analysis, see:** `.claude/FRAISIER_INTEGRATION_ANALYSIS.md`
