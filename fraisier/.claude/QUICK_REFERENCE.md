# Fraisier Quick Reference

Fast lookup for common commands, patterns, and questions.

---

## Commands at a Glance

### Testing
```bash
pytest -v                                    # Run all tests
pytest tests/test_deployers.py -v            # Run deployer tests only
pytest -k "test_api_deployer" -v             # Run specific tests
pytest --cov=fraisier --cov-report=html      # Coverage report
pytest -s -v                                 # Show print output
```

### Code Quality
```bash
ruff format fraisier/                        # Format code
ruff check fraisier/                         # Check for issues
ruff check fraisier/ --fix                   # Auto-fix issues
```

### Git
```bash
git log --oneline -20                        # Last 20 commits
git log -p fraisier/database.py              # Changes to database file
git diff HEAD fraisier/                      # Uncommitted changes
git status                                   # Current status
```

### Database
```bash
sqlite3 fraisier.db "SELECT * FROM v_fraise_status;"
sqlite3 fraisier.db "SELECT * FROM v_deployment_history LIMIT 5;"
sqlite3 fraisier.db ".schema tb_deployment"  # Show table schema
```

---

## Code Patterns

### Adding a New CLI Command

```python
# In fraisier/cli.py
@main.command()
@click.argument("fraise")
@click.option("--verbose", is_flag=True)
@click.pass_context
def my_command(ctx: click.Context, fraise: str, verbose: bool) -> None:
    """Brief description."""
    config = ctx.obj["config"]
    # implementation
```

### Adding a Deployer Type

```python
# In fraisier/deployers/my_type.py
from .base import BaseDeployer, DeploymentResult

class MyTypeDeployer(BaseDeployer):
    def get_current_version(self) -> str | None:
        return ...

    def execute(self) -> DeploymentResult:
        return ...

    def rollback(self, to_version: str | None = None) -> DeploymentResult:
        return ...

    def health_check(self) -> bool:
        return ...
```

### Writing Tests

```python
# In tests/test_my_feature.py
def test_feature_works(test_db, mock_subprocess):
    """Describe what you're testing."""
    # Arrange
    config = {...}

    # Act
    result = do_something(config)

    # Assert
    assert result.success is True
    assert result.status == "success"
```

### Database Queries

```python
# In FraisierDB methods
with get_connection() as conn:
    cursor = conn.execute(
        "SELECT * FROM tb_deployment WHERE fk_fraise_state = ?",
        (pk_fraise_state,)
    )
    return [dict(row) for row in cursor.fetchall()]
```

---

## Common Issues

### "AttributeError: module 'subprocess' has no attribute 'run'"

**Fix**: Mock subprocess correctly in tests
```python
with patch("subprocess.run") as mock_run:
    mock_run.return_value = MagicMock(returncode=0, stdout="output")
    result = deployer.execute()
```

### "ModuleNotFoundError: No module named 'fraisier'"

**Fix**: Install in development mode
```bash
cd fraisier
pip install -e ".[dev]"
```

### "Database locked" errors

**Fix**: Close database connections properly
```python
# Good
with get_connection() as conn:
    # database operations
# Connection auto-closed

# Avoid
conn = sqlite3.connect(db_path)
# ... forgot to close
```

### "Tests have no coverage"

**Fix**: Run pytest with --cov flag
```bash
pytest --cov=fraisier --cov-report=html
# Open htmlcov/index.html in browser
```

---

## Type Hints Quick Ref

### Modern Style (Python 3.10+)
```python
# ✅ Good - use these
def process(items: list[str], config: dict[str, int] | None = None) -> str | None:
    status: bool = True
    results: dict[str, list[int]] = {}
    return value
```

### Old Style (Python 3.9)
```python
# ❌ Avoid in new code
from typing import Optional, List, Dict
def process(items: List[str], config: Optional[Dict[str, int]] = None) -> Optional[str]:
    pass
```

---

## Docstring Template

```python
def deploy_fraise(
    fraise: str,
    environment: str,
    force: bool = False,
) -> DeploymentResult:
    """Deploy a fraise to an environment.

    Args:
        fraise: Fraise name (e.g., "my_api", "etl")
        environment: Target environment (e.g., "production", "staging")
        force: Skip health checks and deploy anyway

    Returns:
        DeploymentResult with success/failure status

    Raises:
        FraiseNotFoundError: If fraise doesn't exist in config
        EnvironmentNotFoundError: If environment not configured
        HealthCheckError: If health check fails (unless force=True)
    """
```

---

## Git Commit Template

```bash
git commit -m "feat(fraisier): Add new deployer type

## Changes

- Added MyTypeDeployer implementation
- Added 5 unit tests
- Updated config validation

## Verification
✅ All tests pass
✅ ruff check clean
✅ Coverage maintained

## Related
Fixes #123
```

---

## Structure Reference

```
fraisier/
├── fraisier/                     # Package
│   ├── __init__.py
│   ├── cli.py                    # CLI entry point
│   ├── config.py                 # Configuration loading
│   ├── database.py               # SQLite layer
│   ├── webhook.py                # Webhook handler (WIP)
│   ├── deployers/
│   │   ├── base.py               # BaseDeployer interface
│   │   ├── api.py                # APIDeployer
│   │   ├── etl.py                # ETLDeployer
│   │   ├── scheduled.py          # ScheduledDeployer
│   │   └── __init__.py
│   └── git/
│       ├── base.py               # GitProvider interface
│       ├── github.py             # GitHub provider
│       ├── gitlab.py             # GitLab provider
│       ├── gitea.py              # Gitea provider
│       ├── bitbucket.py          # Bitbucket provider
│       ├── registry.py           # Provider registry
│       └── __init__.py
├── tests/
│   ├── __init__.py
│   ├── conftest.py               # Shared fixtures
│   ├── test_deployers.py         # Deployer tests
│   ├── test_database.py          # Database tests
│   ├── test_config.py            # Config tests
│   └── test_git_providers.py     # Git provider tests
├── docs/                         # User documentation
├── .claude/                      # Development guide
├── pyproject.toml                # Python project config
├── fraises.example.yaml          # Config template
└── fraisier.db                   # SQLite database (generated)
```

---

## Fixtures Reference

From `tests/conftest.py`:

```python
@pytest.fixture
def test_db(tmp_db_path: Path) -> FraisierDB:
    """Create test database with trinity schema."""
    # Use in tests: def test_something(test_db):

@pytest.fixture
def sample_config(tmp_path: Path) -> FraisierConfig:
    """Create sample fraises.yaml configuration."""
    # Use in tests: def test_something(sample_config):

@pytest.fixture
def mock_subprocess():
    """Mock subprocess.run for testing."""
    # Use in tests: def test_something(mock_subprocess):

@pytest.fixture
def mock_requests():
    """Mock requests.get for health checks."""
    # Use in tests: def test_something(mock_requests):
```

---

## Error Handling Pattern

```python
from fraisier.deployers.base import DeploymentResult, DeploymentStatus

def execute(self) -> DeploymentResult:
    """Execute deployment with proper error handling."""
    try:
        # Deployment logic
        return DeploymentResult(
            success=True,
            status=DeploymentStatus.SUCCESS,
            message="Deployment successful"
        )
    except Exception as e:
        logger.error(f"Deployment failed: {e}")
        return DeploymentResult(
            success=False,
            status=DeploymentStatus.FAILED,
            message=f"Deployment failed: {str(e)}"
        )
```

---

## Trinity Pattern Reminder

Every table follows this column order:
```sql
id → identifier → pk_* → fk_* → domain_columns → audit_columns
```

**Why?**

- `id`: Public UUID for sync across databases
- `identifier`: Business key for human-readable lookups
- `pk_*`: Internal primary key (INTEGER in SQLite, BIGINT in PostgreSQL)
- `fk_*`: Foreign keys always reference `pk_*`, never `id`
- Domain columns: Business logic data
- Audit columns: `created_at`, `updated_at` for tracking

---

## Phase 1 Status Checklist

- [x] APIDeployer (100% complete)
- [x] ETLDeployer (100% complete)
- [x] ScheduledDeployer (100% complete)
- [x] Deployer tests (26 tests)
- [x] FraisierDB (95%+ complete)
- [x] Database tests (24 tests)
- [x] Git providers (4 providers, 22 tests)
- [ ] Webhook handler (FastAPI routes - IN PROGRESS)
- [ ] Webhook tests (10 tests - PENDING)
- [ ] CLI status commands (PENDING)
- [ ] Final verification (PENDING)

---

## Need More Details?

| Topic | See |
|-------|-----|
| Development workflow | [CLAUDE.md](CLAUDE.md) |
| Phase 1 progress | [PHASE_1_PROGRESS.md](PHASE_1_PROGRESS.md) |
| Phase 1 tasks | [PHASE_1_IMPLEMENTATION_PLAN.md](PHASE_1_IMPLEMENTATION_PLAN.md) |
| Database architecture | [TRINITY_PATTERNS.md](TRINITY_PATTERNS.md) |
| System architecture | [../../docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md) |
| Testing strategy | [../../docs/TESTING.md](../../docs/TESTING.md) |
| Navigation hub | [00_START_HERE.md](00_START_HERE.md) |
| Documentation index | [INDEX.md](INDEX.md) |

---

**Created**: 2026-01-22
**Updated**: 2026-01-22
**Status**: Quick reference for common operations
