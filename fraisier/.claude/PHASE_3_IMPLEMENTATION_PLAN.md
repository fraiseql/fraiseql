# Phase 3: Production Hardening Implementation Plan

**Timeline**: 2-3 weeks after Phase 2
**Target**: v1.0.0 (Stable Release)
**Status**: Starting

---

## Overview

Phase 3 transforms Fraisier from a functional tool into a production-ready deployment orchestrator with enterprise-grade reliability, observability, and error handling.

**Key Deliverables**:

- ✅ Comprehensive error handling with custom exception hierarchy
- ✅ Structured logging with JSON format
- ✅ Prometheus metrics and monitoring
- ✅ Distributed tracing support
- ✅ Deployment dashboards and observability
- ✅ Advanced recovery strategies
- ✅ Multi-database support
- ✅ Enhanced documentation

---

## Tasks Overview

### 3.1: Error Handling & Recovery (Days 1-3)

**Goal**: Implement comprehensive error handling with graceful failure modes

#### 3.1.1: Custom Exception Hierarchy

**File**: `fraisier/errors.py` (NEW)

```python
class FraisierError(Exception):
    """Base exception for all Fraisier errors."""

    def __init__(self, message: str, code: str | None = None,
                 context: dict | None = None, recoverable: bool = False):
        self.message = message
        self.code = code
        self.context = context or {}
        self.recoverable = recoverable
        super().__init__(message)


class ConfigurationError(FraisierError):
    """Configuration loading or validation errors."""
    pass


class DeploymentError(FraisierError):
    """Deployment execution errors."""
    pass


class DeploymentTimeoutError(DeploymentError):
    """Deployment timed out."""
    pass


class HealthCheckError(DeploymentError):
    """Health check failed."""
    pass


class ProviderError(FraisierError):
    """Provider-related errors."""
    pass


class ProviderUnavailableError(ProviderError):
    """Provider is temporarily unavailable."""
    recoverable = True


class RollbackError(DeploymentError):
    """Rollback failed."""
    pass


class DatabaseError(FraisierError):
    """Database operation errors."""
    pass
```

**Tests**: 15 tests
- Exception hierarchy and attributes
- Error serialization
- Context preservation
- Recoverable flag handling

#### 3.1.2: Recovery Strategies

**File**: `fraisier/recovery.py` (NEW)

```python
class RecoveryStrategy:
    """Base recovery strategy for errors."""

    def can_recover(self, error: FraisierError) -> bool:
        """Check if error can be recovered from."""
        pass

    def execute_recovery(self, context: dict) -> bool:
        """Execute recovery and return success."""
        pass


class RetryStrategy(RecoveryStrategy):
    """Retry failed operations with exponential backoff."""

    def __init__(self, max_attempts: int = 3, backoff_factor: float = 2.0):
        self.max_attempts = max_attempts
        self.backoff_factor = backoff_factor


class FallbackStrategy(RecoveryStrategy):
    """Fallback to alternative provider."""

    def __init__(self, fallback_provider: str):
        self.fallback_provider = fallback_provider


class RollbackRecoveryStrategy(RecoveryStrategy):
    """Automatic rollback on deployment failure."""

    def __init__(self, rollback_on_timeout: bool = True,
                 rollback_on_health_check_failure: bool = True):
        self.rollback_on_timeout = rollback_on_timeout
        self.rollback_on_health_check_failure = rollback_on_health_check_failure
```

**Tests**: 12 tests
- Retry with backoff
- Fallback provider selection
- Rollback conditions
- Recovery decision logic

#### 3.1.3: Centralized Error Handler

**File**: `fraisier/error_handler.py` (NEW)

```python
class ErrorHandler:
    """Centralized error handling with recovery strategies."""

    def __init__(self, logger, metrics):
        self.logger = logger
        self.metrics = metrics
        self.recovery_strategies: dict[str, RecoveryStrategy] = {}

    def register_strategy(self, error_type: str, strategy: RecoveryStrategy):
        """Register recovery strategy for error type."""
        pass

    def handle_error(self, error: Exception, context: dict) -> bool:
        """Handle error with recovery attempt."""
        pass

    def should_retry(self, error: Exception, attempt: int) -> bool:
        """Determine if operation should be retried."""
        pass
```

**Tests**: 10 tests
- Strategy registration and lookup
- Error handling flow
- Retry decision logic
- Metrics recording

---

### 3.2: Logging & Observability (Days 4-6)

**Goal**: Implement structured logging and observability

#### 3.2.1: Structured Logging

**File**: `fraisier/logging.py` (NEW)

```python
import logging
import json
from datetime import datetime

class JSONFormatter(logging.Formatter):
    """Format logs as JSON for structured logging."""

    def format(self, record: logging.LogRecord) -> str:
        log_obj = {
            'timestamp': datetime.utcnow().isoformat(),
            'level': record.levelname,
            'logger': record.name,
            'message': record.getMessage(),
            'context': getattr(record, 'context', {}),
        }

        if record.exc_info:
            log_obj['exception'] = self.formatException(record.exc_info)

        return json.dumps(log_obj)


class ContextualLogger:
    """Logger with built-in context tracking."""

    def __init__(self, name: str):
        self.logger = logging.getLogger(name)
        self._context = {}

    def with_context(self, **kwargs) -> 'ContextualLogger':
        """Add context for next log message."""
        ctx = ContextualLogger(self.logger.name)
        ctx._context = {**self._context, **kwargs}
        return ctx

    def info(self, message: str, **kwargs):
        """Log info with context."""
        extra = {'context': {**self._context, **kwargs}}
        self.logger.info(message, extra=extra)
```

**Tests**: 10 tests
- JSON formatting
- Context tracking
- Exception formatting
- Log levels

#### 3.2.2: Prometheus Metrics

**File**: `fraisier/metrics.py` (NEW)

```python
from prometheus_client import Counter, Histogram, Gauge

class DeploymentMetrics:
    """Prometheus metrics for deployments."""

    # Counters
    deployments_total = Counter(
        'fraisier_deployments_total',
        'Total deployments attempted',
        ['provider', 'status', 'fraise_type']
    )

    deployment_errors_total = Counter(
        'fraisier_deployment_errors_total',
        'Total deployment errors',
        ['provider', 'error_type']
    )

    rollbacks_total = Counter(
        'fraisier_rollbacks_total',
        'Total rollbacks performed',
        ['provider', 'reason']
    )

    # Histograms
    deployment_duration_seconds = Histogram(
        'fraisier_deployment_duration_seconds',
        'Deployment duration in seconds',
        ['provider', 'status'],
        buckets=[5, 10, 30, 60, 120, 300]
    )

    health_check_duration_seconds = Histogram(
        'fraisier_health_check_duration_seconds',
        'Health check duration in seconds',
        ['provider', 'check_type'],
        buckets=[1, 2, 5, 10]
    )

    # Gauges
    active_deployments = Gauge(
        'fraisier_active_deployments',
        'Currently active deployments',
        ['provider']
    )
```

**Tests**: 12 tests
- Counter increments
- Histogram recordings
- Gauge updates
- Metrics filtering

#### 3.2.3: Audit Logging

**File**: `fraisier/audit.py` (NEW)

```python
class AuditLogger:
    """Log all significant actions for compliance and debugging."""

    def __init__(self, logger):
        self.logger = logger

    def log_deployment_start(self, deployment_id: str, fraise: str,
                            environment: str, provider: str):
        """Log deployment start."""
        self.logger.info(
            'Deployment started',
            deployment_id=deployment_id,
            fraise=fraise,
            environment=environment,
            provider=provider,
            event_type='deployment_start'
        )

    def log_deployment_complete(self, deployment_id: str, status: str,
                               duration: float, error: str | None = None):
        """Log deployment completion."""
        pass

    def log_configuration_change(self, config_type: str, old_value: dict,
                                new_value: dict, changed_by: str | None = None):
        """Log configuration changes."""
        pass
```

**Tests**: 8 tests
- Event logging
- Sensitive data redaction
- Audit trail completeness

---

### 3.3: Deployment Dashboards & UI (Days 7-9)

**Goal**: Create observability dashboards

#### 3.3.1: Prometheus Exporter Endpoint

**File**: Update `fraisier/cli.py` with health/metrics endpoint

```python
@main.command(name="metrics")
def metrics_endpoint():
    """Start Prometheus metrics exporter (for use in monitoring)."""
    from prometheus_client import start_http_server

    # Start metrics server on port 8001
    start_http_server(8001)
    console.print("[green]Prometheus metrics available at http://localhost:8001/metrics[/green]")
```

#### 3.3.2: Grafana Dashboard Template

**File**: `monitoring/grafana-dashboard.json` (NEW)

Dashboard showing:

- Deployment success rate
- Deployment duration trends
- Error rates by provider
- Active deployments
- Health check performance
- Rollback frequency

**Tests**: 5 tests
- Metrics endpoint available
- Dashboard JSON valid
- All metrics queryable

---

### 3.4: Advanced Features (Days 10-12)

#### 3.4.1: Health Check Enhancements

**File**: Update providers

```python
class HealthCheckManager:
    """Manage health checks with retries and monitoring."""

    def check_with_retries(self, provider, service: str,
                          max_retries: int = 3,
                          initial_delay: float = 1.0) -> bool:
        """Check health with exponential backoff retries."""
        pass

    def check_and_monitor(self, provider, service: str,
                         metrics) -> bool:
        """Check health and record metrics."""
        pass
```

**Tests**: 8 tests
- Retry logic
- Metrics recording
- Timeout handling

#### 3.4.2: Multi-Database Support

**File**: `fraisier/db/base.py` (abstract)

```python
class DatabaseDriver:
    """Abstract database driver."""

    @abstractmethod
    def connect(self): pass

    @abstractmethod
    def create_tables(self): pass

    @abstractmethod
    def record_deployment(self, data: dict): pass


# Implementation files:
# fraisier/db/postgres.py - PostgreSQL (existing, enhanced)
# fraisier/db/mysql.py - MySQL (NEW)
# fraisier/db/sqlite.py - SQLite (NEW)
```

**Tests**: 15 tests
- Connection pool management
- Migration compatibility
- Query result consistency across databases

---

### 3.5: Documentation & Examples (Days 13-15)

#### 3.5.1: Operator Guide

**File**: `docs/OPERATOR_GUIDE.md` (NEW)

Topics:

- Monitoring and alerting setup
- Error recovery procedures
- Database migration guide
- Performance tuning
- Troubleshooting common issues

#### 3.5.2: Deployment Patterns

**File**: `docs/DEPLOYMENT_PATTERNS.md` (NEW)

Patterns:

- Rolling deployments
- Canary deployments (setup for Phase 4)
- Blue-green deployments (setup for Phase 4)
- Automatic rollback on health check failure

---

## Test Strategy

### Unit Tests (80+ tests)

- Error handling (25 tests)
- Recovery strategies (12 tests)
- Logging (10 tests)
- Metrics (12 tests)
- Health checks (8 tests)
- Multi-database (15 tests)

### Integration Tests (20+ tests)

- End-to-end error handling
- Recovery workflow
- Logging in real scenarios
- Metrics recording in workflows
- Database operations

### Total: 100+ tests passing

---

## Success Criteria

### 3.1: Error Handling

- ✅ Custom exception hierarchy implemented
- ✅ Recovery strategies working
- ✅ Graceful failure modes tested
- ✅ 20+ tests passing

### 3.2: Logging & Observability

- ✅ Structured JSON logging
- ✅ Prometheus metrics exported
- ✅ Audit logging working
- ✅ 30+ tests passing

### 3.3: Dashboards

- ✅ Prometheus exporter available
- ✅ Grafana dashboard templates provided
- ✅ All metrics queryable
- ✅ 5+ tests passing

### 3.4: Advanced Features

- ✅ Health check retries working
- ✅ Multi-database support (MySQL, SQLite)
- ✅ 23+ tests passing

### 3.5: Documentation

- ✅ Operator guide complete
- ✅ Deployment patterns documented
- ✅ Examples provided

### Phase 3 Complete

- ✅ 100+ tests passing
- ✅ ruff linting passes
- ✅ All major error paths tested
- ✅ Production-ready monitoring
- ✅ v1.0.0 ready for release

---

## Implementation Order

**Week 1 (Days 1-3): Error Handling**
1. Create custom exception hierarchy
2. Implement recovery strategies
3. Build error handler

**Week 1 (Days 4-6): Observability**
1. Implement structured logging
2. Add Prometheus metrics
3. Create audit logging

**Week 2 (Days 7-9): Dashboards**
1. Prometheus exporter endpoint
2. Grafana dashboard templates
3. Integration testing

**Week 2 (Days 10-12): Advanced Features**
1. Health check enhancements
2. Multi-database support
3. Database migration utilities

**Week 3 (Days 13-15): Documentation**
1. Operator guide
2. Deployment patterns
3. Examples and troubleshooting

---

## Files to Create

### New Files

- `fraisier/errors.py` - Custom exception hierarchy
- `fraisier/recovery.py` - Recovery strategies
- `fraisier/error_handler.py` - Centralized error handling
- `fraisier/logging.py` - Structured logging
- `fraisier/metrics.py` - Prometheus metrics
- `fraisier/audit.py` - Audit logging
- `fraisier/db/base.py` - Database abstraction
- `fraisier/db/mysql.py` - MySQL driver
- `fraisier/db/sqlite.py` - SQLite driver
- `monitoring/grafana-dashboard.json` - Dashboard template
- `docs/OPERATOR_GUIDE.md` - Operations documentation
- `docs/DEPLOYMENT_PATTERNS.md` - Deployment patterns
- Tests: `tests/test_errors.py`, `tests/test_recovery.py`, `tests/test_logging.py`,
  `tests/test_metrics.py`, `tests/test_databases.py`, etc.

### Modified Files

- `fraisier/cli.py` - Add metrics endpoint
- `fraisier/providers/base.py` - Add logging and metrics
- `fraisier/database.py` - Add multi-database support
- `fraisier/deployers/base.py` - Add error handling
- Various provider implementations - Add observability

---

## Risk Mitigation

**Risk**: Metrics overhead affecting performance
**Mitigation**: Use asynchronous metrics recording, batch updates

**Risk**: Logging volume overwhelming storage
**Mitigation**: Log rotation, configurable log levels, filtering rules

**Risk**: Database compatibility issues
**Mitigation**: Abstract database layer, comprehensive testing per database

**Risk**: Error recovery causing cascading failures
**Mitigation**: Circuit breaker pattern, max retry limits, timeout safeguards

---

## Next Phase Dependency

Phase 3 enables Phase 4 (Multi-Language & Cloud):

- Stable, monitored foundation
- Production error handling
- Multi-language implementations will use same monitoring
- Cloud platform can rely on observability

---

## Related Documents

- Roadmap: `ROADMAP.md`
- Phase 2: `.claude/PHASE_2_IMPLEMENTATION_PLAN.md`
- Architecture: `docs/PRD.md`
