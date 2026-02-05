# Fraisier Development Guide

## What is Fraisier?

**Fraisier** is THE canonical reference implementation of a FraiseQL application. It demonstrates:

1. **How to build a production FraiseQL service** - Shows best practices
2. **That FraiseQL actually works** - E2E test suite for the framework
3. **Deployment orchestration** - Reference tool for managing services

**Key distinction**: Fraisier is **APPLICATION code that USES FraiseQL**, not framework code.

---

## Context: Monorepo Structure

This repository contains both:

```
fraiseql/  (FraiseQL Framework)
├── crates/              → Rust engine
├── fraiseql-python/     → Python schema authoring
├── fraiseql-typescript/ → TypeScript schema authoring
│
└── fraisier/            ← YOU ARE HERE (Reference Implementation)
    ├── fraisier/        → Deployment orchestrator (Python package)
    ├── docs/            → Fraisier documentation
    ├── tests/           → Fraisier tests
    └── .claude/         → Development instructions (this file)
```

**Important**: When commits reference "Phase 7", they're about FraiseQL core (crates/), not Fraisier.

---

## Quick Start

```bash
# Work on Fraisier
cd fraiseql/fraisier

# Install for development
pip install -e ".[dev]"

# Run tests
pytest

# Run Fraisier CLI
fraisier list
fraisier deploy --help
```

---

## Development Standards

### Type Annotations

Use modern Python 3.10+ style:

```python
# ✅ Good
def deploy(fraise: str, environment: str) -> bool | None:
    config: dict[str, Any] = load_config()
    items: list[str] = get_items()
    return result

# ❌ Old style
from typing import Optional, Dict, Any, List
def deploy(fraise: str, environment: str) -> Optional[bool]:
    config: Dict[str, Any] = load_config()
```

### Code Quality

- **Linting**: Run `ruff check fraisier/` before commits
- **Format**: Use `ruff format fraisier/`
- **Tests**: 100% coverage for new code
- **Docstrings**: All public functions need docstrings

```python
def deploy(fraise: str, environment: str) -> DeploymentResult:
    """Deploy a fraise to an environment.

    Args:
        fraise: Fraise name (e.g., "my_api", "etl")
        environment: Target environment (e.g., "production")

    Returns:
        DeploymentResult with success/failure status

    Raises:
        FraiseNotFoundError: If fraise doesn't exist
        EnvironmentNotFoundError: If environment isn't configured
    """
```

---

## Architecture Principles

### 1. Separation of Concerns

```
CLI (cli.py)
  └─ Configuration (config.py)
      └─ Deployers (deployers/)
          ├─ API deployer
          ├─ ETL deployer
          └─ Scheduled deployer
  └─ Database (database.py)
      ├─ Write tables (tb_*)
      └─ Read views (v_*)
  └─ Git Providers (git/)
      ├─ GitHub
      ├─ GitLab
      ├─ Gitea
      └─ Bitbucket
```

**Rule**: Each module has one responsibility.

### 2. Interface-Based Design

All deployers implement `BaseDeployer`:

```python
class APIDeployer(BaseDeployer):
    def get_current_version(self) -> str | None: ...
    def get_latest_version(self) -> str | None: ...
    def execute(self) -> DeploymentResult: ...
    def rollback(self, to_version: str | None = None) -> DeploymentResult: ...
    def health_check(self) -> bool: ...
```

This allows:

- Easy provider swapping (mock for tests)
- New deployer types without changing CLI
- Consistent interface for all fraise types

### 3. CQRS Database Pattern

Write side (tb_*):
```sql
CREATE TABLE tb_deployment (
    id INTEGER PRIMARY KEY,
    fraise TEXT NOT NULL,
    status TEXT,  -- pending, in_progress, success, failed
    ...
);
```

Read side (v_*):
```sql
CREATE VIEW v_fraise_status AS
SELECT fraise, environment, current_version, status
FROM tb_fraise_state
LEFT JOIN tb_deployment ON ...;
```

**Principle**: Write operations are explicit commands, reads are optimized views.

---

## Development Workflow

### Adding a New CLI Command

1. **Add method to `cli.py`:**

```python
@main.command()
@click.argument("fraise")
@click.option("--verbose", is_flag=True)
@click.pass_context
def my_command(ctx: click.Context, fraise: str, verbose: bool) -> None:
    """Brief description of command."""
    config = ctx.obj["config"]
    # implementation
```

2. **Add tests in `tests/test_cli.py`:**

```python
def test_my_command(config, runner):
    result = runner.invoke(main, ["my_command", "my_api"])
    assert result.exit_code == 0
```

3. **Document in README.md**

---

### Adding a New Deployer Type

1. **Create `deployers/my_type.py`:**

```python
from .base import BaseDeployer, DeploymentResult

class MyTypeDeployer(BaseDeployer):
    def get_current_version(self) -> str | None:
        # implementation

    def execute(self) -> DeploymentResult:
        # implementation
```

2. **Register in `cli.py`:**

```python
def _get_deployer(fraise_type: str, ...):
    elif fraise_type == "my_type":
        from .deployers.my_type import MyTypeDeployer
        return MyTypeDeployer(fraise_config)
```

3. **Add tests in `tests/test_deployers.py`**

4. **Update `README.md`**

---

### Adding a New Git Provider

1. **Create `git/my_provider.py`:**

```python
from .base import GitProvider, WebhookEvent

class MyGitProvider(GitProvider):
    name = "mygit"

    def verify_webhook_signature(self, payload: bytes, headers: dict) -> bool:
        # implementation

    def parse_webhook_event(self, headers: dict, payload: dict) -> WebhookEvent:
        # implementation
```

2. **Register in `git/registry.py`:**

```python
from .my_provider import MyGitProvider
_PROVIDERS = {
    "github": GitHub,
    "mygit": MyGitProvider,
}
```

3. **Add tests in `tests/test_git_providers.py`**

---

## Testing Strategy

### Unit Tests (test_*.py)

Test individual components in isolation with mocks:

```python
def test_api_deployer_execute(tmp_path):
    """Test APIDeployer with mocked systemd."""
    config = {
        "app_path": str(tmp_path / "app"),
        "systemd_service": "test.service",
        "health_check": {"url": "http://localhost:8000/health"}
    }

    deployer = APIDeployer(config)

    # Mock git operations
    with patch("subprocess.run") as mock_run:
        mock_run.return_value = MagicMock(returncode=0)
        result = deployer.execute()

    assert result.success
    assert result.status == DeploymentStatus.SUCCESS
```

### Integration Tests (tests/integration/)

Test real database operations:

```python
def test_deployment_recording(db):
    """Test that deployments are recorded in database."""
    db.record_deployment(
        fraise="my_api",
        environment="production",
        status="success"
    )

    history = db.get_recent_deployments()
    assert len(history) == 1
    assert history[0]["fraise"] == "my_api"
```

### E2E Tests (tests/e2e/)

Test complete scenarios:

```python
def test_full_deployment_flow():
    """Test complete deploy → health check → record flow."""
    config = load_test_config()
    cli_runner = CliRunner()

    # Deploy
    result = cli_runner.invoke(deploy, ["my_api", "production"])
    assert result.exit_code == 0

    # Check history
    result = cli_runner.invoke(history, [])
    assert "my_api" in result.output
    assert "success" in result.output
```

---

## Common Tasks

### Run All Tests

```bash
cd fraisier
pytest -v                    # All tests with verbose output
pytest -k "test_deploy"      # Only tests matching "test_deploy"
pytest --cov               # With coverage report
```

### Format Code

```bash
cd fraisier
ruff format fraisier/        # Format code
ruff check fraisier/         # Check for issues
```

### Test a Specific Feature

```bash
pytest tests/test_deployers.py::test_api_deployer_execute -v
```

---

## Phase Planning

Fraisier has its own phase plan, independent from FraiseQL:

- **Phase 1** (v0.1.0): Complete deployer implementations + tests
- **Phase 2** (v0.2.0): Deployment providers (Coolify, Bare Metal, Docker Compose)
- **Phase 3** (v1.0.0): Production hardening (error handling, monitoring)
- **Phase 4** (v1.1.0): Multi-language implementations (TypeScript, Go, Rust)

See `ROADMAP.md` for detailed breakdown.

---

## Debugging

### Enable Verbose Logging

```python
import logging
logging.basicConfig(level=logging.DEBUG)

# Now all logs show up
logger.debug("This message appears")
```

### Database Inspection

```bash
# View deployment history
sqlite3 fraisier.db "SELECT * FROM v_deployment_history LIMIT 5;"

# View fraise status
sqlite3 fraisier.db "SELECT * FROM v_fraise_status;"
```

### Dry-Run Deployments

```bash
fraisier deploy my_api production --dry-run
```

---

## Dependencies & Tools

| Tool | Purpose | Version |
|------|---------|---------|
| Python | Runtime | 3.11+ |
| pytest | Testing | 8.0.0+ |
| ruff | Linting | 0.3.0+ |
| click | CLI framework | 8.1.0+ |
| rich | Terminal UI | 13.0.0+ |
| fastapi | Webhook server | 0.109.0+ |
| pyyaml | Config parsing | 6.0+ |

---

## Troubleshooting

### "fraises.yaml not found"

```bash
# Fraisier looks in these locations:
# 1. /opt/fraisier/fraises.yaml (production)
# 2. ./fraises.yaml (current directory)
# 3. ./config/fraises.yaml (subdirectory)
# 4. <package>/fraises.yaml (package directory)

# Solution: Create one of these files
cp fraises.example.yaml fraises.yaml
```

### Tests failing with database errors

```bash
# Delete cached database
rm -f fraisier.db

# Re-run tests
pytest
```

### Webhook not receiving events

```bash
# Check webhook server is running
fraisier-webhook

# Check configured secret matches webhook settings
echo $FRAISIER_WEBHOOK_SECRET

# Check logs
tail -f fraisier.log
```

---

## Code Review Checklist

Before submitting PRs, verify:

- [ ] Type hints on all new functions
- [ ] Docstrings on public methods
- [ ] Tests for new functionality (100% coverage)
- [ ] No TODOs without associated issues
- [ ] Ruff passes: `ruff check . && ruff format .`
- [ ] All tests pass: `pytest -v`
- [ ] Updated documentation if adding features
- [ ] Commit messages follow: `feat(fraisier): ...` format

---

## Resources

- **Framework**: See parent FraiseQL docs (crates/README.md, docs/)
- **Architecture**: See `ROADMAP.md` for phase plan
- **Design patterns**: See `docs/` for detailed architecture
- **Examples**: See `fraises.example.yaml` for configuration reference

---

**Remember**: Fraisier is the reference implementation. If something works in Fraisier, FraiseQL works. If it doesn't, we know the framework needs fixing.
