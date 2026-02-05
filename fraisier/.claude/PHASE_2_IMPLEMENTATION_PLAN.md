# Phase 2 Implementation Plan: Deployment Providers

**Objective**: Extend Fraisier to support multiple deployment target providers

**Status**: Starting

**Timeline**: 2-3 weeks

**Success Criteria**:

- ✅ Bare Metal provider fully implemented (SSH/systemd)
- ✅ Docker Compose provider fully implemented
- ✅ Coolify provider fully implemented
- ✅ Provider abstraction tested and working
- ✅ Deployment locks preventing concurrent deploys
- ✅ Pre-flight checks per provider
- ✅ 50+ tests for all providers
- ✅ Ruff linting passes
- ✅ Type checking passes

---

## Current State Assessment

### ✅ What's Ready (from Phase 1)

- Core deployer interface (BaseDeployer)
- CLI infrastructure
- Database layer with trinity pattern
- Webhook integration
- 99+ tests
- Documentation system

### ⚠️ What's Needed for Phase 2

- Provider abstraction layer
- Bare Metal deployment support
- Docker Compose integration
- Coolify API integration
- Deployment locking mechanism
- Provider-specific health checks
- 50+ new tests

---

## Phase 2 Breakdown: 3 Subphases

### 2.1: Provider Abstraction & Bare Metal (Days 1-5)

#### Task 2.1.1: Create Provider Abstraction Layer

**File**: `fraisier/providers/__init__.py` (NEW)

**Purpose**: Define provider interface and registry

**Implementation**:

```python
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any

@dataclass
class ProviderConfig:
    """Configuration for a provider."""
    name: str
    type: str  # bare_metal, docker_compose, coolify
    url: str | None = None
    api_key: str | None = None
    custom_fields: dict[str, Any] = None

class BaseProvider(ABC):
    """Base class for all deployment providers."""

    name: str
    type: str

    def __init__(self, config: ProviderConfig):
        self.config = config

    @abstractmethod
    def pre_flight_check(self) -> tuple[bool, str]:
        """Verify provider is accessible and configured."""
        pass

    @abstractmethod
    def deploy_service(
        self,
        service_name: str,
        version: str,
        config: dict[str, Any],
    ) -> DeploymentResult:
        """Deploy a service to the provider."""
        pass

    @abstractmethod
    def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get current status of a service."""
        pass

    @abstractmethod
    def rollback_service(
        self,
        service_name: str,
        to_version: str | None = None,
    ) -> DeploymentResult:
        """Rollback a service to previous version."""
        pass

    @abstractmethod
    def health_check(self, service_name: str) -> bool:
        """Check if service is healthy."""
        pass

    @abstractmethod
    def get_logs(self, service_name: str, lines: int = 100) -> str:
        """Get recent logs from service."""
        pass

class ProviderRegistry:
    """Registry for managing available providers."""

    _providers: dict[str, type[BaseProvider]] = {}

    @classmethod
    def register(cls, provider_class: type[BaseProvider]) -> None:
        """Register a provider."""
        cls._providers[provider_class.type] = provider_class

    @classmethod
    def get_provider(cls, provider_type: str, config: ProviderConfig) -> BaseProvider:
        """Get a provider instance."""
        if provider_type not in cls._providers:
            raise ValueError(f"Unknown provider: {provider_type}")
        return cls._providers[provider_type](config)

    @classmethod
    def list_providers(cls) -> list[str]:
        """List available provider types."""
        return list(cls._providers.keys())
```

#### Task 2.1.2: Implement Bare Metal Provider

**File**: `fraisier/providers/bare_metal.py` (NEW)

**Features**:

- SSH connection to remote servers
- Systemd service management
- Git pull on remote
- Health checks via HTTP/TCP
- Log retrieval via SSH
- Rollback support

**Key Methods**:
```python
class BareMetalProvider(BaseProvider):
    """Deploy to bare metal servers via SSH."""

    type = "bare_metal"

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self.ssh_key = config.custom_fields.get("ssh_key_path")
        self.ssh_user = config.custom_fields.get("ssh_user", "deploy")
        self.ssh_host = config.url

    def pre_flight_check(self) -> tuple[bool, str]:
        """Test SSH connection."""
        # Use paramiko or subprocess ssh to test connection
        # Return (success, message)

    def deploy_service(self, service_name: str, version: str, config: dict) -> DeploymentResult:
        """Deploy via SSH."""
        # 1. SSH to server
        # 2. cd to app_path
        # 3. git pull origin branch
        # 4. systemctl restart service
        # 5. Check health

    def get_logs(self, service_name: str, lines: int = 100) -> str:
        """Get systemd logs via SSH."""
        # journalctl -u service -n lines
```

**Implementation Steps**:

1. Use `subprocess` with `ssh` commands (no additional dependencies)
2. Implement SSH key-based authentication
3. Execute systemd commands on remote
4. Parse output and return structured results
5. Error handling for SSH failures

**Tests** (12 tests):

- pre_flight_check (success, failure)
- deploy_service (success, failure, health check)
- rollback_service (success, failure)
- get_logs (success, SSH failure)
- health_check (TCP, HTTP, failure)

#### Task 2.1.3: Create Deployment Lock Mechanism

**File**: `fraisier/locking.py` (NEW)

**Purpose**: Prevent concurrent deployments to same service

**Implementation**:
```python
class DeploymentLock:
    """Lock to prevent concurrent deployments."""

    def __init__(self, service_name: str, provider_name: str):
        self.service_name = service_name
        self.provider_name = provider_name
        self.lock_key = f"{provider_name}:{service_name}"

    def acquire(self, timeout: int = 300) -> bool:
        """Try to acquire lock."""
        # Use database to store lock
        # Set timeout

    def release(self) -> None:
        """Release lock."""
        # Remove from database

    def __enter__(self):
        if not self.acquire():
            raise DeploymentLockedError(self.service_name)
        return self

    def __exit__(self, *args):
        self.release()
```

**Usage**:
```python
with DeploymentLock(service_name, provider_name):
    result = provider.deploy_service(...)
```

---

### 2.2: Docker Compose Provider (Days 6-10)

#### Task 2.2.1: Implement Docker Compose Provider

**File**: `fraisier/providers/docker_compose.py` (NEW)

**Features**:

- Docker Compose file management
- Service up/down/restart
- Port mapping configuration
- Volume handling
- Environment variable substitution
- Health checks
- Log retrieval

**Key Methods**:
```python
class DockerComposeProvider(BaseProvider):
    """Deploy using Docker Compose."""

    type = "docker_compose"

    def deploy_service(self, service_name: str, version: str, config: dict) -> DeploymentResult:
        """Deploy service using docker-compose."""
        # 1. Update environment (version, config)
        # 2. docker-compose pull
        # 3. docker-compose up -d service
        # 4. Wait for service ready
        # 5. Check health

    def get_logs(self, service_name: str, lines: int = 100) -> str:
        """Get Docker Compose logs."""
        # docker-compose logs -n lines service
```

**Implementation Steps**:

1. Parse docker-compose.yaml
2. Update environment variables for version
3. Run docker-compose commands
4. Handle Docker errors
5. Track container IDs for rollback

**Tests** (12 tests):

- deploy_service (success, failure)
- compose file validation
- port mapping
- volume handling
- health checks
- logs retrieval

#### Task 2.2.2: Add Docker Compose Configuration

**File**: `fraisier/providers/docker_compose_config.py` (NEW)

**Purpose**: Helper for managing docker-compose.yaml

**Features**:

- Load/parse YAML
- Substitute environment variables
- Update service images
- Validate compose files
- Backup before changes

---

### 2.3: Coolify Provider (Days 11-15)

#### Task 2.3.1: Implement Coolify Provider

**File**: `fraisier/providers/coolify.py` (NEW)

**Features**:

- Coolify API integration
- Project and application management
- Deployment triggering
- Status polling
- Webhook support
- Log retrieval from Coolify

**Key Methods**:
```python
class CoolifyProvider(BaseProvider):
    """Deploy using Coolify platform."""

    type = "coolify"

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self.api_key = config.api_key
        self.base_url = config.url  # https://coolify.example.com
        self.client = CoolifyClient(self.base_url, self.api_key)

    def deploy_service(self, service_name: str, version: str, config: dict) -> DeploymentResult:
        """Deploy via Coolify API."""
        # 1. Find application in Coolify
        # 2. Update environment/config
        # 3. Trigger deployment
        # 4. Poll status
        # 5. Check health

    def get_service_status(self, service_name: str) -> dict[str, Any]:
        """Get status from Coolify API."""
        # GET /api/applications/{app_id}
```

**Implementation Steps**:

1. Create CoolifyClient for API communication
2. Implement OAuth/token-based auth
3. Handle polling and timeouts
4. Map Fraisier concepts to Coolify API
5. Error handling for API failures

**Tests** (12 tests):

- API authentication
- deploy_service (success, failure, timeout)
- status polling
- environment variable updates
- logs retrieval
- rollback support

#### Task 2.3.2: Create CoolifyClient

**File**: `fraisier/providers/coolify_client.py` (NEW)

**Purpose**: HTTP client for Coolify API

**Features**:

- Authenticated requests
- Error handling
- Response parsing
- Retry logic

---

### 2.4: Testing & Integration (Days 16-18)

#### Task 2.4.1: Provider Tests

**File**: `tests/test_providers.py` (NEW)

**Tests** (50+ tests total):

- BareMetalProvider: 12 tests
- DockerComposeProvider: 12 tests
- CoolifyProvider: 12 tests
- ProviderRegistry: 8 tests
- DeploymentLock: 6 tests

**Test Patterns**:
```python
@pytest.fixture
def bare_metal_provider():
    config = ProviderConfig(
        name="production",
        type="bare_metal",
        url="prod.example.com",
        custom_fields={"ssh_user": "deploy", "ssh_key_path": "/path/to/key"}
    )
    return BareMetalProvider(config)

def test_bare_metal_pre_flight_check(bare_metal_provider, mock_subprocess):
    """Test SSH connection check."""
    mock_subprocess.return_value = MagicMock(returncode=0)
    success, message = bare_metal_provider.pre_flight_check()
    assert success is True
```

#### Task 2.4.2: Integration Tests

**File**: `tests/test_provider_integration.py` (NEW)

**Tests** (15+ tests):

- Multi-provider deployment workflow
- Provider switching
- Fallback on provider failure
- Lock mechanism under concurrency
- Health check polling

#### Task 2.4.3: CLI Updates

**File**: `fraisier/cli.py` (MODIFY)

**New Commands**:

- `fraisier providers` - List available providers
- `fraisier provider-info` - Show provider configuration
- `fraisier provider-test` - Run pre-flight checks

---

## Configuration Changes

### Update fraises.yaml Format

**Before**:
```yaml
fraises:
  my_api:
    type: api
    environments:
      production:
        app_path: /var/app
        systemd_service: my_api.service
```

**After**:
```yaml
fraises:
  my_api:
    type: api
    providers:
      production:
        provider: bare_metal  # or docker_compose, coolify
        config:
          ssh_user: deploy
          ssh_key_path: /etc/fraisier/keys/prod.pem
          ssh_host: prod.example.com
          app_path: /var/app
          systemd_service: my_api.service
      staging:
        provider: docker_compose
        config:
          compose_file: docker-compose.yml
          service_name: my_api
```

---

## Database Schema Updates

### Add Provider Configuration Table

**Table**: `tb_provider_config`
```sql
CREATE TABLE tb_provider_config (
    id TEXT NOT NULL UNIQUE,                    -- Public UUID
    identifier TEXT NOT NULL UNIQUE,            -- Business key
    pk_provider_config INTEGER PRIMARY KEY,     -- Internal key

    provider_name TEXT NOT NULL,                -- bare_metal, docker_compose, coolify
    provider_type TEXT NOT NULL,                -- Type identifier
    configuration TEXT NOT NULL,                -- JSON config

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### Add Deployment Lock Table

**Table**: `tb_deployment_lock`
```sql
CREATE TABLE tb_deployment_lock (
    pk_deployment_lock INTEGER PRIMARY KEY,

    service_name TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    locked_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,

    UNIQUE(service_name, provider_name)
);
```

---

## Implementation Order

1. **Week 1 (Days 1-5)**
   - Create provider abstraction layer
   - Implement Bare Metal provider
   - Write 12 Bare Metal tests
   - Create deployment lock mechanism

2. **Week 2 (Days 6-10)**
   - Implement Docker Compose provider
   - Write 12 Docker Compose tests
   - Create docker-compose configuration helper

3. **Week 3 (Days 11-18)**
   - Implement Coolify provider
   - Write 12 Coolify tests
   - Create CoolifyClient
   - Integration tests
   - CLI updates
   - Update documentation

---

## Files to Create

### New Files

- `fraisier/providers/__init__.py` - Provider abstraction
- `fraisier/providers/bare_metal.py` - Bare Metal implementation
- `fraisier/providers/docker_compose.py` - Docker Compose implementation
- `fraisier/providers/docker_compose_config.py` - Compose helper
- `fraisier/providers/coolify.py` - Coolify implementation
- `fraisier/providers/coolify_client.py` - Coolify API client
- `fraisier/locking.py` - Deployment lock mechanism
- `tests/test_providers.py` - Provider unit tests
- `tests/test_provider_integration.py` - Integration tests

### Modified Files

- `fraisier/cli.py` - Add provider commands
- `fraisier/database.py` - Add provider config tables
- `fraisier/deployers/base.py` - Update interface
- `fraisier/config.py` - Support provider config
- `ROADMAP.md` - Update status

---

## Success Criteria per Subphase

### 2.1: Provider Abstraction & Bare Metal

- ✅ Provider interface defined
- ✅ Registry mechanism working
- ✅ Bare Metal provider complete
- ✅ Deployment locks implemented
- ✅ 24 tests passing
- ✅ SSH integration working

### 2.2: Docker Compose Provider

- ✅ Docker Compose provider complete
- ✅ Compose file parsing working
- ✅ Service management working
- ✅ 12 tests passing
- ✅ docker-compose commands working

### 2.3: Coolify Provider & Integration

- ✅ Coolify provider complete
- ✅ API client working
- ✅ Status polling working
- ✅ 12 tests passing
- ✅ All integration tests passing
- ✅ CLI commands updated
- ✅ 50+ tests total
- ✅ Documentation updated

---

## Risk Mitigation

**Risk**: SSH key management complexity
**Mitigation**: Use subprocess with ssh binary, avoid paramiko dependency

**Risk**: Docker Compose file format variations
**Mitigation**: Use yaml library, validate schema

**Risk**: Coolify API changes
**Mitigation**: Version API calls, add compatibility layer

**Risk**: Concurrent deployment conflicts
**Mitigation**: Use database-backed locks with timeout

---

## Next Phase Dependency

 enables Phase 3 (Production Hardening):

- Monitoring and alerting
- Multi-region deployments
- Blue-green deployments
- Canary deployments
- Performance optimization

---

**Created**: 2026-01-22
**Status**: Ready to implement
**Estimated Effort**: 18 days (2-3 weeks)
