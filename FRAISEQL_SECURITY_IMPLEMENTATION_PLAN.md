# FraiseQL Security Implementation Plan

**For Agentic AI** | **TDD Methodology** | **Single Release: v2.0**

---

## Overview

Step-by-step implementation instructions for FraiseQL's security roadmap. All work is done on a single feature branch and released together as **v2.0**.

Each task follows the **RED-GREEN-REFACTOR-QA** TDD cycle.

### TDD Cycle Reference

```
RED     → Write a failing test that defines expected behavior
GREEN   → Write minimal code to make the test pass
REFACTOR → Clean up code while keeping tests green
QA      → Run full test suite, linting, type checking
```

### Prerequisites

- Python 3.11+
- `uv` package manager installed
- Access to FraiseQL repository
- Familiarity with pytest, asyncio, and DDD patterns

### Running Tests

```bash
# Run specific test
uv run pytest path/to/test.py::TestClass::test_method -v

# Run all tests
uv run pytest

# Linting and type checking
uv run ruff check src/
uv run mypy src/
```

### Branch Strategy

All phases are developed on a **single feature branch**:

```bash
git checkout dev
git pull origin dev
git checkout -b feature/v2.0-security-hardening
```

---

## Phase 1: Supply Chain Trust & Framework Integrity

**Theme:** "Visibility & Integrity"

### Goals

1. Merge existing SBOM module from security branch
2. Clean up documentation (remove US-specific language)
3. Implement request body size limits (new feature)
4. Verify CI/CD workflows for artifact signing

---

### Task 1.1: Merge SBOM Module from Security Branch

**Complexity:** Low (code exists, needs merging)

#### Pre-Task Verification

```bash
# Verify you're on the feature branch
git branch --show-current  # Should show: feature/v2.0-security-hardening

# Verify the security branch exists
git branch -r | grep security-hardening  # Should show the remote branch
```

#### Steps

> **Note:** We use merge instead of cherry-pick to preserve commit history and
> reduce risk of conflicts when the security branch has multiple interconnected commits.
> Merge creates a clear audit trail and handles dependency chains automatically.

```bash
# Option 1 (Recommended): Merge the entire security branch
git merge origin/security-hardening --no-ff -m "Merge security-hardening branch for v2.0"

# Option 2 (If only specific commits needed): Cherry-pick with -x flag
# The -x flag records the source commit hash in the commit message
git cherry-pick -x 2431d2e  # "feat: Implement SBOM generation for federal compliance"

# If conflicts occur, resolve them manually
git status
# Edit conflicting files, then:
git add .
git merge --continue  # or: git cherry-pick --continue
```

#### TDD Cycle: Verify SBOM Module

##### RED Phase

Run existing tests to verify they work after cherry-pick:

```bash
uv run pytest tests/unit/sbom/ -v
```

**Expected:** Tests should pass if cherry-pick was clean. If imports fail, proceed to GREEN.

##### GREEN Phase

If tests fail due to import errors, fix paths in `tests/unit/sbom/test_domain_models.py`:

```python
from fraiseql.sbom.domain.models import (
    SBOMComponent,
    SBOMMetadata,
    ComponentType,
    License,
)
```

##### REFACTOR Phase

Verify the public API exports in `src/fraiseql/sbom/__init__.py`:

```python
from fraiseql.sbom.application.sbom_generator import SBOMGenerator
from fraiseql.sbom.domain.models import (
    SBOMComponent,
    SBOMMetadata,
    ComponentType,
    License,
)
from fraiseql.sbom.infrastructure.cyclonedx_adapter import CycloneDXAdapter

__all__ = [
    "SBOMGenerator",
    "SBOMComponent",
    "SBOMMetadata",
    "ComponentType",
    "License",
    "CycloneDXAdapter",
]
```

##### QA Phase

```bash
uv run pytest tests/unit/sbom/ -v
uv run ruff check src/fraiseql/sbom/
uv run mypy src/fraiseql/sbom/
```

**Acceptance Criteria:**
- [ ] All SBOM tests pass
- [ ] No ruff errors
- [ ] No mypy errors
- [ ] `from fraiseql.sbom import SBOMGenerator` works

---

### Task 1.2: Add SBOM Dependencies to pyproject.toml

**Complexity:** Low

#### Pre-Task Verification

```bash
# Check current optional dependencies
grep -A 5 "optional-dependencies" pyproject.toml
```

#### TDD Cycle

##### RED Phase

```bash
# This should fail if dependencies aren't installed
uv run python -c "from cyclonedx.model import Component"
```

##### GREEN Phase

Edit `pyproject.toml` to add under `[project.optional-dependencies]`:

```toml
sbom = [
    "cyclonedx-python-lib>=7.0.0,<8.0",
    "packageurl-python>=0.15.0",
]
```

Also add to the `all` extra:
```toml
all = [
    # ... existing deps ...
    "cyclonedx-python-lib>=7.0.0,<8.0",
    "packageurl-python>=0.15.0",
]
```

##### REFACTOR Phase

```bash
uv sync --all-extras
```

##### QA Phase

```bash
uv run python -c "from cyclonedx.model import Component; print('OK')"
uv run pytest tests/unit/sbom/ -v
```

**Acceptance Criteria:**
- [ ] `uv sync --all-extras` succeeds
- [ ] SBOM imports work

---

### Task 1.3: Implement Request Body Size Limits

**Complexity:** Medium (new feature)

#### Context

This protects against DoS attacks where attackers send extremely large GraphQL queries. We'll add a middleware that rejects requests exceeding a configurable size limit.

#### Pre-Task Verification

```bash
# Verify middleware directory exists
ls src/fraiseql/middleware/
# Should show: __init__.py, rate_limiter.py, apq.py, apq_caching.py
```

#### File Structure

```
src/fraiseql/middleware/
├── __init__.py          # Update exports
├── rate_limiter.py      # Existing
├── apq.py               # Existing
└── body_size_limiter.py # NEW
```

#### TDD Cycle

##### RED Phase

Create the test file `tests/unit/middleware/test_body_size_limiter.py`:

```python
"""Tests for request body size limiting middleware."""

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from fraiseql.middleware.body_size_limiter import (
    BodySizeLimiterMiddleware,
    BodySizeConfig,
    RequestTooLargeError,
)


class TestBodySizeConfig:
    """Tests for BodySizeConfig."""

    def test_default_max_size_is_1mb(self):
        """Default max body size should be 1MB."""
        config = BodySizeConfig()
        assert config.max_body_size == 1_048_576  # 1MB in bytes

    def test_custom_max_size(self):
        """Custom max body size should be respected."""
        config = BodySizeConfig(max_body_size=500_000)
        assert config.max_body_size == 500_000

    def test_exempt_paths_default_empty(self):
        """Exempt paths should default to empty list."""
        config = BodySizeConfig()
        assert config.exempt_paths == []

    def test_human_readable_size_mb(self):
        """Should provide human-readable size string for MB."""
        config = BodySizeConfig(max_body_size=1_048_576)
        assert config.human_readable_size == "1.0 MB"

    def test_human_readable_size_kb(self):
        """Should provide human-readable size string for KB."""
        config = BodySizeConfig(max_body_size=512_000)
        # 512000 / 1024 = 500.0
        assert config.human_readable_size == "500.0 KB"


class TestBodySizeLimiterMiddleware:
    """Tests for BodySizeLimiterMiddleware."""

    @pytest.fixture
    def app_with_middleware(self):
        """Create FastAPI app with body size limiter."""
        app = FastAPI()
        config = BodySizeConfig(max_body_size=1000)  # 1KB for testing
        app.add_middleware(BodySizeLimiterMiddleware, config=config)

        @app.post("/graphql")
        async def graphql_endpoint(request_body: dict):
            return {"status": "ok"}

        @app.get("/health")
        async def health():
            return {"status": "healthy"}

        return app

    @pytest.fixture
    def client(self, app_with_middleware):
        """Create test client."""
        return TestClient(app_with_middleware)

    def test_allows_small_request(self, client):
        """Requests under limit should succeed."""
        response = client.post(
            "/graphql",
            json={"query": "{ users { id } }"},
        )
        assert response.status_code == 200

    def test_rejects_large_request(self, client):
        """Requests over limit should be rejected with 413."""
        large_query = "x" * 2000  # 2KB, over 1KB limit
        response = client.post(
            "/graphql",
            json={"query": large_query},
        )
        assert response.status_code == 413
        assert "Request body too large" in response.json()["detail"]

    def test_get_requests_not_limited(self, client):
        """GET requests should not be size-limited."""
        response = client.get("/health")
        assert response.status_code == 200

    def test_exempt_paths_not_limited(self):
        """Exempt paths should bypass size limit."""
        app = FastAPI()
        config = BodySizeConfig(
            max_body_size=100,
            exempt_paths=["/upload"],
        )
        app.add_middleware(BodySizeLimiterMiddleware, config=config)

        @app.post("/upload")
        async def upload():
            return {"status": "ok"}

        client = TestClient(app)
        response = client.post("/upload", content=b"x" * 200)
        assert response.status_code == 200

    def test_content_length_header_checked_first(self, client):
        """Should reject based on Content-Length header before reading body."""
        response = client.post(
            "/graphql",
            content=b"x" * 100,
            headers={"Content-Length": "999999"},
        )
        assert response.status_code == 413

    def test_rejects_chunked_transfer_over_limit(self):
        """Should reject chunked requests that exceed limit (no Content-Length)."""
        app = FastAPI()
        config = BodySizeConfig(max_body_size=100)
        app.add_middleware(BodySizeLimiterMiddleware, config=config)

        @app.post("/graphql")
        async def graphql_endpoint():
            return {"status": "ok"}

        client = TestClient(app)
        # Chunked transfer - no Content-Length header
        response = client.post(
            "/graphql",
            content=b"x" * 200,  # Over 100 byte limit
            headers={"Transfer-Encoding": "chunked"},
        )
        assert response.status_code == 413

    def test_streaming_body_cutoff(self):
        """Should stop reading body once limit exceeded (DoS protection)."""
        app = FastAPI()
        config = BodySizeConfig(max_body_size=1000)
        app.add_middleware(BodySizeLimiterMiddleware, config=config)

        @app.post("/graphql")
        async def graphql_endpoint():
            return {"status": "ok"}

        client = TestClient(app)
        # Send much larger body - should be cut off early
        response = client.post("/graphql", content=b"x" * 10000)
        assert response.status_code == 413


class TestRequestTooLargeError:
    """Tests for RequestTooLargeError exception."""

    def test_error_message_includes_limit(self):
        """Error message should include the limit."""
        error = RequestTooLargeError(
            max_size=1_000_000,
            actual_size=2_000_000,
        )
        assert "1.0 MB" in str(error) or "1000000" in str(error)

    def test_error_has_status_code_413(self):
        """Error should have HTTP 413 status code."""
        error = RequestTooLargeError(max_size=1000, actual_size=2000)
        assert error.status_code == 413
```

Run the test to see it fail:

```bash
uv run pytest tests/unit/middleware/test_body_size_limiter.py -v
```

**Expected:** `ModuleNotFoundError: No module named 'fraiseql.middleware.body_size_limiter'`

##### GREEN Phase

Create `src/fraiseql/middleware/body_size_limiter.py`:

```python
"""Request body size limiting middleware.

Protects against DoS attacks by rejecting requests that exceed
a configurable body size limit.
"""

from dataclasses import dataclass, field
from typing import Any, Callable

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse


class RequestTooLargeError(Exception):
    """Raised when request body exceeds size limit."""

    status_code: int = 413

    def __init__(self, max_size: int, actual_size: int | None = None) -> None:
        self.max_size = max_size
        self.actual_size = actual_size
        self.status_code = 413
        super().__init__(self._format_message())

    def _format_message(self) -> str:
        max_human = _format_bytes(self.max_size)
        if self.actual_size:
            actual_human = _format_bytes(self.actual_size)
            return f"Request body too large: {actual_human} exceeds limit of {max_human}"
        return f"Request body too large: exceeds limit of {max_human}"


def _format_bytes(size: int) -> str:
    """Format bytes as human-readable string."""
    if size >= 1_048_576:
        return f"{size / 1_048_576:.1f} MB"
    elif size >= 1024:
        return f"{size / 1024:.1f} KB"
    return f"{size} bytes"


@dataclass
class BodySizeConfig:
    """Configuration for body size limiter.

    Attributes:
        max_body_size: Maximum allowed body size in bytes. Default 1MB.
        exempt_paths: List of paths that bypass the size limit.
        exempt_methods: HTTP methods that bypass the limit.
    """

    max_body_size: int = 1_048_576  # 1MB default
    exempt_paths: list[str] = field(default_factory=list)
    exempt_methods: set[str] = field(
        default_factory=lambda: {"GET", "HEAD", "OPTIONS"}
    )

    @property
    def human_readable_size(self) -> str:
        """Return max size as human-readable string."""
        return _format_bytes(self.max_body_size)


class SizeLimitedBody:
    """Wrapper that enforces body size limits while streaming.

    SECURITY: This prevents bypass attacks via chunked transfer encoding
    or missing Content-Length headers. The body is measured as it streams.
    """

    def __init__(self, body: Any, max_size: int) -> None:
        self._body = body
        self._max_size = max_size
        self._bytes_read = 0

    async def __aiter__(self):
        async for chunk in self._body:
            self._bytes_read += len(chunk)
            if self._bytes_read > self._max_size:
                raise RequestTooLargeError(
                    max_size=self._max_size,
                    actual_size=self._bytes_read,
                )
            yield chunk


class BodySizeLimiterMiddleware(BaseHTTPMiddleware):
    """Middleware that limits request body size.

    Rejects requests that exceed the configured maximum body size
    with HTTP 413 Payload Too Large.

    SECURITY: Enforces limits in two ways:
    1. Fast path: Check Content-Length header if present
    2. Safe path: Stream body and measure actual bytes (catches chunked/missing header)

    Usage:
        app = FastAPI()
        config = BodySizeConfig(max_body_size=5_000_000)  # 5MB
        app.add_middleware(BodySizeLimiterMiddleware, config=config)
    """

    def __init__(
        self,
        app: Callable,
        config: BodySizeConfig | None = None,
    ) -> None:
        super().__init__(app)
        self.config = config or BodySizeConfig()

    async def dispatch(
        self,
        request: Request,
        call_next: Callable[[Request], Response],
    ) -> Response:
        """Process request and check body size."""
        # Skip exempt methods
        if request.method in self.config.exempt_methods:
            return await call_next(request)

        # Skip exempt paths
        if request.url.path in self.config.exempt_paths:
            return await call_next(request)

        # Fast path: Check Content-Length header first (efficient)
        content_length = request.headers.get("content-length")
        if content_length:
            try:
                size = int(content_length)
                if size > self.config.max_body_size:
                    return self._create_error_response(size)
            except ValueError:
                pass

        # Safe path: Read and measure actual body (catches chunked transfer, missing headers)
        # This is CRITICAL for security - Content-Length can be omitted or spoofed
        try:
            body = await self._read_body_with_limit(request)
        except RequestTooLargeError as e:
            return self._create_error_response(e.actual_size)

        # Reconstruct request with the body we already read
        # This avoids double-reading the stream
        async def receive():
            return {"type": "http.request", "body": body}

        request._receive = receive

        return await call_next(request)

    async def _read_body_with_limit(self, request: Request) -> bytes:
        """Read request body while enforcing size limit.

        Streams the body and stops early if limit is exceeded,
        preventing memory exhaustion from malicious large requests.
        """
        chunks: list[bytes] = []
        total_size = 0

        async for chunk in request.stream():
            total_size += len(chunk)
            if total_size > self.config.max_body_size:
                raise RequestTooLargeError(
                    max_size=self.config.max_body_size,
                    actual_size=total_size,
                )
            chunks.append(chunk)

        return b"".join(chunks)

    def _create_error_response(self, actual_size: int | None = None) -> JSONResponse:
        """Create 413 error response."""
        error = RequestTooLargeError(
            max_size=self.config.max_body_size,
            actual_size=actual_size,
        )
        return JSONResponse(
            status_code=413,
            content={
                "detail": str(error),
                "max_size": self.config.max_body_size,
                "max_size_human": self.config.human_readable_size,
            },
        )
```

Run tests:

```bash
uv run pytest tests/unit/middleware/test_body_size_limiter.py -v
```

**Expected:** All tests pass

##### REFACTOR Phase

Update `src/fraiseql/middleware/__init__.py` to export the new middleware:

```python
"""Middleware components for FraiseQL."""

from .apq import (
    create_apq_error_response,
    get_apq_hash,
    handle_apq_request,
    is_apq_request,
    is_apq_with_query_request,
)
from .body_size_limiter import (
    BodySizeConfig,
    BodySizeLimiterMiddleware,
    RequestTooLargeError,
)
from .rate_limiter import (
    InMemoryRateLimiter,
    PostgreSQLRateLimiter,
    RateLimitConfig,
    RateLimiterMiddleware,
    RateLimitExceeded,
    RateLimitInfo,
    SlidingWindowRateLimiter,
)

__all__ = [
    # Body size limiter
    "BodySizeConfig",
    "BodySizeLimiterMiddleware",
    "RequestTooLargeError",
    # Rate limiter
    "InMemoryRateLimiter",
    "PostgreSQLRateLimiter",
    "RateLimitConfig",
    "RateLimitExceeded",
    "RateLimitInfo",
    "RateLimiterMiddleware",
    "SlidingWindowRateLimiter",
    # APQ middleware
    "create_apq_error_response",
    "get_apq_hash",
    "handle_apq_request",
    "is_apq_request",
    "is_apq_with_query_request",
]
```

##### QA Phase

```bash
uv run pytest tests/unit/middleware/ -v
uv run pytest --tb=short
uv run ruff check src/fraiseql/middleware/
uv run mypy src/fraiseql/middleware/
```

**Acceptance Criteria:**
- [ ] All middleware tests pass
- [ ] No ruff errors
- [ ] No mypy errors
- [ ] `from fraiseql.middleware import BodySizeLimiterMiddleware` works

---

### Task 1.4: Clean Up Documentation

**Complexity:** Low (text editing)

#### Context

Remove US-specific language from documentation. The SBOM module was originally created for US federal compliance but should be marketed globally.

#### Pre-Task Verification

```bash
# Check if COMPLIANCE directory was cherry-picked
ls COMPLIANCE/
# Should show: EO_14028/ and other directories
```

#### Steps

```bash
# Rename directory
git mv COMPLIANCE/EO_14028 COMPLIANCE/SUPPLY_CHAIN
```

#### File Edits Required

Edit files to replace language:

| Find | Replace With |
|------|--------------|
| `Executive Order 14028` | `industry supply chain security standards` |
| `federal compliance` | `regulatory compliance` |
| `federal procurement requirements` | `enterprise procurement requirements` |
| `U.S. government supply chain mandates` | `industry supply chain security requirements` |
| `Pentagon` | (remove entirely) |
| `DoD` | (remove entirely) |

#### Checklist

Before committing, verify:
- [ ] No "Executive Order" references
- [ ] No "Pentagon" or "DoD" language
- [ ] No NIST 800-53 control mappings
- [ ] No FedRAMP/FISMA/CMMC references
- [ ] Uses "regulated industries" instead of "government"
- [ ] References global standards (ISO, SOC 2, CycloneDX, SLSA)

---

### Task 1.5: Verify CI/CD Workflows

**Complexity:** Low (verification only)

#### Context

The security branch has updated `.github/workflows/publish.yml` with Sigstore signing and SLSA provenance. Verify these were cherry-picked correctly.

#### Steps

```bash
# Check for cosign signing
grep -A 5 "cosign" .github/workflows/publish.yml

# Check for SLSA provenance
grep -A 5 "slsa" .github/workflows/publish.yml
```

#### Verification Checklist

- [ ] `sigstore/cosign-installer@v3` action present
- [ ] `slsa-framework/slsa-github-generator` action present
- [ ] Attestation upload step exists

---

### Phase 1 Checkpoint

Run full validation before proceeding to Phase 2:

```bash
uv run pytest --tb=short
uv run ruff check src/
uv run mypy src/
```

**Phase 1 Completion Checklist:**
- [ ] SBOM module merged and working
- [ ] SBOM dependencies added
- [ ] Body size limiter implemented and tested
- [ ] Documentation cleaned up
- [ ] CI/CD workflows verified

---

## Phase 2: Secrets Management with KMS Providers

**Theme:** "Core Security Infrastructure"

### Goals

1. Implement KMS domain layer (models, exceptions, abstract base class)
2. Implement HashiCorp Vault provider
3. Implement AWS KMS provider
4. Implement GCP Cloud KMS provider
5. Add KeyManager application service

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      KeyManager                              │
│  (Application Service - unified interface for encryption)   │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                   BaseKMSProvider (ABC)                      │
│  - Common error handling, logging, context building          │
│  - Template methods: encrypt(), decrypt(), generate_data_key │
│  - Abstract methods: _do_encrypt(), _do_decrypt(), etc.      │
└─────────────────────────┬───────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
┌─────────────┐   ┌─────────────┐   ┌─────────────┐
│ VaultKMS    │   │ AWSKMS      │   │ GCPKMS      │
│ Provider    │   │ Provider    │   │ Provider    │
│             │   │             │   │             │
│ _do_encrypt │   │ _do_encrypt │   │ _do_encrypt │
│ _do_decrypt │   │ _do_decrypt │   │ _do_decrypt │
│ etc.        │   │ etc.        │   │ etc.        │
└─────────────┘   └─────────────┘   └─────────────┘
```

**Why ABC instead of just Protocol?**
- **Shared logic**: Error handling, logging, context normalization
- **Template method pattern**: Common workflow with provider-specific hooks
- **DRY**: Avoids duplicating ~50 lines of boilerplate in each provider
- **Easier testing**: Test base class behavior once

---

### Task 2.1: Create KMS Domain Models

**Complexity:** Medium

#### Pre-Task Verification

```bash
# Verify security directory exists (or create it)
ls src/fraiseql/security/ || mkdir -p src/fraiseql/security/kms/domain
```

#### File Structure

```
src/fraiseql/security/kms/
├── __init__.py
├── domain/
│   ├── __init__.py
│   ├── models.py           # Value objects
│   ├── base.py             # BaseKMSProvider ABC
│   └── exceptions.py       # Domain exceptions
├── application/
│   ├── __init__.py
│   └── key_manager.py      # Application service
└── infrastructure/
    ├── __init__.py
    ├── vault.py            # HashiCorp Vault
    ├── aws_kms.py          # AWS KMS
    ├── gcp_kms.py          # GCP Cloud KMS
    └── local.py            # Local development (non-production)
```

#### TDD Cycle: Domain Models

##### RED Phase

Create `tests/unit/security/kms/test_domain_models.py`:

```python
"""Tests for KMS domain models."""

import pytest
from datetime import datetime, UTC

from fraiseql.security.kms.domain.models import (
    KeyPurpose,
    KeyState,
    KeyReference,
    EncryptedData,
    DataKeyPair,
    RotationPolicy,
)


class TestKeyReference:
    """Tests for KeyReference value object."""

    def test_is_immutable(self):
        """KeyReference should be immutable (frozen dataclass)."""
        ref = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias="alias/my-key",
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=datetime.now(UTC),
        )
        with pytest.raises((AttributeError, TypeError)):
            ref.key_id = "other-key"

    def test_qualified_id(self):
        """Should generate qualified ID as provider:key_id."""
        ref = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=datetime.now(UTC),
        )
        assert ref.qualified_id == "vault:my-key"

    def test_equality(self):
        """Two references with same values should be equal."""
        now = datetime.now(UTC)
        ref1 = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        ref2 = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        assert ref1 == ref2


class TestEncryptedData:
    """Tests for EncryptedData value object."""

    def test_to_dict_serialization(self):
        """Should serialize to dictionary correctly."""
        now = datetime.now(UTC)
        key_ref = KeyReference(
            provider="vault",
            key_id="my-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        encrypted = EncryptedData(
            ciphertext=b"encrypted-data",
            key_reference=key_ref,
            algorithm="aes256-gcm96",
            encrypted_at=now,
            context={"purpose": "test"},
        )

        result = encrypted.to_dict()

        assert result["ciphertext"] == "656e637279707465642d64617461"  # hex
        assert result["key_id"] == "vault:my-key"
        assert result["algorithm"] == "aes256-gcm96"
        assert result["context"] == {"purpose": "test"}


class TestDataKeyPair:
    """Tests for DataKeyPair value object."""

    def test_contains_both_keys(self):
        """Should contain both plaintext and encrypted keys."""
        now = datetime.now(UTC)
        key_ref = KeyReference(
            provider="vault",
            key_id="master-key",
            key_alias=None,
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=now,
        )
        encrypted_key = EncryptedData(
            ciphertext=b"encrypted-key",
            key_reference=key_ref,
            algorithm="aes256-gcm96",
            encrypted_at=now,
            context={},
        )

        pair = DataKeyPair(
            plaintext_key=b"32-byte-key-here-for-aes256!!!!",
            encrypted_key=encrypted_key,
            key_reference=key_ref,
        )

        assert len(pair.plaintext_key) == 32
        assert pair.encrypted_key.ciphertext == b"encrypted-key"


class TestKeyPurpose:
    """Tests for KeyPurpose enum."""

    def test_encrypt_decrypt_value(self):
        assert KeyPurpose.ENCRYPT_DECRYPT.value == "encrypt_decrypt"

    def test_sign_verify_value(self):
        assert KeyPurpose.SIGN_VERIFY.value == "sign_verify"


class TestRotationPolicy:
    """Tests for RotationPolicy value object."""

    def test_disabled_rotation(self):
        policy = RotationPolicy(
            enabled=False,
            rotation_period_days=0,
            last_rotation=None,
            next_rotation=None,
        )
        assert policy.enabled is False

    def test_enabled_rotation_with_schedule(self):
        now = datetime.now(UTC)
        policy = RotationPolicy(
            enabled=True,
            rotation_period_days=90,
            last_rotation=now,
            next_rotation=None,
        )
        assert policy.enabled is True
        assert policy.rotation_period_days == 90
```

Run tests (should fail):

```bash
uv run pytest tests/unit/security/kms/test_domain_models.py -v
```

##### GREEN Phase

Create `src/fraiseql/security/kms/domain/models.py`:

```python
"""KMS domain models.

Value objects for key management operations.

Algorithm Naming Convention:
---------------------------
Each provider returns its native algorithm identifier in EncryptedData.algorithm:

| Provider | Algorithm String              | Actual Algorithm        |
|----------|------------------------------|-------------------------|
| Vault    | "aes256-gcm96"               | AES-256-GCM (96-bit IV) |
| AWS      | "SYMMETRIC_DEFAULT"          | AES-256-GCM             |
| GCP      | "GOOGLE_SYMMETRIC_ENCRYPTION"| AES-256-GCM             |

NOTE: Algorithm strings are provider-scoped. Do not compare algorithms across
providers directly. If you need to verify algorithm compatibility, check against
known values for the specific provider in key_reference.provider.

All three providers use AES-256-GCM under the hood, but their naming differs.
"""

from dataclasses import dataclass
from datetime import datetime
from enum import Enum
from typing import Any


class KeyPurpose(Enum):
    """Intended use of the key."""

    ENCRYPT_DECRYPT = "encrypt_decrypt"
    SIGN_VERIFY = "sign_verify"
    MAC = "mac"


class KeyState(Enum):
    """Current state of the key."""

    ENABLED = "enabled"
    DISABLED = "disabled"
    PENDING_DELETION = "pending_deletion"
    DESTROYED = "destroyed"


@dataclass(frozen=True)
class KeyReference:
    """Immutable reference to a key in KMS.

    Attributes:
        provider: Provider identifier (e.g., 'vault', 'aws', 'gcp')
        key_id: Provider-specific key identifier
        key_alias: Human-readable alias (optional)
        purpose: Intended use of the key
        created_at: When the key was created
    """

    provider: str
    key_id: str
    key_alias: str | None
    purpose: KeyPurpose
    created_at: datetime

    @property
    def qualified_id(self) -> str:
        """Fully qualified key identifier."""
        return f"{self.provider}:{self.key_id}"


@dataclass(frozen=True)
class EncryptedData:
    """Encrypted data with metadata.

    Attributes:
        ciphertext: The encrypted bytes
        key_reference: Reference to the key used
        algorithm: Encryption algorithm used
        encrypted_at: When encryption occurred
        context: Additional authenticated data (AAD)
    """

    ciphertext: bytes
    key_reference: KeyReference
    algorithm: str
    encrypted_at: datetime
    context: dict[str, str]

    def to_dict(self) -> dict[str, Any]:
        """Serialize for storage."""
        return {
            "ciphertext": self.ciphertext.hex(),
            "key_id": self.key_reference.qualified_id,
            "algorithm": self.algorithm,
            "encrypted_at": self.encrypted_at.isoformat(),
            "context": self.context,
        }


@dataclass(frozen=True)
class DataKeyPair:
    """Data key pair for envelope encryption.

    Attributes:
        plaintext_key: Use immediately, never persist
        encrypted_key: Persist alongside encrypted data
        key_reference: Master key used for wrapping
    """

    plaintext_key: bytes
    encrypted_key: EncryptedData
    key_reference: KeyReference


@dataclass(frozen=True)
class RotationPolicy:
    """Key rotation configuration.

    Attributes:
        enabled: Whether automatic rotation is enabled
        rotation_period_days: Days between rotations
        last_rotation: When key was last rotated
        next_rotation: When key will next be rotated
    """

    enabled: bool
    rotation_period_days: int
    last_rotation: datetime | None
    next_rotation: datetime | None
```

Create `src/fraiseql/security/kms/domain/__init__.py`:

```python
"""KMS domain layer."""

from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
    KeyPurpose,
    KeyReference,
    KeyState,
    RotationPolicy,
)

__all__ = [
    "DataKeyPair",
    "EncryptedData",
    "KeyPurpose",
    "KeyReference",
    "KeyState",
    "RotationPolicy",
]
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_domain_models.py -v
uv run ruff check src/fraiseql/security/kms/
uv run mypy src/fraiseql/security/kms/
```

---

### Task 2.2: Create BaseKMSProvider ABC and Exceptions

**Complexity:** Medium

#### Context

The abstract base class provides:
1. **Template methods** - `encrypt()`, `decrypt()`, `generate_data_key()` with common logic
2. **Abstract hooks** - `_do_encrypt()`, `_do_decrypt()` for provider-specific implementation
3. **Shared utilities** - Context normalization, error wrapping, logging

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_base_provider.py`:

```python
"""Tests for BaseKMSProvider ABC."""

import pytest
from datetime import datetime, UTC
from unittest.mock import AsyncMock

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.models import (
    EncryptedData,
    KeyReference,
    KeyPurpose,
    DataKeyPair,
    RotationPolicy,
)
from fraiseql.security.kms.domain.exceptions import (
    EncryptionError,
    DecryptionError,
)


class ConcreteTestProvider(BaseKMSProvider):
    """Concrete implementation for testing."""

    @property
    def provider_name(self) -> str:
        return "test"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Return (ciphertext, algorithm)."""
        return b"encrypted:" + plaintext, "test-algo"

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Return plaintext."""
        return ciphertext.replace(b"encrypted:", b"")

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Return (plaintext_key, encrypted_key)."""
        return b"0" * 32, b"encrypted-key"

    async def _do_rotate_key(self, key_id: str) -> None:
        pass

    async def _do_get_key_info(self, key_id: str) -> dict:
        return {"alias": None, "created_at": datetime.now(UTC)}

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        return {"enabled": False, "period_days": 0}


class TestBaseKMSProvider:
    """Tests for BaseKMSProvider."""

    @pytest.fixture
    def provider(self):
        return ConcreteTestProvider()

    @pytest.mark.asyncio
    async def test_encrypt_returns_encrypted_data(self, provider):
        """encrypt() should return EncryptedData with metadata."""
        result = await provider.encrypt(b"plaintext", "my-key")

        assert isinstance(result, EncryptedData)
        assert result.ciphertext == b"encrypted:plaintext"
        assert result.key_reference.provider == "test"
        assert result.key_reference.key_id == "my-key"
        assert result.algorithm == "test-algo"

    @pytest.mark.asyncio
    async def test_encrypt_normalizes_context(self, provider):
        """encrypt() should handle None context."""
        result = await provider.encrypt(b"data", "key", context=None)
        assert result.context == {}

    @pytest.mark.asyncio
    async def test_decrypt_returns_plaintext(self, provider):
        """decrypt() should return plaintext bytes."""
        encrypted = EncryptedData(
            ciphertext=b"encrypted:secret",
            key_reference=KeyReference(
                provider="test",
                key_id="my-key",
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            ),
            algorithm="test-algo",
            encrypted_at=datetime.now(UTC),
            context={},
        )

        result = await provider.decrypt(encrypted)

        assert result == b"secret"

    @pytest.mark.asyncio
    async def test_generate_data_key_returns_pair(self, provider):
        """generate_data_key() should return DataKeyPair."""
        result = await provider.generate_data_key("master-key")

        assert isinstance(result, DataKeyPair)
        assert len(result.plaintext_key) == 32
        assert result.encrypted_key.ciphertext == b"encrypted-key"

    def test_cannot_instantiate_abc_directly(self):
        """Should not be able to instantiate ABC without implementing abstracts."""
        with pytest.raises(TypeError):
            BaseKMSProvider()


class TestExceptionWrapping:
    """Tests for exception handling in base class."""

    @pytest.mark.asyncio
    async def test_encrypt_wraps_exceptions(self):
        """Exceptions in _do_encrypt should be wrapped in EncryptionError."""

        class FailingProvider(ConcreteTestProvider):
            async def _do_encrypt(self, plaintext, key_id, context):
                raise RuntimeError("Connection failed")

        provider = FailingProvider()

        with pytest.raises(EncryptionError) as exc_info:
            await provider.encrypt(b"data", "key")

        assert "Connection failed" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_decrypt_wraps_exceptions(self):
        """Exceptions in _do_decrypt should be wrapped in DecryptionError."""

        class FailingProvider(ConcreteTestProvider):
            async def _do_decrypt(self, ciphertext, key_id, context):
                raise RuntimeError("Invalid ciphertext")

        provider = FailingProvider()
        encrypted = EncryptedData(
            ciphertext=b"bad",
            key_reference=KeyReference(
                provider="test",
                key_id="key",
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            ),
            algorithm="test",
            encrypted_at=datetime.now(UTC),
            context={},
        )

        with pytest.raises(DecryptionError) as exc_info:
            await provider.decrypt(encrypted)

        assert "Invalid ciphertext" in str(exc_info.value)
```

##### GREEN Phase

Create `src/fraiseql/security/kms/domain/exceptions.py`:

```python
"""KMS domain exceptions."""


class KMSError(Exception):
    """Base exception for KMS operations."""

    pass


class KeyNotFoundError(KMSError):
    """Raised when a key is not found."""

    pass


class EncryptionError(KMSError):
    """Raised when encryption fails."""

    pass


class DecryptionError(KMSError):
    """Raised when decryption fails."""

    pass


class KeyRotationError(KMSError):
    """Raised when key rotation fails."""

    pass


class ProviderConnectionError(KMSError):
    """Raised when connection to KMS provider fails."""

    pass
```

Create `src/fraiseql/security/kms/domain/base.py`:

```python
"""Base KMS provider abstract class.

Provides template methods with common logic and abstract hooks
for provider-specific implementations.
"""

from abc import ABC, abstractmethod
from datetime import datetime, UTC
import logging

from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
    KeyPurpose,
    KeyReference,
    RotationPolicy,
)
from fraiseql.security.kms.domain.exceptions import (
    DecryptionError,
    EncryptionError,
    KeyRotationError,
)

logger = logging.getLogger(__name__)


class BaseKMSProvider(ABC):
    """Abstract base class for KMS providers.

    Implements the Template Method pattern:
    - Public methods (encrypt, decrypt, etc.) handle common logic
    - Protected abstract methods (_do_encrypt, _do_decrypt, etc.) are
      implemented by concrete providers

    Subclasses must implement:
    - provider_name (property)
    - _do_encrypt()
    - _do_decrypt()
    - _do_generate_data_key()
    - _do_rotate_key()
    - _do_get_key_info()
    - _do_get_rotation_policy()
    """

    @property
    @abstractmethod
    def provider_name(self) -> str:
        """Unique provider identifier (e.g., 'vault', 'aws', 'gcp')."""
        ...

    # ─────────────────────────────────────────────────────────────
    # Template Methods (public API)
    # ─────────────────────────────────────────────────────────────

    async def encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        *,
        context: dict[str, str] | None = None,
    ) -> EncryptedData:
        """Encrypt data using the specified key.

        Args:
            plaintext: Data to encrypt
            key_id: Key identifier
            context: Additional authenticated data (AAD)

        Returns:
            EncryptedData with ciphertext and metadata

        Raises:
            EncryptionError: If encryption fails
        """
        ctx = context or {}
        logger.debug(
            "Encrypting %d bytes with key %s",
            len(plaintext),
            key_id,
        )

        try:
            ciphertext, algorithm = await self._do_encrypt(plaintext, key_id, ctx)

            return EncryptedData(
                ciphertext=ciphertext,
                key_reference=KeyReference(
                    provider=self.provider_name,
                    key_id=key_id,
                    key_alias=None,
                    purpose=KeyPurpose.ENCRYPT_DECRYPT,
                    created_at=datetime.now(UTC),
                ),
                algorithm=algorithm,
                encrypted_at=datetime.now(UTC),
                context=ctx,
            )
        except EncryptionError:
            raise
        except Exception as e:
            # SECURITY: Log full error for debugging, but sanitize message to caller
            # to avoid leaking sensitive info (key IDs, vault paths, AWS ARNs)
            logger.error(
                "Encryption failed for key %s: %s",
                key_id,
                e,
                exc_info=True,
            )
            raise EncryptionError("Encryption operation failed") from e

    async def decrypt(
        self,
        encrypted: EncryptedData,
        *,
        context: dict[str, str] | None = None,
    ) -> bytes:
        """Decrypt data.

        Args:
            encrypted: EncryptedData to decrypt
            context: Override context (uses encrypted.context if not provided)

        Returns:
            Decrypted plaintext bytes

        Raises:
            DecryptionError: If decryption fails
        """
        ctx = context or encrypted.context
        key_id = encrypted.key_reference.key_id
        logger.debug("Decrypting data with key %s", key_id)

        try:
            return await self._do_decrypt(encrypted.ciphertext, key_id, ctx)
        except DecryptionError:
            raise
        except Exception as e:
            # SECURITY: Log full error for debugging, but sanitize message to caller
            logger.error(
                "Decryption failed for key %s: %s",
                key_id,
                e,
                exc_info=True,
            )
            raise DecryptionError("Decryption operation failed") from e

    async def generate_data_key(
        self,
        key_id: str,
        *,
        context: dict[str, str] | None = None,
    ) -> DataKeyPair:
        """Generate a data encryption key (envelope encryption).

        Args:
            key_id: Master key to wrap the data key
            context: Additional authenticated data

        Returns:
            DataKeyPair with plaintext and encrypted data key
        """
        ctx = context or {}
        logger.debug("Generating data key with master key %s", key_id)

        try:
            plaintext_key, encrypted_key_bytes = await self._do_generate_data_key(
                key_id, ctx
            )

            key_ref = KeyReference(
                provider=self.provider_name,
                key_id=key_id,
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            )

            return DataKeyPair(
                plaintext_key=plaintext_key,
                encrypted_key=EncryptedData(
                    ciphertext=encrypted_key_bytes,
                    key_reference=key_ref,
                    algorithm="data-key",
                    encrypted_at=datetime.now(UTC),
                    context=ctx,
                ),
                key_reference=key_ref,
            )
        except Exception as e:
            # SECURITY: Log full error, sanitize message to caller
            logger.error(
                "Data key generation failed for key %s: %s",
                key_id,
                e,
                exc_info=True,
            )
            raise EncryptionError("Data key generation failed") from e

    async def rotate_key(self, key_id: str) -> KeyReference:
        """Rotate the specified key."""
        logger.info("Rotating key %s", key_id)
        try:
            await self._do_rotate_key(key_id)
            return await self.get_key_info(key_id)
        except Exception as e:
            # SECURITY: Log full error, sanitize message to caller
            logger.error(
                "Key rotation failed for key %s: %s",
                key_id,
                e,
                exc_info=True,
            )
            raise KeyRotationError("Key rotation failed") from e

    async def get_key_info(self, key_id: str) -> KeyReference:
        """Get key metadata."""
        info = await self._do_get_key_info(key_id)
        return KeyReference(
            provider=self.provider_name,
            key_id=key_id,
            key_alias=info.get("alias"),
            purpose=KeyPurpose.ENCRYPT_DECRYPT,
            created_at=info.get("created_at", datetime.now(UTC)),
        )

    async def get_rotation_policy(self, key_id: str) -> RotationPolicy:
        """Get key rotation policy."""
        policy = await self._do_get_rotation_policy(key_id)
        return RotationPolicy(
            enabled=policy.get("enabled", False),
            rotation_period_days=policy.get("period_days", 0),
            last_rotation=policy.get("last_rotation"),
            next_rotation=policy.get("next_rotation"),
        )

    # ─────────────────────────────────────────────────────────────
    # Abstract Methods (provider-specific hooks)
    # ─────────────────────────────────────────────────────────────

    @abstractmethod
    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Provider-specific encryption.

        Args:
            plaintext: Data to encrypt
            key_id: Key identifier
            context: AAD context (never None)

        Returns:
            Tuple of (ciphertext, algorithm_name)
        """
        ...

    @abstractmethod
    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Provider-specific decryption.

        Args:
            ciphertext: Data to decrypt
            key_id: Key identifier
            context: AAD context (never None)

        Returns:
            Decrypted plaintext
        """
        ...

    @abstractmethod
    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Provider-specific data key generation.

        Args:
            key_id: Master key identifier
            context: AAD context (never None)

        Returns:
            Tuple of (plaintext_key, encrypted_key)
        """
        ...

    @abstractmethod
    async def _do_rotate_key(self, key_id: str) -> None:
        """Provider-specific key rotation."""
        ...

    @abstractmethod
    async def _do_get_key_info(self, key_id: str) -> dict:
        """Provider-specific key info retrieval.

        Returns:
            Dict with 'alias' (str|None) and 'created_at' (datetime)
        """
        ...

    @abstractmethod
    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Provider-specific rotation policy retrieval.

        Returns:
            Dict with 'enabled' (bool), 'period_days' (int),
            'last_rotation' (datetime|None), 'next_rotation' (datetime|None)
        """
        ...
```

Update `src/fraiseql/security/kms/domain/__init__.py`:

```python
"""KMS domain layer."""

from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
    KeyPurpose,
    KeyReference,
    KeyState,
    RotationPolicy,
)
from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import (
    KMSError,
    KeyNotFoundError,
    EncryptionError,
    DecryptionError,
    KeyRotationError,
    ProviderConnectionError,
)

__all__ = [
    # Models
    "DataKeyPair",
    "EncryptedData",
    "KeyPurpose",
    "KeyReference",
    "KeyState",
    "RotationPolicy",
    # Base class
    "BaseKMSProvider",
    # Exceptions
    "KMSError",
    "KeyNotFoundError",
    "EncryptionError",
    "DecryptionError",
    "KeyRotationError",
    "ProviderConnectionError",
]
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/ -v
uv run ruff check src/fraiseql/security/kms/
uv run mypy src/fraiseql/security/kms/
```

---

### Task 2.3: Implement HashiCorp Vault Provider

**Complexity:** Medium (extends BaseKMSProvider)

#### Context

HashiCorp Vault Transit secrets engine provides encryption-as-a-service. This provider extends `BaseKMSProvider` and only needs to implement the `_do_*` hooks.

**Note:** `httpx` is already a FraiseQL dependency (under `auth0` extra).

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_vault_provider.py`:

```python
"""Tests for Vault KMS provider."""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch
import base64

from fraiseql.security.kms.infrastructure.vault import (
    VaultKMSProvider,
    VaultConfig,
)
from fraiseql.security.kms.domain.base import BaseKMSProvider


class TestVaultConfig:
    """Tests for VaultConfig."""

    def test_default_mount_path(self):
        """Default mount path should be 'transit'."""
        config = VaultConfig(
            vault_addr="http://localhost:8200",
            token="test-token",
        )
        assert config.mount_path == "transit"

    def test_api_url_construction(self):
        """Should construct correct API URL."""
        config = VaultConfig(
            vault_addr="http://localhost:8200",
            token="test-token",
            mount_path="transit",
        )
        expected = "http://localhost:8200/v1/transit/encrypt/my-key"
        assert config.api_url("encrypt/my-key") == expected


class TestVaultKMSProvider:
    """Tests for VaultKMSProvider."""

    @pytest.fixture
    def config(self):
        return VaultConfig(
            vault_addr="http://localhost:8200",
            token="test-token",
        )

    @pytest.fixture
    def provider(self, config):
        return VaultKMSProvider(config)

    def test_extends_base_provider(self, provider):
        """Should extend BaseKMSProvider."""
        assert isinstance(provider, BaseKMSProvider)

    def test_provider_name(self, provider):
        """Provider name should be 'vault'."""
        assert provider.provider_name == "vault"

    @pytest.mark.asyncio
    async def test_do_encrypt_calls_vault_api(self, provider):
        """_do_encrypt should call Vault transit/encrypt endpoint."""
        with patch("httpx.AsyncClient") as mock_client_class:
            mock_client = AsyncMock()
            mock_client_class.return_value.__aenter__.return_value = mock_client

            mock_response = MagicMock()
            mock_response.json.return_value = {
                "data": {"ciphertext": "vault:v1:encrypted-base64-data"}
            }
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            ciphertext, algo = await provider._do_encrypt(
                b"plaintext",
                "my-key",
                {"purpose": "test"},
            )

            mock_client.post.assert_called_once()
            assert algo == "aes256-gcm96"

    @pytest.mark.asyncio
    async def test_do_decrypt_calls_vault_api(self, provider):
        """_do_decrypt should call Vault transit/decrypt endpoint."""
        with patch("httpx.AsyncClient") as mock_client_class:
            mock_client = AsyncMock()
            mock_client_class.return_value.__aenter__.return_value = mock_client

            mock_response = MagicMock()
            plaintext_b64 = base64.b64encode(b"decrypted").decode()
            mock_response.json.return_value = {
                "data": {"plaintext": plaintext_b64}
            }
            mock_response.raise_for_status = MagicMock()
            mock_client.post.return_value = mock_response

            result = await provider._do_decrypt(
                b"vault:v1:ciphertext",
                "my-key",
                {},
            )

            assert result == b"decrypted"
```

##### GREEN Phase

Create `src/fraiseql/security/kms/infrastructure/vault.py`:

```python
"""HashiCorp Vault Transit secrets engine provider."""

from dataclasses import dataclass
import base64
import json

import httpx

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import KeyNotFoundError


@dataclass
class VaultConfig:
    """Configuration for Vault KMS provider.

    SECURITY CONSIDERATIONS:
    ------------------------
    Token Handling:
    - The Vault token is stored in memory for the provider's lifetime
    - Python cannot securely erase memory, so tokens may persist until GC
    - For production deployments, consider:
      1. Using short-lived tokens with automatic renewal
      2. Vault Agent with auto-auth for token management
      3. AppRole authentication with response wrapping
      4. Kubernetes auth method in K8s environments

    Recommended Production Setup:
    - Run Vault Agent as a sidecar that handles authentication
    - Configure token_file to read from Agent's sink file
    - Enable token renewal in Vault Agent config
    - Use response wrapping for initial token delivery
    """

    vault_addr: str
    token: str
    mount_path: str = "transit"
    namespace: str | None = None
    verify_tls: bool = True
    timeout: float = 30.0
    # Future: Support token file for Vault Agent integration
    # token_file: str | None = None

    def api_url(self, path: str) -> str:
        """Build full API URL for a path."""
        addr = self.vault_addr.rstrip("/")
        return f"{addr}/v1/{self.mount_path}/{path}"

    @property
    def headers(self) -> dict[str, str]:
        """Build request headers."""
        headers = {"X-Vault-Token": self.token}
        if self.namespace:
            headers["X-Vault-Namespace"] = self.namespace
        return headers


class VaultKMSProvider(BaseKMSProvider):
    """HashiCorp Vault Transit secrets engine implementation.

    Extends BaseKMSProvider - only implements the _do_* hooks.

    IMPORTANT - Context Parameter Semantics:
    ----------------------------------------
    Vault Transit's `context` parameter is used for **key derivation** with
    convergent encryption keys, NOT as Additional Authenticated Data (AAD).

    Behavior differs by key type:
    - **Convergent keys** (derived=true): `context` is REQUIRED and determines
      the derived key. Same plaintext + same context = same ciphertext.
    - **Standard keys** (derived=false): `context` is IGNORED by Vault.

    This provider JSON-encodes and base64-encodes the context dict for
    convergent key compatibility. If using standard keys, the context
    parameter has no cryptographic effect (but is still stored in metadata).

    For true AAD support, consider using Vault's Transit AEAD operations
    directly, which require Vault 1.9+ and specific key configurations.
    """

    def __init__(self, config: VaultConfig) -> None:
        self._config = config

    @property
    def provider_name(self) -> str:
        return "vault"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Encrypt using Vault Transit."""
        payload: dict[str, str] = {
            "plaintext": base64.b64encode(plaintext).decode(),
        }
        if context:
            ctx_bytes = json.dumps(context, sort_keys=True).encode()
            payload["context"] = base64.b64encode(ctx_bytes).decode()

        async with httpx.AsyncClient(
            verify=self._config.verify_tls,
            timeout=self._config.timeout,
        ) as client:
            response = await client.post(
                self._config.api_url(f"encrypt/{key_id}"),
                headers=self._config.headers,
                json=payload,
            )
            self._check_response(response, key_id)
            data = response.json()

            ciphertext = data["data"]["ciphertext"]
            return ciphertext.encode(), "aes256-gcm96"

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt using Vault Transit."""
        payload: dict[str, str] = {
            "ciphertext": ciphertext.decode(),
        }
        if context:
            ctx_bytes = json.dumps(context, sort_keys=True).encode()
            payload["context"] = base64.b64encode(ctx_bytes).decode()

        async with httpx.AsyncClient(
            verify=self._config.verify_tls,
            timeout=self._config.timeout,
        ) as client:
            response = await client.post(
                self._config.api_url(f"decrypt/{key_id}"),
                headers=self._config.headers,
                json=payload,
            )
            self._check_response(response, key_id)
            data = response.json()

            return base64.b64decode(data["data"]["plaintext"])

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Generate data key using Vault Transit."""
        payload: dict[str, str] = {}
        if context:
            ctx_bytes = json.dumps(context, sort_keys=True).encode()
            payload["context"] = base64.b64encode(ctx_bytes).decode()

        async with httpx.AsyncClient(
            verify=self._config.verify_tls,
            timeout=self._config.timeout,
        ) as client:
            response = await client.post(
                self._config.api_url(f"datakey/plaintext/{key_id}"),
                headers=self._config.headers,
                json=payload,
            )
            self._check_response(response, key_id)
            data = response.json()

            plaintext_key = base64.b64decode(data["data"]["plaintext"])
            encrypted_key = data["data"]["ciphertext"].encode()
            return plaintext_key, encrypted_key

    async def _do_rotate_key(self, key_id: str) -> None:
        """Rotate key in Vault Transit."""
        async with httpx.AsyncClient(
            verify=self._config.verify_tls,
            timeout=self._config.timeout,
        ) as client:
            response = await client.post(
                self._config.api_url(f"keys/{key_id}/rotate"),
                headers=self._config.headers,
            )
            self._check_response(response, key_id)

    async def _do_get_key_info(self, key_id: str) -> dict:
        """Get key info from Vault Transit."""
        async with httpx.AsyncClient(
            verify=self._config.verify_tls,
            timeout=self._config.timeout,
        ) as client:
            response = await client.get(
                self._config.api_url(f"keys/{key_id}"),
                headers=self._config.headers,
            )
            self._check_response(response, key_id)
            data = response.json()
            return {
                "alias": data["data"].get("name"),
                "created_at": None,  # Vault doesn't expose this easily
            }

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Get rotation policy from Vault Transit."""
        async with httpx.AsyncClient(
            verify=self._config.verify_tls,
            timeout=self._config.timeout,
        ) as client:
            response = await client.get(
                self._config.api_url(f"keys/{key_id}"),
                headers=self._config.headers,
            )
            self._check_response(response, key_id)
            data = response.json()
            period = data["data"].get("auto_rotate_period", 0)
            return {
                "enabled": period > 0,
                "period_days": period // 86400 if period else 0,
                "last_rotation": None,
                "next_rotation": None,
            }

    def _check_response(self, response: httpx.Response, key_id: str) -> None:
        """Check response and raise appropriate exception."""
        if response.status_code == 404:
            raise KeyNotFoundError(f"Key not found: {key_id}")
        response.raise_for_status()
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_vault_provider.py -v
```

---

### Task 2.4: Implement AWS KMS Provider

**Complexity:** Medium (extends BaseKMSProvider)

#### Dependencies

Add to `pyproject.toml`:

```toml
kms-aws = [
    "aioboto3>=12.0.0",
]
```

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_aws_kms_provider.py`:

```python
"""Tests for AWS KMS provider."""

import pytest
from unittest.mock import AsyncMock, patch

from fraiseql.security.kms.infrastructure.aws_kms import (
    AWSKMSProvider,
    AWSKMSConfig,
)
from fraiseql.security.kms.domain.base import BaseKMSProvider


class TestAWSKMSConfig:
    def test_requires_region(self):
        config = AWSKMSConfig(region="us-east-1")
        assert config.region == "us-east-1"

    def test_endpoint_url_for_localstack(self):
        config = AWSKMSConfig(region="us-east-1", endpoint_url="http://localhost:4566")
        assert config.endpoint_url == "http://localhost:4566"


class TestAWSKMSProvider:
    @pytest.fixture
    def config(self):
        return AWSKMSConfig(region="us-east-1")

    def test_extends_base_provider(self, config):
        with patch("aioboto3.Session"):
            provider = AWSKMSProvider(config)
            assert isinstance(provider, BaseKMSProvider)

    def test_provider_name(self, config):
        with patch("aioboto3.Session"):
            provider = AWSKMSProvider(config)
            assert provider.provider_name == "aws"
```

##### GREEN Phase

Create `src/fraiseql/security/kms/infrastructure/aws_kms.py`:

```python
"""AWS KMS provider."""

from dataclasses import dataclass
from contextlib import asynccontextmanager
from typing import AsyncIterator, Any
import secrets

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import KeyNotFoundError

try:
    import aioboto3
    AIOBOTO3_AVAILABLE = True
except ImportError:
    AIOBOTO3_AVAILABLE = False
    aioboto3 = None  # type: ignore


@dataclass
class AWSKMSConfig:
    """Configuration for AWS KMS provider."""
    region: str
    endpoint_url: str | None = None  # For LocalStack testing
    aws_access_key_id: str | None = None
    aws_secret_access_key: str | None = None


class AWSKMSProvider(BaseKMSProvider):
    """AWS KMS implementation.

    Extends BaseKMSProvider - only implements the _do_* hooks.
    """

    def __init__(self, config: AWSKMSConfig) -> None:
        if not AIOBOTO3_AVAILABLE:
            raise ImportError("aioboto3 required: pip install aioboto3")
        self._config = config
        self._session = aioboto3.Session(
            aws_access_key_id=config.aws_access_key_id,
            aws_secret_access_key=config.aws_secret_access_key,
            region_name=config.region,
        )

    @property
    def provider_name(self) -> str:
        return "aws"

    @asynccontextmanager
    async def _get_client(self) -> AsyncIterator[Any]:
        async with self._session.client(
            "kms", endpoint_url=self._config.endpoint_url
        ) as client:
            yield client

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        async with self._get_client() as client:
            params: dict[str, Any] = {"KeyId": key_id, "Plaintext": plaintext}
            if context:
                params["EncryptionContext"] = context
            response = await client.encrypt(**params)
            return response["CiphertextBlob"], response.get("EncryptionAlgorithm", "SYMMETRIC_DEFAULT")

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt ciphertext using AWS KMS.

        SECURITY: We explicitly include KeyId even though AWS KMS doesn't
        require it for symmetric keys. This provides:
        1. Validation that the expected key was used
        2. Prevention of cross-account decryption via grants
        3. Explicit key binding in audit logs
        """
        async with self._get_client() as client:
            params: dict[str, Any] = {
                "CiphertextBlob": ciphertext,
                "KeyId": key_id,  # SECURITY: Explicit key validation
            }
            if context:
                params["EncryptionContext"] = context
            response = await client.decrypt(**params)
            return response["Plaintext"]

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        async with self._get_client() as client:
            params: dict[str, Any] = {"KeyId": key_id, "KeySpec": "AES_256"}
            if context:
                params["EncryptionContext"] = context
            response = await client.generate_data_key(**params)
            return response["Plaintext"], response["CiphertextBlob"]

    async def _do_rotate_key(self, key_id: str) -> None:
        async with self._get_client() as client:
            await client.enable_key_rotation(KeyId=key_id)

    async def _do_get_key_info(self, key_id: str) -> dict:
        async with self._get_client() as client:
            response = await client.describe_key(KeyId=key_id)
            metadata = response["KeyMetadata"]
            return {"alias": None, "created_at": metadata["CreationDate"]}

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        async with self._get_client() as client:
            response = await client.get_key_rotation_status(KeyId=key_id)
            return {
                "enabled": response["KeyRotationEnabled"],
                "period_days": 365,  # AWS default
                "last_rotation": None,
                "next_rotation": response.get("NextRotationDate"),
            }
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_aws_kms_provider.py -v
```

---

### Task 2.5: Implement GCP Cloud KMS Provider

**Complexity:** Medium (extends BaseKMSProvider)

#### Dependencies

Add to `pyproject.toml`:

```toml
kms-gcp = [
    "google-cloud-kms>=2.21.0",
]
```

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_gcp_kms_provider.py`:

```python
"""Tests for GCP Cloud KMS provider."""

import pytest
from unittest.mock import AsyncMock, patch, MagicMock

from fraiseql.security.kms.infrastructure.gcp_kms import (
    GCPKMSProvider,
    GCPKMSConfig,
)
from fraiseql.security.kms.domain.base import BaseKMSProvider


class TestGCPKMSConfig:
    def test_key_path_construction(self):
        config = GCPKMSConfig(
            project_id="my-project",
            location="global",
            key_ring="my-keyring",
        )
        expected = "projects/my-project/locations/global/keyRings/my-keyring/cryptoKeys/my-key"
        assert config.key_path("my-key") == expected


class TestGCPKMSProvider:
    @pytest.fixture
    def config(self):
        return GCPKMSConfig(
            project_id="my-project",
            location="global",
            key_ring="my-keyring",
        )

    def test_extends_base_provider(self, config):
        with patch("fraiseql.security.kms.infrastructure.gcp_kms.kms_v1"):
            provider = GCPKMSProvider(config)
            assert isinstance(provider, BaseKMSProvider)

    def test_provider_name(self, config):
        with patch("fraiseql.security.kms.infrastructure.gcp_kms.kms_v1"):
            provider = GCPKMSProvider(config)
            assert provider.provider_name == "gcp"
```

##### GREEN Phase

Create `src/fraiseql/security/kms/infrastructure/gcp_kms.py`:

```python
"""GCP Cloud KMS provider."""

from dataclasses import dataclass
import json
import secrets

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import KeyNotFoundError

try:
    from google.cloud import kms_v1
    from google.cloud.kms_v1 import types
    GCP_KMS_AVAILABLE = True
except ImportError:
    GCP_KMS_AVAILABLE = False
    kms_v1 = None  # type: ignore
    types = None  # type: ignore


@dataclass
class GCPKMSConfig:
    """Configuration for GCP Cloud KMS provider."""
    project_id: str
    location: str
    key_ring: str

    def key_path(self, key_id: str) -> str:
        return (
            f"projects/{self.project_id}/"
            f"locations/{self.location}/"
            f"keyRings/{self.key_ring}/"
            f"cryptoKeys/{key_id}"
        )


class GCPKMSProvider(BaseKMSProvider):
    """GCP Cloud KMS implementation.

    Extends BaseKMSProvider - only implements the _do_* hooks.

    SECURITY CONSIDERATION - Data Key Generation:
    ---------------------------------------------
    Unlike AWS KMS (GenerateDataKey) and Vault Transit (datakey/plaintext),
    GCP Cloud KMS does not provide a native data key generation API.

    This provider implements envelope encryption by:
    1. Generating a random 32-byte key LOCALLY using secrets.token_bytes()
    2. Encrypting that key with the GCP master key
    3. Returning both the plaintext and encrypted key

    Security Implications:
    - The plaintext key briefly exists in local memory before encryption
    - AWS/Vault generate the key server-side, so plaintext never leaves KMS
    - For GCP, ensure the machine running this code is trusted
    - Consider using GCP's ImportJob for importing pre-generated keys
      if your threat model requires server-side key generation

    The local generation uses Python's `secrets` module which provides
    cryptographically secure random bytes suitable for key material.
    """

    def __init__(self, config: GCPKMSConfig) -> None:
        if not GCP_KMS_AVAILABLE:
            raise ImportError("google-cloud-kms required: pip install google-cloud-kms")
        self._config = config
        self._client: kms_v1.KeyManagementServiceAsyncClient | None = None

    async def _get_client(self) -> kms_v1.KeyManagementServiceAsyncClient:
        """Lazy initialization of async client."""
        if self._client is None:
            self._client = kms_v1.KeyManagementServiceAsyncClient()
        return self._client

    @property
    def provider_name(self) -> str:
        return "gcp"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        client = await self._get_client()
        aad = json.dumps(context, sort_keys=True).encode() if context else None

        request = types.EncryptRequest(
            name=self._config.key_path(key_id),
            plaintext=plaintext,
            additional_authenticated_data=aad,
        )
        response = await client.encrypt(request=request)
        return response.ciphertext, "GOOGLE_SYMMETRIC_ENCRYPTION"

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        client = await self._get_client()
        aad = json.dumps(context, sort_keys=True).encode() if context else None

        request = types.DecryptRequest(
            name=self._config.key_path(key_id),
            ciphertext=ciphertext,
            additional_authenticated_data=aad,
        )
        response = await client.decrypt(request=request)
        return response.plaintext

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Manual envelope encryption (GCP doesn't have native data key gen)."""
        plaintext_key = secrets.token_bytes(32)  # AES-256
        ciphertext, _ = await self._do_encrypt(plaintext_key, key_id, context)
        return plaintext_key, ciphertext

    async def _do_rotate_key(self, key_id: str) -> None:
        client = await self._get_client()
        request = types.CreateCryptoKeyVersionRequest(
            parent=self._config.key_path(key_id)
        )
        await client.create_crypto_key_version(request=request)

    async def _do_get_key_info(self, key_id: str) -> dict:
        client = await self._get_client()
        request = types.GetCryptoKeyRequest(name=self._config.key_path(key_id))
        response = await client.get_crypto_key(request=request)
        return {"alias": response.name, "created_at": response.create_time}

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        client = await self._get_client()
        request = types.GetCryptoKeyRequest(name=self._config.key_path(key_id))
        response = await client.get_crypto_key(request=request)
        period = response.rotation_period
        enabled = period is not None and period.seconds > 0
        return {
            "enabled": enabled,
            "period_days": period.seconds // 86400 if enabled else 0,
            "last_rotation": response.primary.create_time if response.primary else None,
            "next_rotation": response.next_rotation_time,
        }
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_gcp_kms_provider.py -v
```

---

### Task 2.6: Implement KeyManager Application Service

**Complexity:** Medium

#### Context

KeyManager provides a unified interface for encryption operations across multiple KMS providers. It uses `BaseKMSProvider` instances and auto-routes decryption to the correct provider.

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_key_manager.py`:

```python
"""Tests for KeyManager application service."""

import pytest
from datetime import datetime, UTC
from unittest.mock import AsyncMock, MagicMock

from fraiseql.security.kms.application.key_manager import KeyManager
from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.models import (
    EncryptedData,
    KeyReference,
    KeyPurpose,
)


class MockProvider(BaseKMSProvider):
    """Mock provider for testing."""

    def __init__(self, name: str):
        self._name = name

    @property
    def provider_name(self) -> str:
        return self._name

    async def _do_encrypt(self, plaintext, key_id, context):
        return b"encrypted:" + plaintext, "mock-algo"

    async def _do_decrypt(self, ciphertext, key_id, context):
        return ciphertext.replace(b"encrypted:", b"")

    async def _do_generate_data_key(self, key_id, context):
        return b"0" * 32, b"encrypted-key"

    async def _do_rotate_key(self, key_id):
        pass

    async def _do_get_key_info(self, key_id):
        return {"alias": None, "created_at": datetime.now(UTC)}

    async def _do_get_rotation_policy(self, key_id):
        return {"enabled": False, "period_days": 0}


class TestKeyManager:
    @pytest.fixture
    def vault_provider(self):
        return MockProvider("vault")

    @pytest.fixture
    def aws_provider(self):
        return MockProvider("aws")

    @pytest.fixture
    def manager(self, vault_provider, aws_provider):
        return KeyManager(
            providers={"vault": vault_provider, "aws": aws_provider},
            default_provider="vault",
            default_key_id="default-key",
        )

    @pytest.mark.asyncio
    async def test_encrypt_uses_default_provider(self, manager):
        """Should use default provider when not specified."""
        result = await manager.encrypt(b"secret")
        assert result.key_reference.provider == "vault"

    @pytest.mark.asyncio
    async def test_encrypt_with_specified_provider(self, manager):
        """Should use specified provider."""
        result = await manager.encrypt(b"secret", provider="aws")
        assert result.key_reference.provider == "aws"

    @pytest.mark.asyncio
    async def test_decrypt_autodetects_provider(self, manager):
        """Should auto-detect provider from EncryptedData."""
        encrypted = EncryptedData(
            ciphertext=b"encrypted:secret",
            key_reference=KeyReference(
                provider="aws",
                key_id="my-key",
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            ),
            algorithm="mock-algo",
            encrypted_at=datetime.now(UTC),
            context={},
        )

        result = await manager.decrypt(encrypted)
        assert result == b"secret"

    def test_raises_for_unknown_provider(self, manager):
        """Should raise for unknown provider."""
        with pytest.raises(ValueError, match="Unknown provider"):
            manager.get_provider("unknown")

    @pytest.mark.asyncio
    async def test_encrypt_field_handles_strings(self, manager):
        """encrypt_field should handle string input."""
        result = await manager.encrypt_field("secret")
        assert isinstance(result, EncryptedData)

    @pytest.mark.asyncio
    async def test_decrypt_field_returns_string(self, manager):
        """decrypt_field should return string when original was string."""
        encrypted = await manager.encrypt_field("secret")
        result = await manager.decrypt_field(encrypted)
        assert result == "secret"

    @pytest.mark.asyncio
    async def test_decrypt_unknown_provider_raises(self, manager):
        """Should raise ValueError when decrypting data from unknown provider."""
        encrypted = EncryptedData(
            ciphertext=b"encrypted:secret",
            key_reference=KeyReference(
                provider="azure",  # Not registered in manager
                key_id="my-key",
                key_alias=None,
                purpose=KeyPurpose.ENCRYPT_DECRYPT,
                created_at=datetime.now(UTC),
            ),
            algorithm="mock-algo",
            encrypted_at=datetime.now(UTC),
            context={},
        )

        with pytest.raises(ValueError, match="Unknown provider: azure"):
            await manager.decrypt(encrypted)
```

##### GREEN Phase

Create `src/fraiseql/security/kms/application/key_manager.py`:

```python
"""KeyManager application service.

Unified interface for encryption operations across multiple KMS providers.
"""

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
)


class KeyManager:
    """Application service for encryption operations.

    Provides a unified interface for encrypting/decrypting data across
    multiple KMS providers. Auto-routes decryption to the correct provider
    based on the EncryptedData's key_reference.

    Usage:
        vault = VaultKMSProvider(vault_config)
        aws = AWSKMSProvider(aws_config)

        manager = KeyManager(
            providers={"vault": vault, "aws": aws},
            default_provider="vault",
            default_key_id="my-encryption-key",
        )

        # Encrypt
        encrypted = await manager.encrypt(b"secret data")

        # Decrypt (auto-detects provider)
        plaintext = await manager.decrypt(encrypted)
    """

    def __init__(
        self,
        providers: dict[str, BaseKMSProvider],
        default_provider: str,
        default_key_id: str,
        context_prefix: str | None = None,
    ) -> None:
        """Initialize KeyManager.

        Args:
            providers: Map of provider name -> provider instance
            default_provider: Provider to use when not specified
            default_key_id: Key ID to use when not specified
            context_prefix: Optional prefix to add to all encryption contexts
        """
        self._providers = providers
        self._default_provider = default_provider
        self._default_key_id = default_key_id
        self._context_prefix = context_prefix

        if default_provider not in providers:
            raise ValueError(f"Default provider '{default_provider}' not in providers")

    def get_provider(self, name: str) -> BaseKMSProvider:
        """Get a provider by name."""
        if name not in self._providers:
            raise ValueError(f"Unknown provider: {name}")
        return self._providers[name]

    def _build_context(
        self,
        context: dict[str, str] | None = None,
    ) -> dict[str, str]:
        """Build encryption context with optional prefix."""
        ctx = dict(context) if context else {}
        if self._context_prefix:
            ctx["service"] = self._context_prefix
        return ctx

    async def encrypt(
        self,
        plaintext: bytes,
        *,
        key_id: str | None = None,
        provider: str | None = None,
        context: dict[str, str] | None = None,
    ) -> EncryptedData:
        """Encrypt data.

        Args:
            plaintext: Data to encrypt
            key_id: Key ID (defaults to default_key_id)
            provider: Provider name (defaults to default_provider)
            context: Additional encryption context

        Returns:
            EncryptedData with ciphertext and metadata
        """
        prov = self.get_provider(provider or self._default_provider)
        return await prov.encrypt(
            plaintext,
            key_id or self._default_key_id,
            context=self._build_context(context),
        )

    async def decrypt(
        self,
        encrypted: EncryptedData,
        *,
        context: dict[str, str] | None = None,
    ) -> bytes:
        """Decrypt data.

        Auto-detects the correct provider from EncryptedData.

        Args:
            encrypted: Data to decrypt
            context: Optional context override

        Returns:
            Decrypted plaintext
        """
        provider_name = encrypted.key_reference.provider
        prov = self.get_provider(provider_name)
        return await prov.decrypt(
            encrypted,
            context=self._build_context(context) if context else None,
        )

    async def encrypt_field(
        self,
        value: str | bytes,
        *,
        key_id: str | None = None,
        provider: str | None = None,
        context: dict[str, str] | None = None,
    ) -> EncryptedData:
        """Encrypt a field value (string or bytes).

        Convenience method that handles string encoding.
        """
        if isinstance(value, str):
            plaintext = value.encode("utf-8")
            ctx = self._build_context(context)
            ctx["_encoding"] = "utf-8"
        else:
            plaintext = value
            ctx = self._build_context(context)

        prov = self.get_provider(provider or self._default_provider)
        return await prov.encrypt(
            plaintext,
            key_id or self._default_key_id,
            context=ctx,
        )

    async def decrypt_field(
        self,
        encrypted: EncryptedData,
    ) -> str | bytes:
        """Decrypt a field value.

        Returns string if original was string (based on context), bytes otherwise.
        """
        plaintext = await self.decrypt(encrypted)
        if encrypted.context.get("_encoding") == "utf-8":
            return plaintext.decode("utf-8")
        return plaintext

    async def generate_data_key(
        self,
        *,
        key_id: str | None = None,
        provider: str | None = None,
        context: dict[str, str] | None = None,
    ) -> DataKeyPair:
        """Generate a data encryption key for envelope encryption."""
        prov = self.get_provider(provider or self._default_provider)
        return await prov.generate_data_key(
            key_id or self._default_key_id,
            context=self._build_context(context),
        )
```

Create `src/fraiseql/security/kms/application/__init__.py`:

```python
"""KMS application layer."""

from fraiseql.security.kms.application.key_manager import KeyManager

__all__ = ["KeyManager"]
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_key_manager.py -v
```

---

### Task 2.7: Add KMS Dependencies to pyproject.toml

Edit `pyproject.toml`:

```toml
[project.optional-dependencies]
# Individual provider dependencies
kms-vault = [
    "httpx>=0.27.0",
]
kms-aws = [
    "aioboto3>=12.0.0",
]
kms-gcp = [
    "google-cloud-kms>=2.21.0",
]

# All KMS providers
kms = [
    "httpx>=0.27.0",
    "aioboto3>=12.0.0",
    "google-cloud-kms>=2.21.0",
]

# Development testing
kms-dev = [
    "httpx>=0.27.0",
    "aioboto3>=12.0.0",
    "google-cloud-kms>=2.21.0",
    "testcontainers[vault]>=4.0.0",
    "moto[kms]>=5.0.0",
]
```

---

### Phase 2 Checkpoint

```bash
uv run pytest tests/unit/security/kms/ -v
uv run pytest --tb=short
uv run ruff check src/fraiseql/security/kms/
uv run mypy src/fraiseql/security/kms/
```

**Phase 2 Completion Checklist:**
- [ ] KMS domain models implemented
- [ ] BaseKMSProvider ABC implemented with template methods
- [ ] KMS exceptions implemented
- [ ] Vault provider implemented (extends BaseKMSProvider)
- [ ] AWS KMS provider implemented (extends BaseKMSProvider)
- [ ] GCP Cloud KMS provider implemented (extends BaseKMSProvider)
- [ ] KeyManager application service implemented
- [ ] Dependencies added to pyproject.toml

---

## Phase 3: Observability & Security Profiles

**Theme:** "Traceability & Safe Operation"

### Goals

1. Enhance OpenTelemetry integration for GraphQL
2. Implement security profiles (standard, regulated, restricted)
3. Integrate body size limits into profiles

---

### Task 3.1: Enhance OpenTelemetry Tracing

**Complexity:** Medium

#### Context

Add GraphQL-specific tracing that integrates with OpenTelemetry. OpenTelemetry is already a FraiseQL dependency under the `tracing` extra.

#### TDD Cycle

##### RED Phase

Create `tests/unit/tracing/test_graphql_tracing.py`:

```python
"""Tests for GraphQL-specific tracing."""

import pytest

from fraiseql.tracing.graphql_tracing import (
    GraphQLTracer,
    TracingConfig,
)


class TestTracingConfig:
    """Tests for TracingConfig."""

    def test_default_trace_resolvers_true(self):
        config = TracingConfig()
        assert config.trace_resolvers is True

    def test_default_include_variables_false(self):
        """Should not include variables by default (security)."""
        config = TracingConfig()
        assert config.include_variables is False

    def test_default_sanitize_variables_patterns(self):
        """Should have default patterns for sensitive variable names."""
        config = TracingConfig()
        assert "password" in config.sanitize_patterns
        assert "token" in config.sanitize_patterns
        assert "secret" in config.sanitize_patterns

    def test_max_query_length_default(self):
        config = TracingConfig()
        assert config.max_query_length == 1000


class TestVariableSanitization:
    """Tests for variable sanitization in tracing."""

    @pytest.fixture
    def tracer_with_sanitization(self):
        config = TracingConfig(
            include_variables=True,
            sanitize_variables=True,
        )
        return GraphQLTracer(config)

    def test_sanitizes_password_variables(self, tracer_with_sanitization):
        """Should mask password-like variable values."""
        variables = {"username": "alice", "password": "secret123"}
        sanitized = tracer_with_sanitization._sanitize_variables(variables)
        assert sanitized["username"] == "alice"
        assert sanitized["password"] == "[REDACTED]"

    def test_sanitizes_nested_sensitive_fields(self, tracer_with_sanitization):
        """Should mask nested sensitive fields."""
        variables = {
            "input": {
                "email": "alice@example.com",
                "apiToken": "tok_12345",
            }
        }
        sanitized = tracer_with_sanitization._sanitize_variables(variables)
        assert sanitized["input"]["email"] == "alice@example.com"
        assert sanitized["input"]["apiToken"] == "[REDACTED]"

    def test_custom_sanitize_patterns(self):
        """Should support custom sanitization patterns."""
        config = TracingConfig(
            include_variables=True,
            sanitize_variables=True,
            sanitize_patterns=["password", "ssn", "credit_card"],
        )
        tracer = GraphQLTracer(config)
        variables = {"name": "Alice", "ssn": "123-45-6789"}
        sanitized = tracer._sanitize_variables(variables)
        assert sanitized["ssn"] == "[REDACTED]"


class TestGraphQLTracer:
    """Tests for GraphQLTracer."""

    @pytest.fixture
    def tracer(self):
        return GraphQLTracer(TracingConfig())

    def test_detects_query_operation(self, tracer):
        assert tracer._detect_operation_type("query { users }") == "query"
        assert tracer._detect_operation_type("{ users }") == "query"

    def test_detects_mutation_operation(self, tracer):
        assert tracer._detect_operation_type("mutation { createUser }") == "mutation"

    def test_detects_subscription_operation(self, tracer):
        result = tracer._detect_operation_type("subscription { onUserCreated }")
        assert result == "subscription"

    def test_truncates_long_queries(self, tracer):
        long_query = "query { " + "x" * 2000 + " }"
        truncated = tracer._truncate_query(long_query)
        assert len(truncated) <= tracer._config.max_query_length + 3
```

##### GREEN Phase

Create `src/fraiseql/tracing/graphql_tracing.py`:

Key implementation details:
- Gracefully handle missing OpenTelemetry (return undecorated functions)
- `TracingConfig` dataclass with:
  - `trace_resolvers: bool = True`
  - `trace_data_access: bool = True`
  - `include_variables: bool = False` (security default)
  - `sanitize_variables: bool = True` (redact sensitive fields when include_variables=True)
  - `sanitize_patterns: list[str]` (default: ["password", "token", "secret", "key", "auth", "credential", "api_key", "apikey"])
  - `max_query_length: int = 1000`
- `GraphQLTracer` class with:
  - `_detect_operation_type()` - parse query/mutation/subscription
  - `_truncate_query()` - limit query length in spans
  - `_sanitize_variables()` - redact sensitive variable values
  - `trace_query()` decorator for query execution
  - `trace_resolver()` decorator for resolver execution

Span attributes to set:
- `graphql.operation.type` (query/mutation/subscription)
- `graphql.operation.name` (if provided)
- `graphql.document` (truncated query)
- `graphql.field.name` (for resolver spans)
- `graphql.field.parent_type` (for resolver spans)

##### QA Phase

```bash
uv run pytest tests/unit/tracing/test_graphql_tracing.py -v
```

---

### Task 3.2: Implement Security Profiles

**Complexity:** Medium

#### Context

Security profiles provide pre-configured security settings for different deployment scenarios:
- **Standard**: Development and general production use
- **Regulated**: Industries with compliance requirements (healthcare, finance)
- **Restricted**: High-security environments

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/profiles/test_security_profiles.py`:

```python
"""Tests for security profiles."""

import pytest

from fraiseql.security.profiles import (
    SecurityProfile,
    SecurityProfileConfig,
    AuditLevel,
    ErrorDetailLevel,
    IntrospectionPolicy,
    get_profile,
    STANDARD_PROFILE,
    REGULATED_PROFILE,
    RESTRICTED_PROFILE,
)


class TestSecurityProfile:
    """Tests for SecurityProfile enum."""

    def test_standard_profile_exists(self):
        assert SecurityProfile.STANDARD.value == "standard"

    def test_regulated_profile_exists(self):
        assert SecurityProfile.REGULATED.value == "regulated"

    def test_restricted_profile_exists(self):
        assert SecurityProfile.RESTRICTED.value == "restricted"


class TestSecurityProfileConfig:
    """Tests for SecurityProfileConfig."""

    def test_standard_profile_allows_introspection(self):
        assert STANDARD_PROFILE.introspection_policy == IntrospectionPolicy.AUTHENTICATED

    def test_regulated_profile_disables_introspection(self):
        assert REGULATED_PROFILE.introspection_policy == IntrospectionPolicy.DISABLED

    def test_restricted_profile_requires_mtls(self):
        assert RESTRICTED_PROFILE.mtls_required is True

    def test_restricted_profile_smaller_body_size(self):
        assert RESTRICTED_PROFILE.max_body_size == 524_288  # 512KB

    def test_to_dict_serialization(self):
        result = STANDARD_PROFILE.to_dict()
        assert result["profile"] == "standard"
        assert "max_body_size" in result


class TestGetProfile:
    """Tests for get_profile function."""

    def test_get_by_string(self):
        profile = get_profile("standard")
        assert profile.profile == SecurityProfile.STANDARD

    def test_get_by_enum(self):
        profile = get_profile(SecurityProfile.REGULATED)
        assert profile.profile == SecurityProfile.REGULATED

    def test_invalid_profile_raises(self):
        with pytest.raises(ValueError):
            get_profile("invalid")
```

##### GREEN Phase

Create `src/fraiseql/security/profiles/definitions.py`:

Define the following:

**Enums:**
- `SecurityProfile`: STANDARD, REGULATED, RESTRICTED
- `AuditLevel`: MINIMAL, STANDARD, ENHANCED, VERBOSE
- `ErrorDetailLevel`: FULL, SAFE, MINIMAL
- `IntrospectionPolicy`: ENABLED, AUTHENTICATED, DISABLED

**SecurityProfileConfig dataclass:**
```python
@dataclass
class SecurityProfileConfig:
    profile: SecurityProfile

    # TLS
    tls_required: bool = False
    mtls_required: bool = False
    min_tls_version: str = "1.2"

    # Authentication
    auth_required: bool = True
    token_expiry_minutes: int = 60

    # GraphQL Security
    introspection_policy: IntrospectionPolicy = IntrospectionPolicy.AUTHENTICATED
    max_query_depth: int = 10
    max_query_complexity: int = 1000
    max_body_size: int = 1_048_576  # 1MB

    # Rate Limiting
    rate_limit_enabled: bool = True
    rate_limit_requests_per_minute: int = 100

    # Audit
    audit_level: AuditLevel = AuditLevel.STANDARD
    audit_field_access: bool = False

    # Error Handling
    error_detail_level: ErrorDetailLevel = ErrorDetailLevel.SAFE
```

**Pre-defined profiles:**

| Setting | STANDARD | REGULATED | RESTRICTED |
|---------|----------|-----------|------------|
| tls_required | False | True | True |
| mtls_required | False | False | True |
| token_expiry_minutes | 60 | 15 | 5 |
| introspection_policy | AUTHENTICATED | DISABLED | DISABLED |
| max_query_depth | 15 | 10 | 5 |
| max_body_size | 1MB | 1MB | 512KB |
| audit_level | STANDARD | ENHANCED | VERBOSE |
| error_detail_level | SAFE | SAFE | MINIMAL |

Create `src/fraiseql/security/profiles/__init__.py` to export all components.

##### QA Phase

```bash
uv run pytest tests/unit/security/profiles/ -v
```

---

### Task 3.3: Implement Security Profile Enforcement

**Complexity:** Medium

#### Context

Security profiles define configurations, but without enforcement they're just documentation. This task adds `ProfileEnforcer` which wires profile settings into actual middleware and validators.

#### TDD Cycle

##### RED Phase

Add to `tests/unit/security/profiles/test_security_profiles.py`:

```python
class TestProfileEnforcer:
    """Tests for ProfileEnforcer application service."""

    def test_creates_body_size_middleware_config(self):
        """Should create BodySizeConfig from profile."""
        enforcer = ProfileEnforcer(RESTRICTED_PROFILE)
        config = enforcer.get_body_size_config()

        assert config.max_body_size == 524_288  # 512KB for restricted

    def test_creates_rate_limit_config(self):
        """Should create RateLimitConfig from profile."""
        enforcer = ProfileEnforcer(STANDARD_PROFILE)
        config = enforcer.get_rate_limit_config()

        assert config.requests_per_minute == 100

    def test_creates_query_validator_config(self):
        """Should create QueryValidatorConfig from profile."""
        enforcer = ProfileEnforcer(REGULATED_PROFILE)
        config = enforcer.get_query_validator_config()

        assert config.max_depth == 10
        assert config.max_complexity == 1000

    def test_get_fastapi_middleware_stack(self):
        """Should return list of configured middleware tuples."""
        enforcer = ProfileEnforcer(STANDARD_PROFILE)
        middleware = enforcer.get_middleware_stack()

        # Returns list of (MiddlewareClass, kwargs) tuples
        assert len(middleware) >= 2  # At minimum: body size + rate limit

    def test_validates_request_context(self):
        """Should validate request meets profile requirements."""
        enforcer = ProfileEnforcer(RESTRICTED_PROFILE)

        # Restricted requires mTLS
        context = {"mtls_verified": False, "tls_version": "1.3"}
        errors = enforcer.validate_request_context(context)

        assert any("mTLS" in e for e in errors)

    def test_apply_to_app_adds_middleware(self):
        """Should add all profile middleware to FastAPI app."""
        from fastapi import FastAPI

        app = FastAPI()
        enforcer = ProfileEnforcer(STANDARD_PROFILE)

        enforcer.apply_to_app(app)

        # Verify middleware was added
        assert len(app.user_middleware) > 0
```

##### GREEN Phase

Create `src/fraiseql/security/profiles/enforcer.py`:

```python
"""Security profile enforcement.

Wires SecurityProfileConfig into actual middleware and validators.
"""

from dataclasses import dataclass
from typing import Any

from fraiseql.middleware.body_size_limiter import (
    BodySizeConfig,
    BodySizeLimiterMiddleware,
)
from fraiseql.middleware.rate_limiter import RateLimitConfig, RateLimiterMiddleware

from .definitions import (
    SecurityProfileConfig,
    IntrospectionPolicy,
    ErrorDetailLevel,
)


@dataclass
class QueryValidatorConfig:
    """Configuration for GraphQL query validation."""

    max_depth: int
    max_complexity: int
    introspection_allowed: bool


class ProfileEnforcer:
    """Applies security profile settings to application components.

    This is the bridge between SecurityProfileConfig (declarative) and
    actual middleware/validators (operational). It ensures profile
    settings are consistently enforced across the application.

    Usage:
        from fraiseql.security.profiles import get_profile, ProfileEnforcer

        profile = get_profile("regulated")
        enforcer = ProfileEnforcer(profile)

        # Option 1: Apply all middleware at once
        enforcer.apply_to_app(app)

        # Option 2: Get individual configs for manual setup
        body_config = enforcer.get_body_size_config()
        rate_config = enforcer.get_rate_limit_config()
    """

    def __init__(self, profile: SecurityProfileConfig) -> None:
        self._profile = profile

    @property
    def profile(self) -> SecurityProfileConfig:
        """Get the underlying profile configuration."""
        return self._profile

    def get_body_size_config(self) -> BodySizeConfig:
        """Create BodySizeConfig from profile settings."""
        return BodySizeConfig(
            max_body_size=self._profile.max_body_size,
        )

    def get_rate_limit_config(self) -> RateLimitConfig:
        """Create RateLimitConfig from profile settings."""
        return RateLimitConfig(
            requests_per_minute=self._profile.rate_limit_requests_per_minute,
            enabled=self._profile.rate_limit_enabled,
        )

    def get_query_validator_config(self) -> QueryValidatorConfig:
        """Create query validator configuration from profile."""
        return QueryValidatorConfig(
            max_depth=self._profile.max_query_depth,
            max_complexity=self._profile.max_query_complexity,
            introspection_allowed=(
                self._profile.introspection_policy != IntrospectionPolicy.DISABLED
            ),
        )

    def get_middleware_stack(self) -> list[tuple[type, dict[str, Any]]]:
        """Get list of middleware classes and their configurations.

        Returns:
            List of (MiddlewareClass, kwargs) tuples ready for app.add_middleware()
        """
        stack: list[tuple[type, dict[str, Any]]] = []

        # Body size limiter (always added)
        stack.append((
            BodySizeLimiterMiddleware,
            {"config": self.get_body_size_config()},
        ))

        # Rate limiter (if enabled)
        if self._profile.rate_limit_enabled:
            stack.append((
                RateLimiterMiddleware,
                {"config": self.get_rate_limit_config()},
            ))

        return stack

    def validate_request_context(
        self,
        context: dict[str, Any],
    ) -> list[str]:
        """Validate that request context meets profile requirements.

        Args:
            context: Dict with keys like 'mtls_verified', 'tls_version',
                    'authenticated', etc.

        Returns:
            List of validation error messages (empty if valid)
        """
        errors: list[str] = []

        # Check mTLS requirement
        if self._profile.mtls_required:
            if not context.get("mtls_verified", False):
                errors.append(
                    f"Profile '{self._profile.profile.value}' requires mTLS client certificate"
                )

        # Check TLS version
        if self._profile.tls_required:
            tls_version = context.get("tls_version", "")
            min_version = self._profile.min_tls_version
            if not self._is_tls_version_acceptable(tls_version, min_version):
                errors.append(
                    f"Profile requires TLS {min_version}+, got {tls_version or 'none'}"
                )

        # Check authentication
        if self._profile.auth_required:
            if not context.get("authenticated", False):
                errors.append(
                    f"Profile '{self._profile.profile.value}' requires authentication"
                )

        return errors

    def apply_to_app(self, app: Any) -> None:
        """Apply all profile middleware to a FastAPI/Starlette app.

        Args:
            app: FastAPI or Starlette application instance
        """
        for middleware_class, kwargs in self.get_middleware_stack():
            app.add_middleware(middleware_class, **kwargs)

    def _is_tls_version_acceptable(
        self,
        actual: str,
        minimum: str,
    ) -> bool:
        """Check if TLS version meets minimum requirement."""
        version_order = ["1.0", "1.1", "1.2", "1.3"]
        try:
            actual_idx = version_order.index(actual)
            min_idx = version_order.index(minimum)
            return actual_idx >= min_idx
        except ValueError:
            return False
```

Update `src/fraiseql/security/profiles/__init__.py`:

```python
"""Security profiles for FraiseQL."""

from .definitions import (
    SecurityProfile,
    SecurityProfileConfig,
    AuditLevel,
    ErrorDetailLevel,
    IntrospectionPolicy,
    get_profile,
    STANDARD_PROFILE,
    REGULATED_PROFILE,
    RESTRICTED_PROFILE,
)
from .enforcer import ProfileEnforcer, QueryValidatorConfig

__all__ = [
    # Enums
    "SecurityProfile",
    "AuditLevel",
    "ErrorDetailLevel",
    "IntrospectionPolicy",
    # Configs
    "SecurityProfileConfig",
    "QueryValidatorConfig",
    # Pre-defined profiles
    "STANDARD_PROFILE",
    "REGULATED_PROFILE",
    "RESTRICTED_PROFILE",
    # Functions
    "get_profile",
    # Enforcer
    "ProfileEnforcer",
]
```

##### QA Phase

```bash
uv run pytest tests/unit/security/profiles/ -v
```

**Acceptance Criteria:**
- [ ] ProfileEnforcer creates correct middleware configs
- [ ] ProfileEnforcer validates request context
- [ ] ProfileEnforcer can apply middleware to FastAPI app
- [ ] All profile settings are enforced, not just documented

---

### Phase 3 Checkpoint

```bash
uv run pytest tests/unit/tracing/ tests/unit/security/profiles/ -v
uv run pytest --tb=short
uv run ruff check src/
uv run mypy src/
```

**Phase 3 Completion Checklist:**
- [ ] GraphQL tracing enhanced
- [ ] Security profiles implemented
- [ ] ProfileEnforcer implemented (bridges config to middleware)
- [ ] All enums and configurations tested

---

## Phase 4: Rust Pipeline Compatibility & GCP Provider

**Theme:** "Performance-First Key Management"

### Architectural Decision Record

#### Context

FraiseQL uses a high-performance Rust pipeline (`fraiseql_rs`) for JSON transformation:
- Zero-copy streaming from PostgreSQL → HTTP response
- SIMD-optimized key transformation (snake_case → camelCase)
- Current latency: **~6-17ms per request**

Adding KMS calls in the request path would:
- Add 50-200ms per request (network latency to KMS)
- Increase latency by **10-15x**
- Break the zero-copy optimization

#### Decision

**KMS is for key management and secrets storage, NOT per-request encryption.**

```
┌─────────────────────────────────────────────────────────────────┐
│ KMS USAGE MODEL                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ✅ CORRECT: Key Rotation & Secrets Management                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ STARTUP / SCHEDULED (infrequent)                        │   │
│  │                                                         │   │
│  │  • Generate/rotate data encryption keys via KMS         │   │
│  │  • Decrypt application secrets (API keys, tokens)       │   │
│  │  • Store data keys in process memory                    │   │
│  │  • Rotate keys every N hours (configurable)             │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ❌ WRONG: Per-Request Encryption                               │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ HOT PATH (every request) - DO NOT DO THIS               │   │
│  │                                                         │   │
│  │  • Call KMS to encrypt/decrypt response data            │   │
│  │  • Would add 50-200ms latency per request               │   │
│  │  • Would break Rust zero-copy optimization              │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

#### Encryption Strategy

| Layer | Mechanism | KMS Role |
|-------|-----------|----------|
| **Data at Rest** | PostgreSQL native encryption (pgcrypto) | Key rotation only |
| **Data in Transit** | HTTPS/TLS | None (handled by TLS) |
| **Application Secrets** | Envelope encryption | Decrypt at startup |
| **API Keys/Tokens** | Column-level encryption | Decrypt on access |

---

### Goals

1. Refactor KeyManager to support startup-time key loading
2. Add GCP Cloud KMS provider
3. Add AWS KMS provider
4. Add local development provider (for testing without cloud KMS)
5. Document Rust pipeline compatibility

---

### Task 4.1: Refactor KeyManager for Startup-Time Usage

**Complexity:** Medium

#### Context

The current KeyManager is designed for per-operation encryption. We need to add support for:
1. Startup-time data key generation
2. In-memory key caching
3. Scheduled key rotation

#### TDD Cycle

##### RED Phase

Add to `tests/unit/security/kms/test_key_manager.py`:

```python
class TestKeyManagerStartup:
    """Tests for startup-time key management."""

    @pytest.fixture
    def vault_provider(self):
        return MockProvider("vault")

    @pytest.mark.asyncio
    async def test_initialize_data_key(self, vault_provider):
        """Should generate and cache data key at startup."""
        manager = KeyManager(
            providers={"vault": vault_provider},
            default_provider="vault",
            default_key_id="master-key",
        )

        await manager.initialize()

        assert manager.has_cached_data_key()
        assert manager.get_cached_data_key() is not None

    @pytest.mark.asyncio
    async def test_local_encrypt_uses_cached_key(self, vault_provider):
        """Should use cached data key for local encryption (no KMS call)."""
        manager = KeyManager(
            providers={"vault": vault_provider},
            default_provider="vault",
            default_key_id="master-key",
        )
        await manager.initialize()

        # This should NOT call KMS - uses cached key
        encrypted = manager.local_encrypt(b"sensitive data")

        assert encrypted is not None
        assert vault_provider.encrypt_call_count == 0  # No KMS call

    @pytest.mark.asyncio
    async def test_local_decrypt_uses_cached_key(self, vault_provider):
        """Should use cached data key for local decryption."""
        manager = KeyManager(
            providers={"vault": vault_provider},
            default_provider="vault",
            default_key_id="master-key",
        )
        await manager.initialize()

        encrypted = manager.local_encrypt(b"sensitive data")
        decrypted = manager.local_decrypt(encrypted)

        assert decrypted == b"sensitive data"

    @pytest.mark.asyncio
    async def test_rotate_data_key(self, vault_provider):
        """Should rotate data key via KMS."""
        manager = KeyManager(
            providers={"vault": vault_provider},
            default_provider="vault",
            default_key_id="master-key",
        )
        await manager.initialize()
        old_key = manager.get_cached_data_key()

        await manager.rotate_data_key()

        new_key = manager.get_cached_data_key()
        assert new_key != old_key

    def test_local_encrypt_fails_without_initialization(self, vault_provider):
        """Should raise if local_encrypt called before initialize()."""
        manager = KeyManager(
            providers={"vault": vault_provider},
            default_provider="vault",
            default_key_id="master-key",
        )

        with pytest.raises(RuntimeError, match="not initialized"):
            manager.local_encrypt(b"data")
```

##### GREEN Phase

Update `src/fraiseql/security/kms/application/key_manager.py`:

```python
"""KeyManager application service.

Unified interface for encryption operations across multiple KMS providers.

ARCHITECTURE NOTE:
------------------
This KeyManager is designed for TWO usage patterns:

1. STARTUP-TIME (recommended for performance):
   - Call initialize() at application startup
   - Generates a data encryption key (DEK) via KMS
   - Caches DEK in memory for fast local encryption
   - Use local_encrypt() / local_decrypt() in hot paths
   - Rotate keys periodically via rotate_data_key()

2. PER-REQUEST (for infrequent high-security operations):
   - Call encrypt() / decrypt() directly
   - Each call contacts KMS (50-200ms latency)
   - Use only for secrets management, not response data

The Rust pipeline should NEVER wait on KMS calls. Use local_encrypt()
with cached keys for any encryption in the request path.
"""

import secrets
from cryptography.fernet import Fernet
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.models import (
    DataKeyPair,
    EncryptedData,
)


class KeyManager:
    """Application service for encryption operations.

    Provides both KMS-backed encryption (slow, secure) and
    local encryption with cached keys (fast, for hot paths).
    """

    def __init__(
        self,
        providers: dict[str, BaseKMSProvider],
        default_provider: str,
        default_key_id: str,
        context_prefix: str | None = None,
    ) -> None:
        self._providers = providers
        self._default_provider = default_provider
        self._default_key_id = default_key_id
        self._context_prefix = context_prefix

        # Cached data key for local encryption (set by initialize())
        self._data_key_pair: DataKeyPair | None = None
        self._aesgcm: AESGCM | None = None

        if default_provider not in providers:
            raise ValueError(f"Default provider '{default_provider}' not in providers")

    # ─────────────────────────────────────────────────────────────
    # Startup-Time Key Management (recommended)
    # ─────────────────────────────────────────────────────────────

    async def initialize(self) -> None:
        """Initialize KeyManager by generating a cached data key.

        Call this at application startup. The data key is generated
        via KMS and cached in memory for fast local encryption.
        """
        provider = self.get_provider(self._default_provider)
        self._data_key_pair = await provider.generate_data_key(
            self._default_key_id,
            context=self._build_context({"purpose": "data_encryption"}),
        )
        self._aesgcm = AESGCM(self._data_key_pair.plaintext_key)

    def has_cached_data_key(self) -> bool:
        """Check if a data key is cached."""
        return self._data_key_pair is not None

    def get_cached_data_key(self) -> bytes | None:
        """Get the cached plaintext data key (for debugging only)."""
        if self._data_key_pair is None:
            return None
        return self._data_key_pair.plaintext_key

    async def rotate_data_key(self) -> None:
        """Rotate the cached data key via KMS.

        Call this periodically (e.g., every few hours) to rotate keys.
        """
        await self.initialize()

    def local_encrypt(self, plaintext: bytes) -> bytes:
        """Encrypt data using the cached data key (NO KMS call).

        This is fast (~microseconds) and safe for use in hot paths.

        Args:
            plaintext: Data to encrypt

        Returns:
            Encrypted bytes (nonce + ciphertext)

        Raises:
            RuntimeError: If initialize() was not called
        """
        if self._aesgcm is None:
            raise RuntimeError(
                "KeyManager not initialized. Call initialize() at startup."
            )

        nonce = secrets.token_bytes(12)  # 96-bit nonce for AES-GCM
        ciphertext = self._aesgcm.encrypt(nonce, plaintext, None)
        return nonce + ciphertext

    def local_decrypt(self, encrypted: bytes) -> bytes:
        """Decrypt data using the cached data key (NO KMS call).

        Args:
            encrypted: Encrypted bytes (nonce + ciphertext)

        Returns:
            Decrypted plaintext

        Raises:
            RuntimeError: If initialize() was not called
        """
        if self._aesgcm is None:
            raise RuntimeError(
                "KeyManager not initialized. Call initialize() at startup."
            )

        nonce = encrypted[:12]
        ciphertext = encrypted[12:]
        return self._aesgcm.decrypt(nonce, ciphertext, None)

    # ─────────────────────────────────────────────────────────────
    # Per-Request KMS Operations (slow, use sparingly)
    # ─────────────────────────────────────────────────────────────

    # ... existing encrypt(), decrypt(), encrypt_field(), decrypt_field() ...
```

Add dependency to `pyproject.toml`:

```toml
kms = [
    "cryptography>=42.0.0",  # For local AES-GCM encryption
    "httpx>=0.27.0",         # For Vault provider
]
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_key_manager.py -v
```

---

### Task 4.2: Implement GCP Cloud KMS Provider

**Complexity:** Medium

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_gcp_kms_provider.py`:

```python
"""Tests for GCP Cloud KMS provider."""

import pytest
from unittest.mock import AsyncMock, patch, MagicMock

from fraiseql.security.kms.infrastructure.gcp_kms import (
    GCPKMSProvider,
    GCPKMSConfig,
)
from fraiseql.security.kms.domain.base import BaseKMSProvider


class TestGCPKMSConfig:
    def test_key_path_construction(self):
        config = GCPKMSConfig(
            project_id="my-project",
            location="global",
            key_ring="my-keyring",
        )
        expected = "projects/my-project/locations/global/keyRings/my-keyring/cryptoKeys/my-key"
        assert config.key_path("my-key") == expected

    def test_key_version_path(self):
        config = GCPKMSConfig(
            project_id="my-project",
            location="us-east1",
            key_ring="prod-keys",
        )
        expected = "projects/my-project/locations/us-east1/keyRings/prod-keys/cryptoKeys/api-key/cryptoKeyVersions/1"
        assert config.key_version_path("api-key", "1") == expected


class TestGCPKMSProvider:
    @pytest.fixture
    def config(self):
        return GCPKMSConfig(
            project_id="my-project",
            location="global",
            key_ring="my-keyring",
        )

    def test_extends_base_provider(self, config):
        with patch("fraiseql.security.kms.infrastructure.gcp_kms.kms_v1"):
            provider = GCPKMSProvider(config)
            assert isinstance(provider, BaseKMSProvider)

    def test_provider_name(self, config):
        with patch("fraiseql.security.kms.infrastructure.gcp_kms.kms_v1"):
            provider = GCPKMSProvider(config)
            assert provider.provider_name == "gcp"

    @pytest.mark.asyncio
    async def test_do_encrypt_calls_gcp_api(self, config):
        """_do_encrypt should call GCP Cloud KMS encrypt endpoint."""
        with patch("fraiseql.security.kms.infrastructure.gcp_kms.kms_v1") as mock_kms:
            mock_client = AsyncMock()
            mock_kms.KeyManagementServiceAsyncClient.return_value = mock_client

            mock_response = MagicMock()
            mock_response.ciphertext = b"encrypted-data"
            mock_client.encrypt.return_value = mock_response

            provider = GCPKMSProvider(config)
            ciphertext, algo = await provider._do_encrypt(
                b"plaintext",
                "my-key",
                {"purpose": "test"},
            )

            mock_client.encrypt.assert_called_once()
            assert ciphertext == b"encrypted-data"
            assert algo == "GOOGLE_SYMMETRIC_ENCRYPTION"

    @pytest.mark.asyncio
    async def test_do_decrypt_calls_gcp_api(self, config):
        """_do_decrypt should call GCP Cloud KMS decrypt endpoint."""
        with patch("fraiseql.security.kms.infrastructure.gcp_kms.kms_v1") as mock_kms:
            mock_client = AsyncMock()
            mock_kms.KeyManagementServiceAsyncClient.return_value = mock_client

            mock_response = MagicMock()
            mock_response.plaintext = b"decrypted-data"
            mock_client.decrypt.return_value = mock_response

            provider = GCPKMSProvider(config)
            result = await provider._do_decrypt(
                b"ciphertext",
                "my-key",
                {},
            )

            assert result == b"decrypted-data"
```

##### GREEN Phase

Create `src/fraiseql/security/kms/infrastructure/gcp_kms.py`:

```python
"""GCP Cloud KMS provider."""

from dataclasses import dataclass
import json
import secrets

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import KeyNotFoundError

try:
    from google.cloud import kms_v1
    from google.cloud.kms_v1 import types
    from google.api_core import exceptions as gcp_exceptions
    GCP_KMS_AVAILABLE = True
except ImportError:
    GCP_KMS_AVAILABLE = False
    kms_v1 = None  # type: ignore
    types = None  # type: ignore
    gcp_exceptions = None  # type: ignore


@dataclass
class GCPKMSConfig:
    """Configuration for GCP Cloud KMS provider."""

    project_id: str
    location: str
    key_ring: str

    def key_path(self, key_id: str) -> str:
        """Build full key resource path."""
        return (
            f"projects/{self.project_id}/"
            f"locations/{self.location}/"
            f"keyRings/{self.key_ring}/"
            f"cryptoKeys/{key_id}"
        )

    def key_version_path(self, key_id: str, version: str) -> str:
        """Build key version resource path."""
        return f"{self.key_path(key_id)}/cryptoKeyVersions/{version}"


class GCPKMSProvider(BaseKMSProvider):
    """GCP Cloud KMS implementation.

    Extends BaseKMSProvider - only implements the _do_* hooks.

    SECURITY CONSIDERATION - Data Key Generation:
    ---------------------------------------------
    Unlike AWS KMS (GenerateDataKey) and Vault Transit (datakey/plaintext),
    GCP Cloud KMS does not provide a native data key generation API.

    This provider implements envelope encryption by:
    1. Generating a random 32-byte key LOCALLY using secrets.token_bytes()
    2. Encrypting that key with the GCP master key
    3. Returning both the plaintext and encrypted key

    Security Implications:
    - The plaintext key briefly exists in local memory before encryption
    - AWS/Vault generate the key server-side, so plaintext never leaves KMS
    - For GCP, ensure the machine running this code is trusted
    - Consider using GCP's ImportJob for importing pre-generated keys
      if your threat model requires server-side key generation

    The local generation uses Python's `secrets` module which provides
    cryptographically secure random bytes suitable for key material.
    """

    def __init__(self, config: GCPKMSConfig) -> None:
        if not GCP_KMS_AVAILABLE:
            raise ImportError(
                "google-cloud-kms required: pip install 'fraiseql[kms-gcp]'"
            )
        self._config = config
        self._client: kms_v1.KeyManagementServiceAsyncClient | None = None

    async def _get_client(self) -> kms_v1.KeyManagementServiceAsyncClient:
        """Lazy initialization of async client."""
        if self._client is None:
            self._client = kms_v1.KeyManagementServiceAsyncClient()
        return self._client

    @property
    def provider_name(self) -> str:
        return "gcp"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Encrypt using GCP Cloud KMS."""
        client = await self._get_client()
        aad = json.dumps(context, sort_keys=True).encode() if context else None

        request = types.EncryptRequest(
            name=self._config.key_path(key_id),
            plaintext=plaintext,
            additional_authenticated_data=aad,
        )

        try:
            response = await client.encrypt(request=request)
            return response.ciphertext, "GOOGLE_SYMMETRIC_ENCRYPTION"
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt using GCP Cloud KMS."""
        client = await self._get_client()
        aad = json.dumps(context, sort_keys=True).encode() if context else None

        request = types.DecryptRequest(
            name=self._config.key_path(key_id),
            ciphertext=ciphertext,
            additional_authenticated_data=aad,
        )

        try:
            response = await client.decrypt(request=request)
            return response.plaintext
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Manual envelope encryption (GCP doesn't have native data key gen).

        Generates a 32-byte AES-256 key locally and encrypts it with the
        master key. See class docstring for security considerations.
        """
        plaintext_key = secrets.token_bytes(32)  # AES-256
        ciphertext, _ = await self._do_encrypt(plaintext_key, key_id, context)
        return plaintext_key, ciphertext

    async def _do_rotate_key(self, key_id: str) -> None:
        """Create new key version in GCP Cloud KMS."""
        client = await self._get_client()
        request = types.CreateCryptoKeyVersionRequest(
            parent=self._config.key_path(key_id)
        )
        await client.create_crypto_key_version(request=request)

    async def _do_get_key_info(self, key_id: str) -> dict:
        """Get key info from GCP Cloud KMS."""
        client = await self._get_client()
        request = types.GetCryptoKeyRequest(name=self._config.key_path(key_id))

        try:
            response = await client.get_crypto_key(request=request)
            return {
                "alias": response.name,
                "created_at": response.create_time,
            }
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Get rotation policy from GCP Cloud KMS."""
        client = await self._get_client()
        request = types.GetCryptoKeyRequest(name=self._config.key_path(key_id))

        try:
            response = await client.get_crypto_key(request=request)
            period = response.rotation_period
            enabled = period is not None and period.seconds > 0
            return {
                "enabled": enabled,
                "period_days": period.seconds // 86400 if enabled else 0,
                "last_rotation": (
                    response.primary.create_time if response.primary else None
                ),
                "next_rotation": response.next_rotation_time,
            }
        except gcp_exceptions.NotFound:
            raise KeyNotFoundError(f"Key not found: {key_id}")
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_gcp_kms_provider.py -v
```

---

### Task 4.3: Implement AWS KMS Provider

**Complexity:** Medium

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_aws_kms_provider.py`:

```python
"""Tests for AWS KMS provider."""

import pytest
from unittest.mock import AsyncMock, patch, MagicMock

from fraiseql.security.kms.infrastructure.aws_kms import (
    AWSKMSProvider,
    AWSKMSConfig,
)
from fraiseql.security.kms.domain.base import BaseKMSProvider


class TestAWSKMSConfig:
    def test_requires_region(self):
        config = AWSKMSConfig(region="us-east-1")
        assert config.region == "us-east-1"

    def test_endpoint_url_for_localstack(self):
        config = AWSKMSConfig(
            region="us-east-1",
            endpoint_url="http://localhost:4566",
        )
        assert config.endpoint_url == "http://localhost:4566"


class TestAWSKMSProvider:
    @pytest.fixture
    def config(self):
        return AWSKMSConfig(region="us-east-1")

    def test_extends_base_provider(self, config):
        with patch("fraiseql.security.kms.infrastructure.aws_kms.aioboto3"):
            provider = AWSKMSProvider(config)
            assert isinstance(provider, BaseKMSProvider)

    def test_provider_name(self, config):
        with patch("fraiseql.security.kms.infrastructure.aws_kms.aioboto3"):
            provider = AWSKMSProvider(config)
            assert provider.provider_name == "aws"

    @pytest.mark.asyncio
    async def test_do_encrypt_calls_aws_api(self, config):
        """_do_encrypt should call AWS KMS encrypt endpoint."""
        with patch("fraiseql.security.kms.infrastructure.aws_kms.aioboto3") as mock_boto:
            mock_session = MagicMock()
            mock_boto.Session.return_value = mock_session

            mock_client = AsyncMock()
            mock_client.encrypt.return_value = {
                "CiphertextBlob": b"encrypted-data",
                "EncryptionAlgorithm": "SYMMETRIC_DEFAULT",
            }

            mock_cm = MagicMock()
            mock_cm.__aenter__ = AsyncMock(return_value=mock_client)
            mock_cm.__aexit__ = AsyncMock(return_value=None)
            mock_session.client.return_value = mock_cm

            provider = AWSKMSProvider(config)
            ciphertext, algo = await provider._do_encrypt(
                b"plaintext",
                "alias/my-key",
                {"purpose": "test"},
            )

            assert ciphertext == b"encrypted-data"
            assert algo == "SYMMETRIC_DEFAULT"

    @pytest.mark.asyncio
    async def test_do_decrypt_includes_key_id(self, config):
        """_do_decrypt should include KeyId for security."""
        with patch("fraiseql.security.kms.infrastructure.aws_kms.aioboto3") as mock_boto:
            mock_session = MagicMock()
            mock_boto.Session.return_value = mock_session

            mock_client = AsyncMock()
            mock_client.decrypt.return_value = {"Plaintext": b"decrypted"}

            mock_cm = MagicMock()
            mock_cm.__aenter__ = AsyncMock(return_value=mock_client)
            mock_cm.__aexit__ = AsyncMock(return_value=None)
            mock_session.client.return_value = mock_cm

            provider = AWSKMSProvider(config)
            await provider._do_decrypt(b"ciphertext", "alias/my-key", {})

            # Verify KeyId was included in the call
            call_kwargs = mock_client.decrypt.call_args[1]
            assert "KeyId" in call_kwargs
            assert call_kwargs["KeyId"] == "alias/my-key"
```

##### GREEN Phase

Create `src/fraiseql/security/kms/infrastructure/aws_kms.py`:

```python
"""AWS KMS provider."""

from dataclasses import dataclass
from contextlib import asynccontextmanager
from typing import AsyncIterator, Any

from fraiseql.security.kms.domain.base import BaseKMSProvider
from fraiseql.security.kms.domain.exceptions import KeyNotFoundError

try:
    import aioboto3
    from botocore.exceptions import ClientError
    AIOBOTO3_AVAILABLE = True
except ImportError:
    AIOBOTO3_AVAILABLE = False
    aioboto3 = None  # type: ignore
    ClientError = Exception  # type: ignore


@dataclass
class AWSKMSConfig:
    """Configuration for AWS KMS provider."""

    region: str
    endpoint_url: str | None = None  # For LocalStack testing
    aws_access_key_id: str | None = None
    aws_secret_access_key: str | None = None


class AWSKMSProvider(BaseKMSProvider):
    """AWS KMS implementation.

    Extends BaseKMSProvider - only implements the _do_* hooks.
    """

    def __init__(self, config: AWSKMSConfig) -> None:
        if not AIOBOTO3_AVAILABLE:
            raise ImportError(
                "aioboto3 required: pip install 'fraiseql[kms-aws]'"
            )
        self._config = config
        self._session = aioboto3.Session(
            aws_access_key_id=config.aws_access_key_id,
            aws_secret_access_key=config.aws_secret_access_key,
            region_name=config.region,
        )

    @property
    def provider_name(self) -> str:
        return "aws"

    @asynccontextmanager
    async def _get_client(self) -> AsyncIterator[Any]:
        async with self._session.client(
            "kms", endpoint_url=self._config.endpoint_url
        ) as client:
            yield client

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Encrypt using AWS KMS."""
        async with self._get_client() as client:
            params: dict[str, Any] = {"KeyId": key_id, "Plaintext": plaintext}
            if context:
                params["EncryptionContext"] = context

            try:
                response = await client.encrypt(**params)
                return (
                    response["CiphertextBlob"],
                    response.get("EncryptionAlgorithm", "SYMMETRIC_DEFAULT"),
                )
            except ClientError as e:
                if e.response["Error"]["Code"] == "NotFoundException":
                    raise KeyNotFoundError(f"Key not found: {key_id}")
                raise

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt ciphertext using AWS KMS.

        SECURITY: We explicitly include KeyId even though AWS KMS doesn't
        require it for symmetric keys. This provides:
        1. Validation that the expected key was used
        2. Prevention of cross-account decryption via grants
        3. Explicit key binding in audit logs
        """
        async with self._get_client() as client:
            params: dict[str, Any] = {
                "CiphertextBlob": ciphertext,
                "KeyId": key_id,  # SECURITY: Explicit key validation
            }
            if context:
                params["EncryptionContext"] = context

            try:
                response = await client.decrypt(**params)
                return response["Plaintext"]
            except ClientError as e:
                if e.response["Error"]["Code"] == "NotFoundException":
                    raise KeyNotFoundError(f"Key not found: {key_id}")
                raise

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Generate data key using AWS KMS GenerateDataKey API."""
        async with self._get_client() as client:
            params: dict[str, Any] = {"KeyId": key_id, "KeySpec": "AES_256"}
            if context:
                params["EncryptionContext"] = context

            response = await client.generate_data_key(**params)
            return response["Plaintext"], response["CiphertextBlob"]

    async def _do_rotate_key(self, key_id: str) -> None:
        """Enable automatic key rotation in AWS KMS."""
        async with self._get_client() as client:
            await client.enable_key_rotation(KeyId=key_id)

    async def _do_get_key_info(self, key_id: str) -> dict:
        """Get key info from AWS KMS."""
        async with self._get_client() as client:
            try:
                response = await client.describe_key(KeyId=key_id)
                metadata = response["KeyMetadata"]
                return {
                    "alias": metadata.get("KeyId"),
                    "created_at": metadata["CreationDate"],
                }
            except ClientError as e:
                if e.response["Error"]["Code"] == "NotFoundException":
                    raise KeyNotFoundError(f"Key not found: {key_id}")
                raise

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Get key rotation policy from AWS KMS."""
        async with self._get_client() as client:
            response = await client.get_key_rotation_status(KeyId=key_id)
            return {
                "enabled": response["KeyRotationEnabled"],
                "period_days": 365,  # AWS default
                "last_rotation": None,
                "next_rotation": response.get("NextRotationDate"),
            }
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_aws_kms_provider.py -v
```

---

### Task 4.4: Implement Local Development Provider

**Complexity:** Low

#### Context

For local development and testing, we need a provider that doesn't require cloud infrastructure.

#### TDD Cycle

##### RED Phase

Create `tests/unit/security/kms/test_local_provider.py`:

```python
"""Tests for local development KMS provider."""

import pytest

from fraiseql.security.kms.infrastructure.local import (
    LocalKMSProvider,
    LocalKMSConfig,
)
from fraiseql.security.kms.domain.base import BaseKMSProvider


class TestLocalKMSProvider:
    @pytest.fixture
    def provider(self):
        config = LocalKMSConfig(master_key=b"0" * 32)
        return LocalKMSProvider(config)

    def test_extends_base_provider(self, provider):
        assert isinstance(provider, BaseKMSProvider)

    def test_provider_name(self, provider):
        assert provider.provider_name == "local"

    @pytest.mark.asyncio
    async def test_encrypt_decrypt_roundtrip(self, provider):
        """Should encrypt and decrypt data correctly."""
        plaintext = b"sensitive data"

        encrypted = await provider.encrypt(plaintext, "test-key")
        decrypted = await provider.decrypt(encrypted)

        assert decrypted == plaintext

    @pytest.mark.asyncio
    async def test_generate_data_key(self, provider):
        """Should generate a valid data key pair."""
        pair = await provider.generate_data_key("test-key")

        assert len(pair.plaintext_key) == 32
        assert len(pair.encrypted_key.ciphertext) > 0

    def test_warns_about_production_use(self, capsys):
        """Should warn that this is for development only."""
        import warnings
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            LocalKMSProvider(LocalKMSConfig())
            assert any("development" in str(warning.message).lower() for warning in w)
```

##### GREEN Phase

Create `src/fraiseql/security/kms/infrastructure/local.py`:

```python
"""Local development KMS provider.

WARNING: This provider is for LOCAL DEVELOPMENT ONLY.
DO NOT use in production - it provides no real security.
"""

from dataclasses import dataclass, field
import secrets
import warnings
from datetime import datetime, UTC

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from fraiseql.security.kms.domain.base import BaseKMSProvider


@dataclass
class LocalKMSConfig:
    """Configuration for local development KMS.

    WARNING: For development/testing only. Not secure for production.
    """

    master_key: bytes = field(default_factory=lambda: secrets.token_bytes(32))


class LocalKMSProvider(BaseKMSProvider):
    """Local development KMS provider using in-memory keys.

    WARNING: This provider is for LOCAL DEVELOPMENT AND TESTING ONLY.
    It provides NO real security guarantees:
    - Keys are stored in memory (not HSM-protected)
    - No audit logging
    - No key rotation policies
    - No access controls

    Use VaultKMSProvider, AWSKMSProvider, or GCPKMSProvider for production.
    """

    def __init__(self, config: LocalKMSConfig | None = None) -> None:
        warnings.warn(
            "LocalKMSProvider is for development only. "
            "Use Vault/AWS/GCP providers in production.",
            UserWarning,
            stacklevel=2,
        )
        self._config = config or LocalKMSConfig()
        self._aesgcm = AESGCM(self._config.master_key)
        self._keys: dict[str, bytes] = {}

    @property
    def provider_name(self) -> str:
        return "local"

    async def _do_encrypt(
        self,
        plaintext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, str]:
        """Encrypt using local AES-GCM."""
        nonce = secrets.token_bytes(12)
        aad = str(context).encode() if context else None
        ciphertext = self._aesgcm.encrypt(nonce, plaintext, aad)
        return nonce + ciphertext, "AES-256-GCM"

    async def _do_decrypt(
        self,
        ciphertext: bytes,
        key_id: str,
        context: dict[str, str],
    ) -> bytes:
        """Decrypt using local AES-GCM."""
        nonce = ciphertext[:12]
        ct = ciphertext[12:]
        aad = str(context).encode() if context else None
        return self._aesgcm.decrypt(nonce, ct, aad)

    async def _do_generate_data_key(
        self,
        key_id: str,
        context: dict[str, str],
    ) -> tuple[bytes, bytes]:
        """Generate a local data key."""
        plaintext_key = secrets.token_bytes(32)
        encrypted_key, _ = await self._do_encrypt(plaintext_key, key_id, context)
        return plaintext_key, encrypted_key

    async def _do_rotate_key(self, key_id: str) -> None:
        """No-op for local provider."""
        pass

    async def _do_get_key_info(self, key_id: str) -> dict:
        """Return mock key info."""
        return {"alias": key_id, "created_at": datetime.now(UTC)}

    async def _do_get_rotation_policy(self, key_id: str) -> dict:
        """Return mock rotation policy."""
        return {
            "enabled": False,
            "period_days": 0,
            "last_rotation": None,
            "next_rotation": None,
        }
```

##### QA Phase

```bash
uv run pytest tests/unit/security/kms/test_local_provider.py -v
```

---

### Task 4.5: Update Dependencies and Exports

**Complexity:** Low

#### Steps

Update `pyproject.toml`:

```toml
[project.optional-dependencies]
# Individual KMS provider dependencies
kms-vault = [
    "httpx>=0.27.0",
]
kms-aws = [
    "aioboto3>=12.0.0",
]
kms-gcp = [
    "google-cloud-kms>=2.21.0",
]

# Core KMS (includes local encryption capability)
kms = [
    "cryptography>=42.0.0",
]

# All KMS providers
kms-all = [
    "cryptography>=42.0.0",
    "httpx>=0.27.0",
    "aioboto3>=12.0.0",
    "google-cloud-kms>=2.21.0",
]
```

Update `src/fraiseql/security/kms/infrastructure/__init__.py`:

```python
"""KMS infrastructure providers."""

from fraiseql.security.kms.infrastructure.vault import (
    VaultKMSProvider,
    VaultConfig,
)
from fraiseql.security.kms.infrastructure.local import (
    LocalKMSProvider,
    LocalKMSConfig,
)

__all__ = [
    "VaultKMSProvider",
    "VaultConfig",
    "LocalKMSProvider",
    "LocalKMSConfig",
]

# Optional providers (may not be installed)
try:
    from fraiseql.security.kms.infrastructure.aws_kms import (
        AWSKMSProvider,
        AWSKMSConfig,
    )
    __all__.extend(["AWSKMSProvider", "AWSKMSConfig"])
except ImportError:
    pass

try:
    from fraiseql.security.kms.infrastructure.gcp_kms import (
        GCPKMSProvider,
        GCPKMSConfig,
    )
    __all__.extend(["GCPKMSProvider", "GCPKMSConfig"])
except ImportError:
    pass
```

---

### Phase 4 Checkpoint

```bash
uv run pytest tests/unit/security/kms/ -v
uv run pytest --tb=short
uv run ruff check src/fraiseql/security/kms/
uv run mypy src/fraiseql/security/kms/
```

**Phase 4 Completion Checklist:**
- [ ] KeyManager refactored for startup-time usage
- [ ] Local encryption methods added (no KMS call)
- [ ] GCP Cloud KMS provider implemented
- [ ] AWS KMS provider implemented
- [ ] Local development provider implemented
- [ ] Dependencies updated in pyproject.toml
- [ ] Architecture decision documented

---

## Final Release: v2.0

### Pre-Release Checklist

```bash
# Full test suite
uv run pytest --tb=short -v

# Coverage report
uv run pytest --cov=src/fraiseql --cov-report=html

# Linting
uv run ruff check src/

# Type checking
uv run mypy src/

# Build
uv build
```

### Final Checklist

**Phase 1 - Supply Chain:**
- [ ] SBOM module merged and tested
- [ ] Body size limiter implemented
- [ ] Documentation cleaned (no US-specific language)
- [ ] CI/CD workflows verified

**Phase 2 - KMS Core:**
- [ ] KMS domain models implemented
- [ ] BaseKMSProvider ABC implemented
- [ ] Vault provider implemented (extends BaseKMSProvider)
- [ ] KeyManager service implemented

**Phase 3 - Observability & Enforcement:**
- [ ] GraphQL tracing enhanced
- [ ] Security profiles implemented
- [ ] ProfileEnforcer implemented (runtime enforcement)

**Phase 4 - Rust Pipeline Compatibility & Cloud Providers:**
- [ ] KeyManager refactored for startup-time usage (local_encrypt/decrypt)
- [ ] AWS KMS provider implemented
- [ ] GCP Cloud KMS provider implemented
- [ ] Local development provider implemented
- [ ] Architecture decision documented (no per-request KMS calls)

**Release:**
- [ ] CHANGELOG.md updated
- [ ] Version bumped to 2.0.0 in pyproject.toml
- [ ] All tests passing
- [ ] PR created and reviewed

### Release Process

```bash
# 1. Update version in pyproject.toml: version = "2.0.0"

# 2. Update CHANGELOG.md

# 3. Commit and push
git add .
git commit -m "$(cat <<'EOF'
feat: Release v2.0.0 - Security Hardening

- Add SBOM generation (CycloneDX, SPDX)
- Add artifact signing with Sigstore
- Add SLSA Level 2 provenance
- Add request body size limiting
- Add KMS integration (Vault, AWS, GCP)
- Add KeyManager unified interface
- Add enhanced OpenTelemetry tracing
- Add security profiles (standard, regulated, restricted)

🤖 Generated with Claude Code

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"

git push origin feature/v2.0-security-hardening

# 4. Create PR
gh pr create --title "feat: v2.0.0 Security Hardening Release" --base dev
```

---

## Appendix: Testing Commands Quick Reference

```bash
# Run all tests
uv run pytest

# Run specific module tests
uv run pytest tests/unit/security/kms/ -v
uv run pytest tests/unit/middleware/ -v
uv run pytest tests/unit/tracing/ -v

# Run with coverage
uv run pytest --cov=src/fraiseql --cov-report=term-missing

# Linting and types
uv run ruff check src/
uv run mypy src/
```
