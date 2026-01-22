# Fraisier Testing Guide

Comprehensive testing strategy for Fraisier development.

---

## Overview

Fraisier uses three levels of testing:

1. **Unit Tests** - Individual components with mocks
2. **Integration Tests** - Components together with real database
3. **E2E Tests** - Complete scenarios with all dependencies

**Target**: 100% code coverage for Phase 1 work

---

## Setup

### Install Test Dependencies

```bash
pip install -e ".[dev]"

# Should install:
# - pytest
# - pytest-asyncio
# - pytest-cov
# - pytest-mock
# - ruff
```

### Test Structure

```
tests/
├── conftest.py                 # Shared fixtures
├── test_cli.py                 # CLI command tests
├── test_config.py              # Configuration tests
├── test_database.py            # Database tests
├── test_deployers.py           # Deployer tests
├── test_git_providers.py       # Git provider tests
├── integration/
│   ├── conftest.py
│   ├── test_deployment_flow.py # Full deployment workflow
│   └── test_webhook_flow.py    # Webhook-triggered deployment
└── e2e/
    ├── conftest.py
    ├── test_cli_workflow.py    # Complete CLI scenarios
    └── test_deployment_scenario.py
```

---

## Running Tests

### All Tests

```bash
pytest                      # Run all tests
pytest -v                   # Verbose output
pytest -x                   # Stop on first failure
pytest -k "deploy"          # Run only tests matching "deploy"
```

### With Coverage

```bash
pytest --cov=fraisier --cov-report=html
open htmlcov/index.html     # View coverage report
```

### Specific Test File

```bash
pytest tests/test_cli.py -v
```

### Specific Test Function

```bash
pytest tests/test_cli.py::test_list_command -v
```

### Parallel Execution

```bash
pip install pytest-xdist
pytest -n auto              # Run tests in parallel
```

### Watch Mode (Development)

```bash
pip install pytest-watch
ptw                         # Re-run tests on file changes
```

---

## Unit Tests

Test individual components in isolation with mocks.

### CLI Tests

**File**: `tests/test_cli.py`

```python
from click.testing import CliRunner
from fraisier.cli import main

def test_list_command():
    """Test 'fraisier list' command."""
    runner = CliRunner()
    result = runner.invoke(main, ["list"])
    assert result.exit_code == 0
    assert "my_api" in result.output

def test_deploy_dry_run():
    """Test deployment dry-run."""
    runner = CliRunner()
    result = runner.invoke(main, [
        "deploy", "my_api", "production", "--dry-run"
    ])
    assert result.exit_code == 0
    assert "DRY RUN" in result.output
```

### Configuration Tests

**File**: `tests/test_config.py`

```python
from fraisier.config import FraisierConfig
from pathlib import Path

def test_load_config(tmp_path):
    """Test configuration loading."""
    config_file = tmp_path / "fraises.yaml"
    config_file.write_text("""
fraises:
  test_api:
    type: api
    environments:
      production:
        name: test-api
        branch: main
""")

    config = FraisierConfig(str(config_file))
    fraise = config.get_fraise("test_api")
    assert fraise is not None
    assert "production" in fraise["environments"]

def test_branch_mapping():
    """Test branch to fraise mapping."""
    config = FraisierConfig("tests/fixtures/fraises.yaml")
    result = config.get_fraise_for_branch("main")
    assert result["fraise"] == "my_api"
    assert result["environment"] == "production"
```

### Deployer Tests

**File**: `tests/test_deployers.py`

```python
from unittest.mock import patch, MagicMock
from fraisier.deployers.api import APIDeployer
from fraisier.deployers.base import DeploymentStatus
import subprocess

def test_api_deployer_init():
    """Test APIDeployer initialization."""
    config = {
        "app_path": "/var/www/api",
        "systemd_service": "api.service",
        "git_repo": "https://github.com/user/api.git"
    }
    deployer = APIDeployer(config)
    assert deployer.app_path == "/var/www/api"
    assert deployer.systemd_service == "api.service"

@patch("subprocess.run")
def test_get_current_version(mock_run):
    """Test getting current deployed version."""
    mock_run.return_value = MagicMock(
        stdout="abc123def456\n",
        returncode=0
    )

    deployer = APIDeployer({"app_path": "/var/www/api"})
    version = deployer.get_current_version()

    assert version == "abc123de"  # First 8 chars
    mock_run.assert_called_once()

@patch("subprocess.run")
def test_execute_deployment(mock_run):
    """Test execution flow."""
    # Mock git pull success
    mock_run.return_value = MagicMock(
        stdout="",
        returncode=0
    )

    deployer = APIDeployer({
        "app_path": "/var/www/api",
        "systemd_service": "api.service"
    })

    result = deployer.execute()

    assert result.success
    assert result.status == DeploymentStatus.SUCCESS
```

### Git Provider Tests

**File**: `tests/test_git_providers.py`

```python
from fraisier.git.github import GitHub
from fraisier.git.base import WebhookEvent
import hmac
import hashlib
import json

def test_github_signature_verification():
    """Test GitHub webhook signature verification."""
    secret = b"test-secret"
    payload = b'{"test": "data"}'

    # Create correct signature
    signature = "sha256=" + hmac.new(
        secret,
        payload,
        hashlib.sha256
    ).hexdigest()

    provider = GitHub({"webhook_secret": secret.decode()})

    # Verify signature
    assert provider.verify_webhook_signature(payload, {
        "X-Hub-Signature-256": signature
    })

def test_github_parse_push_event():
    """Test parsing GitHub push event."""
    provider = GitHub({"webhook_secret": "secret"})

    payload = {
        "ref": "refs/heads/main",
        "repository": {"full_name": "user/repo"},
        "pusher": {"name": "user"},
        "head_commit": {"id": "abc123"}
    }
    headers = {"X-GitHub-Event": "push"}

    event = provider.parse_webhook_event(headers, payload)

    assert event.branch == "main"
    assert event.commit_sha == "abc123"
    assert event.is_push
```

---

## Integration Tests

Test components together with real database.

**File**: `tests/integration/test_deployment_flow.py`

```python
import pytest
from fraisier.database import get_connection, init_database
from fraisier.deployers.api import APIDeployer
from fraisier.deployers.base import DeploymentStatus
from unittest.mock import patch

@pytest.fixture
def test_db(tmp_path):
    """Provide test database."""
    db_path = tmp_path / "test.db"

    # Initialize schema
    from fraisier.database import _execute_script
    _execute_script(str(db_path), "... schema SQL ...")

    yield str(db_path)

def test_full_deployment_flow(test_db):
    """Test complete deployment recorded in database."""
    config = {
        "app_path": "/var/www/api",
        "systemd_service": "api.service"
    }

    with patch("subprocess.run"):
        deployer = APIDeployer(config)
        result = deployer.execute()

    # Verify result
    assert result.success
    assert result.status == DeploymentStatus.SUCCESS

    # Verify recorded in database
    from fraisier.database import get_db
    db = get_db()
    history = db.get_recent_deployments(limit=1)

    assert len(history) == 1
    assert history[0]["status"] == "success"

def test_webhook_event_recording(test_db):
    """Test webhook events are recorded."""
    from fraisier.database import Database

    db = Database(test_db)
    db.record_webhook_event(
        event_type="push",
        branch="main",
        commit_sha="abc123",
        sender="user",
        payload={"test": "data"}
    )

    events = db.get_recent_webhooks(limit=1)
    assert len(events) == 1
    assert events[0]["branch"] == "main"
```

---

## E2E Tests

Test complete scenarios end-to-end.

**File**: `tests/e2e/test_cli_workflow.py`

```python
import pytest
from click.testing import CliRunner
from fraisier.cli import main
from pathlib import Path
import tempfile

@pytest.fixture
def test_config(tmp_path):
    """Create test fraises.yaml."""
    config_file = tmp_path / "fraises.yaml"
    config_file.write_text("""
git:
  provider: github
  github:
    webhook_secret: test-secret

fraises:
  test_api:
    type: api
    environments:
      development:
        app_path: /tmp/test-api
        systemd_service: test-api.service
""")
    return str(config_file)

def test_complete_cli_workflow(test_config, monkeypatch):
    """Test complete CLI workflow."""
    runner = CliRunner()

    # Use test config
    monkeypatch.setenv("FRAISIER_CONFIG", test_config)

    # List fraises
    result = runner.invoke(main, ["list"])
    assert result.exit_code == 0
    assert "test_api" in result.output

    # Validate config
    result = runner.invoke(main, ["config", "validate"])
    assert result.exit_code == 0

    # Try dry-run deploy
    result = runner.invoke(main, [
        "deploy", "test_api", "development", "--dry-run"
    ])
    assert "DRY RUN" in result.output
```

---

## Test Fixtures

Reusable test fixtures in `tests/conftest.py`:

```python
import pytest
from pathlib import Path
from fraisier.config import FraisierConfig

@pytest.fixture
def sample_config(tmp_path):
    """Create sample fraises.yaml."""
    config_file = tmp_path / "fraises.yaml"
    config_file.write_text("""
git:
  provider: github
  github:
    webhook_secret: test-secret

fraises:
  my_api:
    type: api
    description: Test API
    environments:
      development:
        app_path: /tmp/api
        systemd_service: api.service
""")
    return FraisierConfig(str(config_file))

@pytest.fixture
def cli_runner():
    """Provide CLI test runner."""
    from click.testing import CliRunner
    return CliRunner()

@pytest.fixture
def mock_subprocess(monkeypatch):
    """Mock subprocess calls."""
    from unittest.mock import MagicMock
    mock_run = MagicMock(return_value=MagicMock(
        stdout="test output",
        returncode=0
    ))
    monkeypatch.setattr("subprocess.run", mock_run)
    return mock_run
```

---

## Mocking Strategies

### Mock External Calls

```python
from unittest.mock import patch, MagicMock

@patch("requests.get")
def test_health_check(mock_get):
    """Mock HTTP health check."""
    mock_get.return_value = MagicMock(status_code=200)

    # Your test code
    assert mock_get.called
```

### Mock Subprocess

```python
@patch("subprocess.run")
def test_git_pull(mock_run):
    """Mock git operations."""
    mock_run.return_value = MagicMock(
        returncode=0,
        stdout="Already up to date.\n"
    )

    # Your test code
```

### Mock Database

```python
@patch("sqlite3.connect")
def test_db_operations(mock_connect):
    """Mock database connection."""
    mock_db = MagicMock()
    mock_connect.return_value = mock_db

    # Your test code
```

---

## Coverage Requirements

### Target Coverage

- **Overall**: 90%+ (Phase 1)
- **Critical paths**: 100%
  - `deployers/api.py` execute()
  - `deployers/etl.py` execute()
  - `deployers/scheduled.py` execute()
  - `git/` all providers
  - `cli.py` main commands
  - `database.py` core operations

### Generate Report

```bash
pytest --cov=fraisier --cov-report=html --cov-report=term

# View detailed report
open htmlcov/index.html
```

### Coverage by Module

```bash
pytest --cov=fraisier --cov-report=term-missing

# Shows which lines aren't covered
```

---

## Performance Testing

### Benchmark Tests

```python
import pytest

@pytest.mark.benchmark
def test_deployment_speed(benchmark):
    """Benchmark deployment execution."""
    deployer = APIDeployer({...})
    result = benchmark(deployer.execute)
    assert result.success
```

Run benchmarks:

```bash
pytest --benchmark-only
pytest --benchmark-compare
```

---

## Continuous Integration

### Pre-commit Checks

```bash
# Before committing
pytest -x                           # Stop on first failure
ruff check fraisier/ && ruff format fraisier/ --check
```

### GitHub Actions

See `.github/workflows/fraisier-ci.yml` for full CI setup.

Runs:
1. Linting (ruff)
2. Tests (pytest)
3. Coverage report
4. Build package

---

## Test Checklist

Before submitting PR:

- [ ] All tests pass: `pytest -v`
- [ ] Coverage >90%: `pytest --cov`
- [ ] Linting passes: `ruff check`
- [ ] Format correct: `ruff format --check`
- [ ] No TODOs without issues
- [ ] New functions have tests
- [ ] New code is documented

---

## Debugging Tests

### Run with Print Output

```bash
pytest -s                   # Don't capture print output
pytest -vv                  # Very verbose
```

### Pause in Test

```python
import pdb

def test_something():
    deployer = APIDeployer({...})
    pdb.set_trace()         # Execution pauses here
    result = deployer.execute()
```

### Inspect Database in Test

```python
def test_database(test_db):
    from fraisier.database import get_db
    db = get_db()

    # Inspect state
    result = db.get_recent_deployments()
    print(result)           # Use pytest -s to see output
```

---

## Common Issues

### Tests Fail with Import Error

```bash
# Reinstall package in dev mode
pip install -e ".[dev]"
```

### Database Locked

```bash
# Kill hanging processes
pkill -f pytest
rm -f fraisier.db
```

### Fixture Not Found

```bash
# Ensure conftest.py is in same directory
# Pytest auto-discovers conftest.py
```

---

## References

- **Testing Framework**: [pytest documentation](https://docs.pytest.org)
- **Mocking**: [unittest.mock documentation](https://docs.python.org/3/library/unittest.mock.html)
- **Click Testing**: [Click documentation](https://click.palletsprojects.com)

---

**Last Updated**: 2026-01-22
