# Fraisier Development Setup Guide

Getting started with Fraisier development.

---

## Prerequisites

- **Python**: 3.11+ (check with `python --version`)
- **Git**: Any recent version
- **pip/uv**: Package manager (uv recommended)
- **SQLite3**: Usually included with Python

### Optional

- **Docker**: For testing Docker Compose deployments
- **Rust**: If building FraiseQL core from source
- **PostgreSQL**: For testing multi-database support (later phases)

---

## Initial Setup

### 1. Clone the Repository

```bash
# Clone FraiseQL monorepo (includes Fraisier)
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql/fraisier

# Or if you only want Fraisier (requires separate setup)
cd fraiseql/fraisier
```

### 2. Create Virtual Environment

```bash
# Using Python venv
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Or using uv (faster)
uv venv
source .venv/bin/activate
```

### 3. Install Dependencies

```bash
# Development install (editable mode + dev dependencies)
pip install -e ".[dev]"

# Or with uv
uv pip install -e ".[dev]"
```

### 4. Verify Installation

```bash
# Check CLI works
fraisier --version
# Output: Fraisier v0.1.0

# Check Python package imports
python -c "import fraisier; print(fraisier.__version__)"
# Output: 0.1.0

# Check database initializes
python -c "from fraisier.database import init_database; init_database(); print('OK')"
# Output: OK (creates fraisier.db)
```

---

## Project Structure

```
fraisier/
├── .claude/
│   └── CLAUDE.md              # Development instructions
├── fraisier/                  # Main Python package
│   ├── __init__.py
│   ├── cli.py                 # CLI interface (Click commands)
│   ├── config.py              # Configuration loader (fraises.yaml)
│   ├── database.py            # SQLite operations (CQRS pattern)
│   ├── webhook.py             # FastAPI webhook server
│   ├── deployers/
│   │   ├── base.py            # Abstract BaseDeployer class
│   │   ├── api.py             # API/web service deployer
│   │   ├── etl.py             # ETL/script deployer
│   │   └── scheduled.py       # Scheduled/cron job deployer
│   └── git/
│       ├── base.py            # Abstract GitProvider interface
│       ├── github.py          # GitHub provider
│       ├── gitlab.py          # GitLab provider
│       ├── gitea.py           # Gitea provider
│       ├── bitbucket.py       # Bitbucket provider
│       └── registry.py        # Provider registry
├── docs/
│   ├── PRD.md                 # Product requirements
│   ├── ARCHITECTURE.md        # Architecture deep-dive
│   └── DEPLOYMENT_GUIDE.md    # Operator guide
├── tests/
│   ├── conftest.py            # Pytest fixtures
│   ├── test_cli.py            # CLI tests
│   ├── test_config.py         # Config tests
│   ├── test_database.py       # Database tests
│   ├── test_deployers.py      # Deployer tests
│   ├── test_git_providers.py  # Git provider tests
│   └── integration/           # Integration tests
├── pyproject.toml             # Project metadata & dependencies
├── README.md                  # Quick start
├── ROADMAP.md                 # Development phases
├── DEVELOPMENT.md             # This file
└── fraises.example.yaml       # Configuration example
```

---

## Common Tasks

### Running Tests

```bash
# All tests
pytest

# With verbose output
pytest -v

# With coverage report
pytest --cov=fraisier --cov-report=html

# Specific test file
pytest tests/test_cli.py -v

# Specific test function
pytest tests/test_cli.py::test_list_command -v

# Tests matching pattern
pytest -k "deploy" -v
```

### Code Quality

```bash
# Check for issues (ruff linter)
ruff check fraisier/

# Format code
ruff format fraisier/

# Type checking
mypy fraisier/  # If mypy is installed

# All checks together
ruff check fraisier/ && ruff format fraisier/ --check
```

### Running CLI Commands

```bash
# List all fraises
fraisier list
fraisier list --flat

# Show configuration
fraisier config validate
fraisier config show

# Try a deployment (dry-run)
fraisier deploy my_api production --dry-run

# Check deployment history
fraisier history
fraisier history --fraise my_api --limit 10

# View statistics
fraisier stats
fraisier stats --fraise my_api --days 7

# See webhook events
fraisier webhooks
fraisier webhooks --limit 20
```

### Starting the Webhook Server

```bash
# Start webhook listener (development)
fraisier-webhook

# With environment variables
export FRAISIER_WEBHOOK_SECRET=test-secret
export FRAISIER_GIT_PROVIDER=github
export FRAISIER_HOST=0.0.0.0
export FRAISIER_PORT=8000
fraisier-webhook

# The server listens on http://localhost:8000
# POST /webhook - Universal endpoint (auto-detects Git provider)
# GET /health - Health check
# GET /providers - List available providers
```

### Database Inspection

```bash
# Connect to database
sqlite3 fraisier.db

# View schema
.schema

# View deployment history
SELECT * FROM tb_deployment LIMIT 5;

# View fraise status
SELECT * FROM v_fraise_status;

# View statistics
SELECT * FROM v_deployment_stats;

# Exit
.quit
```

---

## Configuration Setup

### Create Test Configuration

```bash
# Copy example configuration
cp fraises.example.yaml fraises.yaml

# Edit for your environment
vim fraises.yaml
```

### Minimal fraises.yaml

```yaml
git:
  provider: github
  github:
    webhook_secret: test-secret

fraises:
  test_api:
    type: api
    description: Test API
    environments:
      development:
        name: test-api-dev
        branch: develop
        app_path: /tmp/test-api
        systemd_service: test-api.service
        health_check:
          url: http://localhost:8000/health
          timeout: 30
```

---

## Development Workflow

### 1. Create Feature Branch

```bash
git checkout -b feature/fraisier/my-feature
```

### 2. Make Changes

Edit Python files, add tests:

```bash
# Example: Add new CLI command
vim fraisier/cli.py
vim tests/test_cli.py
```

### 3. Test Locally

```bash
# Run tests for your change
pytest tests/test_cli.py -v

# Run code quality checks
ruff check fraisier/ && ruff format fraisier/ --check
```

### 4. Commit

```bash
git add fraisier/cli.py tests/test_cli.py
git commit -m "feat(fraisier): Add new CLI command

Detailed description of what this does.
"
```

### 5. Push and Create PR

```bash
git push -u origin feature/fraisier/my-feature

# Create PR (if using GitHub CLI)
gh pr create --title "Add new CLI command" --body "Description"
```

---

## Debugging

### Enable Debug Logging

```python
import logging

# In your code:
logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)
logger.debug("This message will appear")
```

### Or from CLI:

```bash
# Python PYTHONPATH
PYTHONVERBOSE=2 fraisier list

# Or in code:
import logging
logging.getLogger("fraisier").setLevel(logging.DEBUG)
```

### Inspect Database

```bash
# Pretty print deployment history
sqlite3 fraisier.db << EOF
.headers on
.mode column
SELECT id, fraise, environment, status, started_at FROM tb_deployment LIMIT 10;
EOF
```

### Test Webhook Events

```bash
# Using curl to send test webhook
curl -X POST http://localhost:8000/webhook \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: push" \
  -H "X-Hub-Signature-256: sha256=test" \
  -d '{
    "ref": "refs/heads/main",
    "repository": {"full_name": "user/repo"},
    "pusher": {"name": "user"},
    "head_commit": {"id": "abc123"}
  }'
```

---

## Docker Setup (Optional)

### Build Docker Image

```bash
# From root fraiseql directory
docker build -f docker/Dockerfile.fraisier -t fraisier:latest .

# Run container
docker run -it -p 8000:8000 \
  -v $(pwd)/fraises.yaml:/etc/fraisier/fraises.yaml \
  -e FRAISIER_WEBHOOK_SECRET=test-secret \
  fraisier:latest
```

---

## Testing Strategies

### Unit Testing

Test individual components with mocks:

```python
def test_api_deployer_init():
    """Test APIDeployer initialization."""
    config = {
        "app_path": "/var/www/api",
        "systemd_service": "api.service"
    }
    deployer = APIDeployer(config)
    assert deployer.app_path == "/var/www/api"
```

### Integration Testing

Test components together with real (test) database:

```python
def test_deployment_recorded_in_db(tmp_db):
    """Test that deployments are recorded."""
    db = tmp_db
    db.record_deployment(
        fraise="my_api",
        environment="staging",
        status="success"
    )
    history = db.get_recent_deployments()
    assert len(history) == 1
```

### Mocking External Calls

```python
from unittest.mock import patch, MagicMock

def test_health_check_retry():
    """Test health check retries on timeout."""
    with patch("requests.get") as mock_get:
        mock_get.side_effect = [
            TimeoutError(),
            MagicMock(status_code=200)
        ]
        deployer = APIDeployer({...})
        # Retry logic should succeed on second attempt
```

---

## Git Workflow for Monorepo

### Branch Naming

```
feature/fraisier/<description>      # New features
bugfix/fraisier/<issue-number>      # Bug fixes
docs/fraisier/<topic>               # Documentation
refactor/fraisier/<component>       # Refactoring
```

### Commit Messages

```
feat(fraisier): Add new feature
fix(fraisier): Fix bug in component
docs(fraisier): Update README
refactor(fraisier): Simplify module
test(fraisier): Add test for feature
chore(fraisier): Update dependencies
```

### View Fraisier-Only History

```bash
# Commits affecting fraisier/ only
git log --all -- fraisier/

# Shorthand
git log -- fraisier/ | head -20
```

---

## Performance Tips

### Faster pytest Runs

```bash
# Run tests in parallel
pytest -n auto  # Requires pytest-xdist

# Run only failed tests from last run
pytest --lf

# Stop at first failure
pytest -x
```

### Faster Linting

```bash
# Check only changed files
ruff check --select F501  # Only import errors, e.g.

# Format with line-length for readability
ruff format --line-length 100 fraisier/
```

---

## Troubleshooting

### "fraises.yaml not found"

```bash
# Solution: Create config file
cp fraises.example.yaml fraises.yaml
vim fraises.yaml
```

### Import errors after changes

```bash
# Reinstall in development mode
pip install -e ".[dev]"

# Or if using uv
uv pip install -e ".[dev]"
```

### Database locked errors

```bash
# SQLite database is locked by another process
# Solution: Delete and recreate
rm fraisier.db
python -c "from fraisier.database import init_database; init_database()"
```

### Tests fail with "No module named fraisier"

```bash
# Run from fraisier directory
cd fraisier
pytest

# Or specify Python path
PYTHONPATH=. pytest
```

### Webhook server won't start

```bash
# Check port is available
lsof -i :8000  # On Linux/Mac

# Use different port
export FRAISIER_PORT=9000
fraisier-webhook
```

---

## IDE Setup

### VSCode

Create `.vscode/settings.json`:

```json
{
  "python.defaultInterpreterPath": "${workspaceFolder}/venv/bin/python",
  "python.linting.enabled": true,
  "python.linting.ruffEnabled": true,
  "[python]": {
    "editor.formatOnSave": true,
    "editor.defaultFormatter": "charliermarsh.ruff"
  }
}
```

### PyCharm

1. Open `fraisier/` as project
2. Configure interpreter: `fraisier/venv/bin/python`
3. Enable pytest runner: Settings → Tools → Python Integrated Tools
4. Install Ruff plugin for linting

---

## Next Steps

After setup:

1. Read `.claude/CLAUDE.md` for project standards
2. Read `ROADMAP.md` for what needs to be built
3. Check `tests/` for test examples
4. Run `fraisier --help` to see available commands
5. Try `fraisier list` with example config

---

## Getting Help

### In This Repository

- **Architecture questions**: See `docs/PRD.md`
- **Development standards**: See `.claude/CLAUDE.md`
- **What to build next**: See `ROADMAP.md`
- **Code examples**: Look in `tests/`

### About FraiseQL

- **Framework docs**: See parent `crates/README.md`
- **Framework architecture**: See parent `docs/`
- **Language bindings**: See `fraiseql-python/`, etc.

---

## Quick Reference

```bash
# Development setup
python -m venv venv && source venv/bin/activate
pip install -e ".[dev]"

# Common commands
pytest                           # Run tests
ruff check fraisier/            # Lint
ruff format fraisier/           # Format
fraisier list                   # Test CLI
fraisier-webhook                # Test webhook server

# Git workflow
git checkout -b feature/fraisier/my-feature
# ... make changes ...
pytest -v
ruff check fraisier/ && ruff format fraisier/ --check
git add ...
git commit -m "feat(fraisier): Description"
git push -u origin feature/fraisier/my-feature
```

---

**Last Updated**: 2026-01-22
**Maintainer**: FraiseQL Team
