# Phase 1 Implementation Plan: Foundation & Core Deployment

**Objective**: Make Fraisier deployable and testable MVP

**Timeline**: 1-2 weeks

**Success Criteria**:
- ✅ CLI commands work (list, deploy, status, history, stats)
- ✅ Deployers execute successfully (API, ETL, Scheduled)
- ✅ Database records deployments accurately
- ✅ 80+ tests with 100% coverage on critical paths
- ✅ Webhook events trigger deployments
- ✅ Ruff linting passes
- ✅ Type checking passes
- ✅ CI/CD pipeline works

---

## Current State Assessment

### ✅ What's Already Working

1. **CLI Structure** (cli.py - 70% done)
   - ✅ Commands defined: list, deploy, status, history, stats, webhooks
   - ✅ Configuration loading
   - ✅ Rich output formatting
   - ⚠️ Status checking incomplete (TODOs at lines 200, 226)
   - ⚠️ Deployment output needs verification

2. **Deployers** (35% done overall)
   - ✅ **APIDeployer** (70% done)
     - ✅ Structure: git pull, migrations, systemctl restart, health check
     - ⚠️ Helper methods incomplete (_run_migrations, etc.)
     - ⚠️ Rollback partially done
   - ✅ **ETLDeployer** (40% done)
     - ✅ Basic structure
     - ⚠️ Not much logic
   - ✅ **ScheduledDeployer** (40% done)
     - ✅ Basic timer management
     - ⚠️ Limited testing capability

3. **Database** (80% done)
   - ✅ Schema initialized (CQRS tables + views)
   - ✅ Connection management
   - ✅ State operations partially done
   - ⚠️ Deployment recording methods incomplete
   - ⚠️ Webhook event recording incomplete

4. **Git Providers** (80% done)
   - ✅ All providers implemented (GitHub, GitLab, Gitea, Bitbucket)
   - ✅ Signature verification
   - ✅ Webhook event parsing

5. **Webhook Handler** (5% done)
   - ✅ FastAPI app structure
   - ✅ execute_deployment stub
   - ❌ process_webhook_event incomplete
   - ❌ No @app.post routes
   - ❌ No background task execution

6. **Tests** (0% done)
   - ❌ No test files
   - ❌ No conftest.py
   - ❌ No fixtures

---

## Phase 1 Breakdown: 3 Subphases

### 1.1: Complete Core Implementations (Days 1-3)

#### Task 1.1.1: Complete APIDeployer Implementation

**File**: `fraisier/deployers/api.py`

**Current State**: 70% (lines 1-200 read, stubs after)

**Missing Implementation**:

1. **`_run_migrations()` method (lines 129-142)**
   - Currently: `pass` statements only
   - Needs:
     ```python
     def _run_migrations(self) -> None:
         strategy = self.database_config.get("strategy", "apply")
         if strategy == "rebuild":
             # For dev/staging: drop and recreate
             # Implementation depends on migration tool (confiture, alembic, etc.)
             # For now: log warning
             logger.warning("Database rebuild not yet implemented")
         elif strategy == "apply":
             # For production: safe migrations only
             # Call migration tool (confiture, alembic, etc.)
             # subprocess.run(["confiture", "build"], cwd=self.app_path, check=True)
             logger.info("Migrations would be run here (tool TBD)")
     ```

2. **Complete `rollback()` method (lines 181-200+)**
   - Current: Partially implemented (only git checkout shown)
   - Needs:
     - Service restart after git checkout
     - Health check after rollback
     - Database rollback (if applicable)
     - Proper error handling

3. **Verify all subprocess calls**
   - ✅ `_git_pull()` looks complete
   - ✅ `_restart_service()` looks complete
   - ✅ `_wait_for_health()` looks complete
   - 🔍 Need to verify these actually work

**Tests Needed**: (in `tests/test_deployers.py`)
```python
# Test each method with mocked subprocess
- test_api_deployer_get_current_version()
- test_api_deployer_get_latest_version()
- test_api_deployer_execute_success()
- test_api_deployer_execute_failure()
- test_api_deployer_git_pull()
- test_api_deployer_run_migrations_rebuild()
- test_api_deployer_run_migrations_apply()
- test_api_deployer_restart_service()
- test_api_deployer_wait_for_health_success()
- test_api_deployer_wait_for_health_failure()
- test_api_deployer_rollback_success()
- test_api_deployer_rollback_failure()
- test_api_deployer_health_check()
```

#### Task 1.1.2: Complete ETLDeployer Implementation

**File**: `fraisier/deployers/etl.py`

**Current State**: 40% complete

**Analysis**:
- Structure is reasonable (lines 1-94)
- Missing: More robust error handling, logging, actual ETL execution
- The deployer mostly verifies the script exists, which is probably OK for MVP

**Changes Needed**:
1. Consider if more logic needed (update docs if scope is limited)
2. Add better error messages
3. Verify script execution (optional - might be out of scope)

**Tests Needed**: (in `tests/test_deployers.py`)
```python
- test_etl_deployer_get_current_version()
- test_etl_deployer_execute_success()
- test_etl_deployer_execute_script_not_found()
- test_etl_deployer_execute_failure()
```

#### Task 1.1.3: Complete ScheduledDeployer Implementation

**File**: `fraisier/deployers/scheduled.py`

**Current State**: 40% complete

**Changes Needed**:
1. Verify systemd timer operations are correct
2. Add error handling for missing timers
3. Consider rollback strategy (might be N/A for scheduled jobs)

**Tests Needed**: (in `tests/test_deployers.py`)
```python
- test_scheduled_deployer_get_current_version()
- test_scheduled_deployer_is_deployment_needed()
- test_scheduled_deployer_execute_success()
- test_scheduled_deployer_execute_failure()
- test_scheduled_deployer_health_check()
```

---

### 1.2: Complete Database Layer (Days 2-3)

#### Task 1.2.1: Complete FraisierDB Methods

**File**: `fraisier/database.py` (lines 155-400+)

**Missing Methods** (currently stubs or incomplete):

```python
def record_deployment(
    self,
    fraise: str,
    environment: str,
    status: str,
    triggered_by: str,
    old_version: str | None = None,
    new_version: str | None = None,
    git_branch: str | None = None,
    git_commit: str | None = None,
) -> int:
    """Record a deployment. Returns deployment ID."""
    # INSERT into tb_deployment
    # Return inserted row ID

def get_recent_deployments(
    self,
    limit: int = 20,
    fraise: str | None = None,
    environment: str | None = None,
) -> list[dict[str, Any]]:
    """Get recent deployments from v_deployment_history."""
    # Query v_deployment_history with filters and limit

def get_deployment_stats(
    self,
    fraise: str | None = None,
    days: int = 30,
) -> dict[str, Any]:
    """Get deployment statistics."""
    # Query stats from v_deployment_stats
    # Filter by fraise and date

def record_webhook_event(
    self,
    event_type: str,
    branch: str | None = None,
    commit_sha: str | None = None,
    sender: str | None = None,
    payload: dict | None = None,
) -> int:
    """Record a webhook event. Returns event ID."""

def get_recent_webhooks(self, limit: int = 10) -> list[dict[str, Any]]:
    """Get recent webhook events."""

def link_webhook_to_deployment(self, webhook_id: int, deployment_id: int) -> None:
    """Link a webhook event to a deployment."""
```

**Tests Needed**: (in `tests/test_database.py`)
```python
# Fixtures first
@pytest.fixture
def test_db(tmp_path):
    """Create test database."""
    # Initialize with schema, return path

# Then tests
- test_record_deployment()
- test_update_fraise_state()
- test_get_fraise_state()
- test_get_recent_deployments()
- test_get_recent_deployments_filtered()
- test_get_deployment_stats()
- test_record_webhook_event()
- test_get_recent_webhooks()
- test_link_webhook_to_deployment()
```

---

### 1.3: Complete Webhook Handler & Tests (Days 3-4)

#### Task 1.3.1: Complete Webhook Handler

**File**: `fraisier/webhook.py` (lines 143+)

**Missing**:

1. **Complete `process_webhook_event()` function** (started at line 143)
   ```python
   def process_webhook_event(...) -> dict[str, Any]:
       """Process normalized webhook event.

       Steps:
       1. Record webhook event in database
       2. Map branch to fraise/environment using config
       3. If match found, trigger background deployment
       4. Return status
       """
   ```

2. **Add FastAPI routes**:
   ```python
   @app.post("/webhook")
   async def receive_webhook(request: Request, background_tasks: BackgroundTasks):
       """Universal webhook endpoint."""
       # Parse headers
       # Get provider (auto-detect or from query param)
       # Get provider instance
       # Verify signature
       # Parse event
       # Process event
       # Return response

   @app.get("/health")
   async def health_check():
       """Health check endpoint."""
       return {"status": "healthy"}

   @app.get("/providers")
   async def list_providers():
       """List available Git providers."""
       return {"providers": [...]}
   ```

3. **Implement background execution**:
   - Currently: `background_tasks.add_task(execute_deployment, ...)`
   - Needs: Proper async handling

**Tests Needed**: (in `tests/test_webhook.py`)
```python
- test_webhook_github_push_event()
- test_webhook_gitlab_push_event()
- test_webhook_gitea_push_event()
- test_webhook_signature_verification()
- test_webhook_signature_invalid()
- test_webhook_branch_mapping()
- test_webhook_branch_no_mapping()
- test_webhook_deployment_triggered()
- test_webhook_health_check()
- test_webhook_list_providers()
```

#### Task 1.3.2: Create Test Infrastructure

**Files**:
- `tests/__init__.py` (new)
- `tests/conftest.py` (new)
- `tests/fixtures.py` (new - optional)

**Contents**:

`conftest.py`:
```python
import pytest
from pathlib import Path
from fraisier.config import FraisierConfig
from fraisier.database import FraisierDB
from click.testing import CliRunner

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
  test_api:
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
    return CliRunner()

@pytest.fixture
def test_db(tmp_path):
    """Create test database."""
    db_path = tmp_path / "test.db"
    # Initialize schema
    db = FraisierDB(str(db_path))
    return db
```

---

### 1.4: Write Comprehensive Tests (Days 4-5)

#### Test File Structure:

```
tests/
├── conftest.py              # Shared fixtures
├── test_cli.py              # CLI command tests (15 tests)
├── test_config.py           # Configuration tests (8 tests)
├── test_database.py         # Database tests (12 tests)
├── test_deployers.py        # Deployer tests (30 tests)
├── test_git_providers.py    # Git provider tests (15 tests)
├── test_webhook.py          # Webhook tests (10 tests)
└── integration/
    ├── test_deployment_flow.py    # Full deployment (5 tests)
    └── test_webhook_flow.py       # Webhook deployment (3 tests)
```

**Total Tests Target**: 80+ tests

**Coverage Target**: 100% on critical paths:
- All deployer execute() methods
- All database record/query methods
- CLI commands
- Git provider signature verification
- Webhook routing

#### Test Writing Strategy:

1. **Unit tests** (60 tests): Individual components with mocks
   - Mock subprocess.run, requests.get, etc.
   - Test happy path and error cases
   - Test edge cases (missing config, malformed input, etc.)

2. **Integration tests** (15 tests): Components with real database
   - Use tmp_path for test database
   - Verify database state after operations
   - Deployment recording and retrieval

3. **E2E tests** (5 tests): Complete scenarios
   - CLI deploy command end-to-end
   - Webhook → deployment end-to-end

---

### 1.5: Fix CLI Status Commands (Days 5)

#### Task 1.5.1: Implement Status Checking

**File**: `fraisier/cli.py` (lines 200, 226)

**Current**:
```python
# TODO: Add actual version/health checking once deployers are complete
# TODO: Implement actual status checking
```

**Implementation**:
```python
def status(ctx, fraise, environment):
    config = ctx.obj["config"]
    fraise_config = config.get_fraise_environment(fraise, environment)

    # Get deployer
    deployer = _get_deployer(fraise_config.get("type"), fraise_config)

    # Get current version
    current = deployer.get_current_version()

    # Get health
    is_healthy = deployer.health_check()

    # Get database state
    db = get_db()
    state = db.get_fraise_state(fraise, environment)

    # Display
    console.print(f"Fraise: {fraise}")
    console.print(f"Environment: {environment}")
    console.print(f"Current version: {current}")
    console.print(f"Status: {'[green]healthy[/green]' if is_healthy else '[red]unhealthy[/red]'}")
    console.print(f"Last deployed: {state.get('last_deployed_at')}")
```

---

## Implementation Order

### Week 1

**Day 1-2: Deployers**
1. Complete APIDeployer.execute() and helpers ✅
2. Complete ETLDeployer ✅
3. Complete ScheduledDeployer ✅

**Day 2-3: Database**
1. Implement all FraisierDB methods ✅
2. Test database operations ✅

**Day 3: Webhook Handler**
1. Complete process_webhook_event() ✅
2. Add FastAPI routes ✅
3. Implement background task execution ✅

### Week 2

**Day 4-5: Tests**
1. Create test infrastructure (conftest.py, fixtures) ✅
2. Write unit tests (50+ tests) ✅
3. Write integration tests (15+ tests) ✅
4. Write E2E tests (5+ tests) ✅

**Day 5: Polish**
1. Fix CLI status commands ✅
2. Run all tests and achieve 90%+ coverage ✅
3. Ruff linting pass ✅
4. Type checking pass ✅
5. Commit and prepare for Phase 2 ✅

---

## Implementation Patterns

### Error Handling Pattern

```python
from dataclasses import dataclass

@dataclass
class DeploymentResult:
    success: bool
    status: DeploymentStatus
    old_version: str | None = None
    new_version: str | None = None
    duration_seconds: float = 0.0
    error_message: str | None = None
    details: dict = field(default_factory=dict)

# Always return DeploymentResult, never raise
try:
    # do work
except Exception as e:
    logger.exception(...)
    return DeploymentResult(success=False, status=DeploymentStatus.FAILED, error_message=str(e))
```

### Database Pattern (CQRS)

```python
# Write (Commands)
db.record_deployment(...)  # INSERT
db.update_fraise_state(...)  # INSERT OR UPDATE

# Read (Queries)
deployments = db.get_recent_deployments()  # SELECT from v_deployment_history
```

### Testing Pattern with Mocks

```python
@patch("subprocess.run")
def test_something(mock_run):
    mock_run.return_value = MagicMock(stdout="output", returncode=0)

    # test code

    assert mock_run.called
```

---

## Definition of Done for Phase 1

### Code Quality
- [ ] All code has type hints (Python 3.10+ style)
- [ ] All public functions have docstrings
- [ ] No TODOs without associated issues
- [ ] Ruff linting: 0 issues
- [ ] Code formatted with Ruff

### Testing
- [ ] 80+ tests written
- [ ] 90%+ code coverage
- [ ] All critical paths at 100% coverage
- [ ] Tests pass locally
- [ ] CI/CD green (GitHub Actions)

### Functionality
- [ ] `fraisier list` works
- [ ] `fraisier deploy` works (dry-run and actual)
- [ ] `fraisier status` shows real status
- [ ] `fraisier history` shows deployments
- [ ] `fraisier stats` shows statistics
- [ ] Webhook server starts
- [ ] Webhook events trigger deployments
- [ ] Database records all operations

### Documentation
- [ ] All changes documented in code
- [ ] ROADMAP.md updated with progress
- [ ] No new TODOs added without explanation

### Version Control
- [ ] Changes committed with scope prefix: `feat(fraisier): ...`
- [ ] Commit messages are clear and descriptive
- [ ] Branch: `feature/fraisier/phase-1-foundation` (or similar)

---

## Known Unknowns / Decisions Needed

1. **Database Migrations**
   - What tool should we use? (confiture, alembic, raw SQL?)
   - Decision: For MVP, use raw SQL in _run_migrations() as placeholder

2. **Background Task Execution**
   - How long to wait for deployment?
   - Should we retry on failure?
   - Decision: Use BackgroundTasks from FastAPI for now (simple, works)

3. **Health Check Defaults**
   - Max attempts: 10 (current) - good?
   - Delay: 3 seconds - good?
   - Decision: Keep current, can tune later

4. **Error Reporting**
   - Should we create custom exception hierarchy?
   - For MVP: No, use generic Exception + logging

---

## Success Metrics

**After Phase 1, you should have**:

✅ A deployable CLI that:
- Lists fraises from YAML
- Deploys to test environments
- Shows deployment status
- Displays deployment history

✅ A working webhook server that:
- Accepts GitHub/GitLab/Gitea webhooks
- Routes to correct fraise/environment
- Triggers background deployments
- Records events in database

✅ Comprehensive test suite with:
- 80+ tests
- 90%+ coverage
- All critical paths verified
- CI/CD automation

✅ Professional code quality:
- Type hints throughout
- Clear documentation
- Ruff linting pass
- No TODOs without issues

---

**Next Step**: Review this plan and clarify any unknowns, then begin implementation!
