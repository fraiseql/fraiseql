# Fraisier Architecture

Fraisier is a deployment orchestrator built as the reference implementation of a FraiseQL application.

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Fraisier                              │
│                  Deployment Orchestrator                     │
└─────────────────────────────────────────────────────────────┘
                              │
                ┌─────────────┼─────────────┐
                ▼             ▼             ▼
        ┌───────────────┐ ┌──────────┐ ┌─────────────┐
        │      CLI      │ │ Webhook  │ │  GraphQL    │
        │   (Click)     │ │ (FastAPI)│ │   API       │
        │               │ │          │ │ (FraiseQL)  │
        └───────────────┘ └──────────┘ └─────────────┘
                │             │             │
                └─────────────┼─────────────┘
                              │
                        ┌─────▼──────┐
                        │ Deployers  │
                        │            │
                        │  API       │
                        │  ETL       │
                        │  Scheduled │
                        └─────┬──────┘
                              │
                ┌─────────────┼──────────────┐
                ▼             ▼              ▼
        ┌────────────┐ ┌────────────┐ ┌──────────────┐
        │   Git      │ │ Database   │ │  Deployment  │
        │ Providers  │ │  (CQRS)    │ │  Providers   │
        │            │ │            │ │              │
        │ GitHub     │ │ tb_* (W)   │ │ Coolify      │
        │ GitLab     │ │ v_*  (R)   │ │ AWS          │
        │ Gitea      │ │            │ │ Bare Metal   │
        │ Bitbucket  │ │            │ │ Docker       │
        └────────────┘ └────────────┘ └──────────────┘
```

---

## Component Overview

### 1. CLI Layer (`cli.py`)

**Responsibility**: User interface for command-line operations

**Commands**:

- `fraisier list` - List all fraises
- `fraisier deploy <fraise> <environment>` - Deploy a service
- `fraisier status <fraise> <environment>` - Check status
- `fraisier history` - View deployment history
- `fraisier stats` - Show statistics
- `fraisier webhooks` - View webhook events
- `fraisier config validate` - Validate configuration

**Architecture**:

- Uses Click framework for CLI building
- Delegates to deployers based on fraise type
- Returns formatted output via Rich library

**Data Flow**:
```
User Input → CLI Command → Config Loader → Deployer → Result
```

### 2. Configuration Layer (`config.py`)

**Responsibility**: Load and resolve fraise configurations

**Key Classes**:

- `FraisierConfig` - Main configuration loader

**Features**:

- Loads YAML from standard locations
- Resolves hierarchical structure (fraise → environment → specific config)
- Supports environment variable substitution
- Branch mapping for webhook routing

**Example**:
```python
config = FraisierConfig("fraises.yaml")

# Get specific fraise+environment
fraise_config = config.get_fraise_environment("my_api", "production")

# Get fraise for a git branch (webhook use)
fraise_config = config.get_fraise_for_branch("main")
```

### 3. Deployer Layer (`deployers/`)

**Responsibility**: Execute deployments for different service types

**Base Class**: `BaseDeployer` (abstract)

**Interface**:
```python
class BaseDeployer(ABC):
    def get_current_version(self) -> str | None
    def get_latest_version(self) -> str | None
    def is_deployment_needed(self) -> bool
    def execute(self) -> DeploymentResult
    def rollback(self, to_version: str | None) -> DeploymentResult
    def health_check(self) -> bool
```

**Implementations**:

#### APIDeployer

- **For**: Web services, APIs
- **Operations**: Git pull, database migrations, systemd restart, health check
- **Config**: app_path, systemd_service, git_repo, health_check

#### ETLDeployer

- **For**: Data processing pipelines
- **Operations**: Git pull, script execution, log capture
- **Config**: script_path, dependencies

#### ScheduledDeployer

- **For**: Cron jobs, timers
- **Operations**: Schedule setup, execution, result recording
- **Config**: schedule, script, dependencies

**Deployment Flow**:
```

1. Load fraise configuration
2. Get current version (what's running)
3. Get latest version (from git/source)
4. If versions differ:
   a. Record deployment request
   b. Execute deployment steps
   c. Run health check
   d. Record result
5. Return DeploymentResult (success/failure)
```

### 4. Git Provider Layer (`git/`)

**Responsibility**: Normalize webhooks from different Git hosting platforms

**Base Class**: `GitProvider` (abstract)

**Interface**:
```python
class GitProvider(ABC):
    def verify_webhook_signature(self, payload: bytes, headers: dict) -> bool
    def parse_webhook_event(self, headers: dict, payload: dict) -> WebhookEvent
    def get_signature_header_name(self) -> str
    def get_event_header_name(self) -> str
    def get_clone_url(self, repository: str) -> str
```

**Implementations**:

| Provider | Signature | Header | Status |
|----------|-----------|--------|--------|
| GitHub | HMAC-SHA256 | X-Hub-Signature-256 | ✅ |
| GitLab | Token | X-Gitlab-Token | ✅ |
| Gitea | HMAC-SHA256 | X-Gitea-Signature | ✅ |
| Bitbucket | HMAC/IP | X-Hub-Signature | ✅ |

**Webhook Event Normalization**:
```python
@dataclass
class WebhookEvent:
    provider: str                    # "github", "gitlab", etc.
    event_type: str                  # "push", "merge_request", etc.
    branch: str | None               # Branch name
    commit_sha: str | None           # Commit SHA
    sender: str | None               # Username
    repository: str | None           # owner/repo
    raw_payload: dict                # Original payload

    # Normalized flags
    is_push: bool
    is_tag: bool
    is_merge_request: bool
    is_ping: bool
```

**Provider Registry**:

- Dynamic provider loading
- Auto-detection from webhook headers
- Pluggable architecture for custom providers

### 5. Database Layer (`database.py`)

**Responsibility**: Persistent state management using CQRS pattern

**Architecture**: Command Query Responsibility Segregation

```
Write Side (Commands)          Read Side (Queries)
─────────────────────          ──────────────────
tb_fraise_state          ──→  v_fraise_status
tb_deployment            ──→  v_deployment_history
tb_webhook_event         ──→  v_deployment_stats
                         ──→  v_recent_webhooks
```

**Tables** (Write Side):

```sql
tb_fraise_state
├── fraise: str
├── environment: str
├── job: str (optional, for scheduled jobs)
├── current_version: str
├── last_deployed_at: datetime
├── last_deployed_by: str
├── status: enum[healthy|degraded|down|unknown]
└── UNIQUE(fraise, environment, job)

tb_deployment
├── fraise: str
├── environment: str
├── job: str (optional)
├── started_at: datetime
├── completed_at: datetime
├── duration_seconds: float
├── old_version: str
├── new_version: str
├── status: enum[pending|in_progress|success|failed|rolled_back]
├── triggered_by: enum[webhook|manual|scheduled|api]
├── git_commit: str
├── git_branch: str
├── error_message: str
└── details: JSON

tb_webhook_event
├── received_at: datetime
├── event_type: str
├── branch: str
├── commit_sha: str
├── sender: str
├── payload: JSON
├── processed: bool
└── deployment_id: foreign_key(tb_deployment)
```

**Views** (Read Side):

```sql
v_fraise_status
├── fraise
├── environment
├── current_version
├── status
├── last_deployed_at
└── last_deploy_duration

v_deployment_history
├── id
├── fraise
├── environment
├── started_at
├── completed_at
├── version_change
├── status
├── error_message

v_deployment_stats
├── total
├── successful
├── failed
├── rolled_back
├── avg_duration
├── success_rate
```

**Key Methods**:
```python
class Database:
    def record_deployment(...)
    def get_recent_deployments(...)
    def get_deployment_stats(...)
    def record_webhook_event(...)
    def get_recent_webhooks(...)
    def update_fraise_state(...)
```

### 6. Webhook Handler (`webhook.py`)

**Responsibility**: Event-driven deployment trigger

**Architecture**:
```
Git Webhook Event
       ↓
    FastAPI
       ↓
Git Provider Verification
       ↓
Webhook Event Parsing
       ↓
Branch → Fraise Mapping
       ↓
Background Deployment Task
       ↓
Database Recording
```

**Endpoints**:

- `POST /webhook` - Universal endpoint (auto-detects provider)
- `POST /webhook?provider=github` - Explicit provider
- `GET /health` - Health check
- `GET /providers` - List supported providers

**Flow**:
```python
1. Receive webhook
2. Verify signature (provider-specific)
3. Parse payload (normalize to WebhookEvent)
4. Look up deployment target (branch_mapping)
5. Get fraise configuration
6. Launch deployment in background
7. Return 202 Accepted
8. Deployment proceeds asynchronously
```

---

## Data Flow: Deployment Process

### Manual Deploy (CLI)

```
User: fraisier deploy my_api production
           │
           ▼
    CLI loads fraises.yaml
           │
           ▼
    Get fraise config for my_api/production
           │
           ▼
    APIDeployer(config)
           │
           ▼
    1. Get current version (git HEAD)
    2. Get latest version (git remote)
    3. If different:
       - Record deployment_request
       - Execute deployment:
         a. git pull
         b. Run migrations
         c. systemctl restart
         d. Health check
       - Record deployment result
           │
           ▼
    Display result to user
```

### Webhook-Triggered Deploy

```
GitHub webhook push to main
           │
           ▼
POST /webhook
           │
           ▼
Verify GitHub signature
           │
           ▼
Parse webhook event
  (branch: main, commit: abc123)
           │
           ▼
Look up branch_mapping
  (main → my_api / production)
           │
           ▼
Get my_api/production config
           │
           ▼
Launch background deployment task
  (save to tb_deployment request)
           │
           ▼
Return 202 Accepted to GitHub
           │
           ▼
Background process:
  1. Execute deployment (same as manual)
  2. Update tb_deployment
  3. Update tb_fraise_state
  4. Send notifications (optional)
```

---

## Design Patterns

### 1. Strategy Pattern (Deployers)

```python
# Each fraise type uses appropriate strategy
deployer: BaseDeployer = {
    "api": APIDeployer,
    "etl": ETLDeployer,
    "scheduled": ScheduledDeployer,
}[fraise_type](config)

result = deployer.execute()
```

**Benefits**:

- Easy to add new fraise types
- Each type encapsulates logic
- Consistent interface

### 2. Strategy Pattern (Git Providers)

```python
# Each Git platform uses appropriate provider
provider: GitProvider = get_provider(provider_name, config)
event = provider.parse_webhook_event(headers, payload)
```

**Benefits**:

- Universal webhook endpoint
- Support any Git platform
- Extensible for custom providers

### 3. CQRS Pattern (Database)

```python
# Write operations explicit
db.record_deployment(...)
db.update_fraise_state(...)

# Read operations optimized
stats = db.get_deployment_stats()
history = db.get_recent_deployments()
```

**Benefits**:

- Clean separation of concerns
- Write operations are events
- Read views can be optimized independently

### 4. Repository Pattern (Configuration)

```python
# Config is a repository of fraise definitions
config = FraisierConfig("fraises.yaml")
fraise = config.get_fraise("my_api")
```

**Benefits**:

- Single source of truth
- Easy to swap implementations
- Testable

---

## Extension Points

### 1. Custom Deployer Type

```python
class CustomDeployer(BaseDeployer):
    def execute(self) -> DeploymentResult:
        # Your custom logic
        pass
```

### 2. Custom Git Provider

```python
class MyGitProvider(GitProvider):
    name = "mygit"

    def verify_webhook_signature(self, ...):
        # Your verification logic
        pass
```

### 3. Custom Deployment Provider

Future: Custom deployment target

```python
class CustomProvider(DeploymentProvider):
    def deploy(self, fraise: FraiseConfig, version: str):
        # Deploy to your target
        pass
```

---

## Error Handling

### Current

Basic exception handling:
```python
try:
    result = deployer.execute()
except subprocess.CalledProcessError as e:
    return DeploymentResult(success=False, error_message=str(e))
```

### Planned

Custom exception hierarchy:
```python
class FraiseQLError(Exception):
    """Base exception"""
    pass

class DeploymentError(FraiseQLError):
    """Deployment failed"""
    pass

class HealthCheckError(DeploymentError):
    """Health check failed"""
    pass

class RollbackError(DeploymentError):
    """Rollback failed"""
    pass
```

---

## Testing Strategy

### Unit Tests

- Deployers with mocked subprocess calls
- Git providers with mocked HTTP
- Config loading
- CLI commands

### Integration Tests

- Real database operations
- Full deployment flow
- Webhook routing

### E2E Tests

- Complete CLI scenarios
- Multi-step deployments
- Rollback scenarios

---

## Performance Considerations

### Database

- SQLite for simplicity
- PostgreSQL for production
- Views indexed for fast reads
- Cleanup old data periodically

### Webhook Processing

- Async background tasks (FastAPI + Starlette)
- No blocking on webhook response
- Retry logic for failures

### Git Operations

- Cache current version (file-based)
- Limit concurrent deployments (locks)
- Parallel-safe operations

---

## Security Considerations

### Webhook Verification

- ✅ All providers verify signatures
- ✅ Replay attack prevention via timestamps
- ⚠️ Need: Rate limiting, request ID tracking

### Configuration

- ✅ YAML configuration (no code execution)
- ✅ Environment variables for secrets
- ⚠️ Need: Encryption for sensitive config

### Deployment Execution

- ✅ Deployer runs as regular user (not root)
- ⚠️ Need: Sandbox environments, blast radius limits
- ⚠️ Need: Audit logging of all operations

---

## Related Documents

- **Reference Implementation**: `../README.md`
- **Development Guide**: `../.claude/CLAUDE.md`
- **Roadmap**: `../ROADMAP.md`
- **Setup Instructions**: `../DEVELOPMENT.md`
- **Product Requirements**: `./PRD.md`

---

**Last Updated**: 2026-01-22
