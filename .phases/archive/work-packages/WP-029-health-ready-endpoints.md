# WP-029: Implement /ready Endpoint for Kubernetes

**Assignee:** ENG-CORE
**Priority:** P1 (Important)
**Estimated Hours:** 4
**Week:** 2
**Dependencies:** None

---

## Objective

Implement a `/ready` (readiness probe) endpoint in FraiseQL to complement the existing `/health` endpoint, as documented in journey guides and Kubernetes examples.

**Current State:**
- `/health` and `/metrics` endpoints exist in `src/fraiseql/fastapi/apq_metrics_router.py`
- `/ready` endpoint is mentioned in Kubernetes docs (`deploy/kubernetes/README.md:85-89`) as an example pattern but **not implemented** in core FraiseQL
- Journey docs reference `/ready` endpoint that doesn't exist

**Target State:** Production-ready `/ready` endpoint that validates database connectivity and application readiness before accepting traffic.

---

## Problem Statement

**From Journey Doc Verification:**
- `docs/journeys/backend-engineer.md:133` shows:
  ```bash
  # Readiness probe
  curl http://localhost:8000/ready
  ```
- This endpoint does not exist in the codebase (only `/health` and `/metrics`)
- Kubernetes deployments need separate health vs readiness checks
- Missing `/ready` endpoint means Kubernetes may route traffic to non-ready pods

**Health vs Readiness:**
- **`/health` (liveness probe):** Is the app process running? (simple check)
- **`/ready` (readiness probe):** Is the app ready to serve traffic? (database connected, migrations run, etc.)

---

## Technical Design

### Endpoint Specification

**Route:** `GET /ready`
**Purpose:** Kubernetes readiness probe - checks if application is ready to accept traffic

**Response Codes:**
- `200 OK`: Application is ready (database connected, all dependencies available)
- `503 Service Unavailable`: Application not ready (database unreachable, initialization pending)

**Response Body (JSON):**
```json
{
  "status": "ready",  // or "not_ready"
  "checks": {
    "database": "ok",  // or "failed"
    "migrations": "ok",  // or "pending"
    "schema": "ok"  // or "loading"
  },
  "timestamp": "2025-12-08T10:30:00Z"
}
```

### Implementation Details

**File:** `src/fraiseql/fastapi/apq_metrics_router.py` (add to existing health/metrics router)

```python
from fastapi import APIRouter, Response, status
import asyncpg
import time

router = APIRouter()

# Existing endpoints (keep these)
@router.get("/health")
async def health_check():
    """Liveness probe - is the process alive?"""
    return {"status": "healthy", "timestamp": time.time()}

@router.get("/metrics")
async def metrics():
    """Prometheus metrics endpoint"""
    # ... existing implementation ...
    pass

# NEW ENDPOINT ↓
@router.get("/ready")
async def readiness_check(request: Request, response: Response):
    """
    Readiness probe - is the application ready to serve traffic?

    Checks:
    - Database connection pool is available
    - Database is reachable (simple query)
    - Schema is loaded (GraphQL schema initialized)

    Returns:
        200 OK: Application is ready
        503 Service Unavailable: Application not ready
    """
    checks = {
        "database": "unknown",
        "migrations": "unknown",
        "schema": "unknown"
    }

    all_ready = True

    # Check 1: Database connection pool
    try:
        db_pool = request.app.state.db_pool
        if db_pool is None:
            checks["database"] = "failed"
            all_ready = False
        else:
            # Check 2: Database reachability (simple query)
            async with db_pool.acquire() as conn:
                await conn.fetchval("SELECT 1")
            checks["database"] = "ok"
    except (AttributeError, asyncpg.PostgresError, TimeoutError) as e:
        checks["database"] = f"failed: {str(e)}"
        all_ready = False

    # Check 3: Migrations (optional - only if migration system exists)
    # For now, skip this check
    checks["migrations"] = "ok"  # or "not_applicable"

    # Check 4: GraphQL schema loaded
    try:
        schema = request.app.state.graphql_schema
        if schema is None:
            checks["schema"] = "failed"
            all_ready = False
        else:
            checks["schema"] = "ok"
    except AttributeError:
        checks["schema"] = "failed"
        all_ready = False

    # Set response status
    if all_ready:
        response_status = "ready"
        response.status_code = status.HTTP_200_OK
    else:
        response_status = "not_ready"
        response.status_code = status.HTTP_503_SERVICE_UNAVAILABLE

    return {
        "status": response_status,
        "checks": checks,
        "timestamp": time.time()
    }
```

### Kubernetes Integration

**Update:** `deploy/kubernetes/helm/fraiseql/templates/deployment.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
spec:
  template:
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:latest
        ports:
        - containerPort: 8000

        # Liveness probe - restart pod if process crashes
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 30
          timeoutSeconds: 5
          failureThreshold: 3

        # Readiness probe - don't route traffic until ready
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
```

**Why Both Probes?**
- **Liveness probe** (`/health`): Restarts pod if it crashes (process-level check)
- **Readiness probe** (`/ready`): Removes pod from load balancer if database is down (dependency-level check)

---

## Files to Modify

### 1. `src/fraiseql/fastapi/apq_metrics_router.py`
**Changes:**
- Add `@router.get("/ready")` endpoint
- Implement database connectivity check
- Implement schema initialization check
- Return 200 or 503 based on checks

**Lines to add:** ~30-50 lines

### 2. `deploy/kubernetes/helm/fraiseql/templates/deployment.yaml`
**Changes:**
- Add `readinessProbe` configuration
- Update comments to explain difference from `livenessProbe`

**Lines to modify:** ~10-15 lines

### 3. `docs/reference/api.md` (or create `docs/reference/health-endpoints.md`)
**New section:**

```markdown
## Health & Readiness Endpoints

FraiseQL provides two health check endpoints for production deployments:

### `/health` - Liveness Probe

**Purpose:** Check if the application process is alive (for Kubernetes liveness probes)

**Response:**
```json
{
  "status": "healthy",
  "timestamp": 1670500000.0
}
```

**Status Codes:**
- `200 OK`: Process is running

**Use Case:** Kubernetes liveness probe - restart pod if this fails

---

### `/ready` - Readiness Probe

**Purpose:** Check if the application is ready to serve traffic (for Kubernetes readiness probes)

**Response (Ready):**
```json
{
  "status": "ready",
  "checks": {
    "database": "ok",
    "migrations": "ok",
    "schema": "ok"
  },
  "timestamp": 1670500000.0
}
```

**Response (Not Ready):**
```json
{
  "status": "not_ready",
  "checks": {
    "database": "failed: connection timeout",
    "migrations": "ok",
    "schema": "ok"
  },
  "timestamp": 1670500000.0
}
```

**Status Codes:**
- `200 OK`: Application is ready to serve traffic
- `503 Service Unavailable`: Application not ready (don't route traffic)

**Use Case:** Kubernetes readiness probe - remove pod from load balancer if database is unreachable

---

### Kubernetes Configuration

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8000
  initialDelaySeconds: 10
  periodSeconds: 30

readinessProbe:
  httpGet:
    path: /ready
    port: 8000
  initialDelaySeconds: 5
  periodSeconds: 10
```
```

### 4. `docs/journeys/backend-engineer.md`
**Changes:**
- No changes needed (already references `/ready` - now it works!)

---

## Acceptance Criteria

### Functional Requirements
- ✅ `/ready` endpoint returns 200 when database is reachable
- ✅ `/ready` endpoint returns 503 when database is unreachable
- ✅ Response includes detailed check results (database, schema)
- ✅ Endpoint responds within 5 seconds (timeout for Kubernetes)

### Kubernetes Requirements
- ✅ Readiness probe configuration in Helm chart
- ✅ Liveness probe uses `/health`, readiness probe uses `/ready`
- ✅ Pod removed from service when `/ready` returns 503
- ✅ Pod added back to service when `/ready` returns 200

### Documentation Requirements
- ✅ API reference documents `/ready` endpoint
- ✅ Difference between `/health` and `/ready` explained
- ✅ Kubernetes example configuration provided
- ✅ Journey doc step now works (curl command succeeds)

### Testing Requirements
- ✅ Unit test: `/ready` returns 200 when database is up
- ✅ Unit test: `/ready` returns 503 when database is down
- ✅ Integration test: Kubernetes readiness probe works in test cluster
- ✅ Manual test: Backend engineer persona can curl `/ready` endpoint

---

## Testing Plan

### Unit Tests (`tests/unit/test_health_endpoints.py`)

```python
import pytest
from fastapi.testclient import TestClient
from fraiseql.fastapi import create_fraiseql_app

def test_ready_endpoint_healthy():
    """Test /ready returns 200 when database is reachable."""
    app = create_fraiseql_app(database_url="postgresql://localhost/test")
    client = TestClient(app)

    response = client.get("/ready")
    assert response.status_code == 200
    data = response.json()
    assert data["status"] == "ready"
    assert data["checks"]["database"] == "ok"

def test_ready_endpoint_database_down():
    """Test /ready returns 503 when database is unreachable."""
    app = create_fraiseql_app(database_url="postgresql://invalid-host/test")
    client = TestClient(app)

    response = client.get("/ready")
    assert response.status_code == 503
    data = response.json()
    assert data["status"] == "not_ready"
    assert "failed" in data["checks"]["database"]

def test_health_vs_ready_difference():
    """Test that /health always returns 200, /ready depends on dependencies."""
    app = create_fraiseql_app(database_url="postgresql://invalid-host/test")
    client = TestClient(app)

    # Health should always be 200 (process is alive)
    health_response = client.get("/health")
    assert health_response.status_code == 200

    # Ready should be 503 (database unreachable)
    ready_response = client.get("/ready")
    assert ready_response.status_code == 503
```

### Integration Tests (`tests/integration/test_kubernetes_probes.py`)

```python
import pytest
import httpx
import asyncio

@pytest.mark.asyncio
async def test_kubernetes_readiness_scenario():
    """Simulate Kubernetes readiness probe behavior."""
    # Start app
    async with httpx.AsyncClient(base_url="http://localhost:8000") as client:
        # Initially not ready (database starting up)
        response = await client.get("/ready")
        assert response.status_code == 503

        # Wait for database to be ready
        await asyncio.sleep(2)

        # Now ready
        response = await client.get("/ready")
        assert response.status_code == 200

        # Simulate database failure
        # (stop database container)

        # Should become not ready
        response = await client.get("/ready")
        assert response.status_code == 503
```

### Manual Testing

```bash
# Test locally
curl http://localhost:8000/health  # Should always return 200
curl http://localhost:8000/ready   # Should return 200 if DB is up, 503 if down

# Test in Kubernetes
kubectl port-forward pod/fraiseql-xxx 8000:8000
curl http://localhost:8000/ready

# Simulate database failure
kubectl exec -it postgres-pod -- pg_ctl stop
curl http://localhost:8000/ready  # Should return 503

# Pod should be removed from service
kubectl get endpoints  # fraiseql service should have 0 endpoints
```

---

## Implementation Steps

### Step 1: Core Implementation (2 hours)
1. Add `/ready` endpoint to `apq_metrics_router.py`
2. Implement database connectivity check
3. Implement schema initialization check
4. Add response status logic (200 vs 503)

### Step 2: Testing (1 hour)
1. Write unit tests for ready endpoint
2. Test with database up and down scenarios
3. Verify response format matches spec

### Step 3: Kubernetes Integration (1 hour)
1. Update Helm chart with readiness probe
2. Test in local Kubernetes cluster (minikube or kind)
3. Verify pod lifecycle behavior

---

## DO NOT

- ❌ Do not make `/ready` check too slow (must respond <5 seconds)
- ❌ Do not make `/ready` same as `/health` (defeats purpose)
- ❌ Do not include external API checks in `/ready` (only database)
- ❌ Do not return 200 if database is unreachable (violates readiness contract)
- ❌ Do not forget to document difference from `/health`

---

## Success Metrics

### Technical
- `/ready` endpoint implemented and tested
- Kubernetes readiness probe works in test cluster
- Response time <5 seconds (meets Kubernetes timeout)

### User Experience
- DevOps engineer can configure Kubernetes probes using documented example
- Journey doc curl command now works (`curl /ready`)
- Clear understanding of health vs readiness (documented)

---

## Related Work Packages

- **WP-004:** Backend Engineer Journey (fixes `/ready` reference)
- **WP-014:** Production Deployment Checklist (add readiness probe configuration)
- **WP-027:** Connection Pooling (readiness checks use connection pool)

---

## Notes

**Why This Matters:**
- Kubernetes readiness probes are critical for zero-downtime deployments
- Missing `/ready` endpoint causes traffic routing to non-ready pods
- Separating health from readiness is a Kubernetes best practice
- Documentation claiming endpoint that doesn't exist damages credibility

**Alternatives Considered:**
1. Use `/health` for both liveness and readiness → Doesn't allow separate lifecycle
2. Make `/ready` optional (configuration flag) → Too complex, readiness probes are standard
3. Only document in Kubernetes examples, don't implement → Breaks journey doc

**Decision:** Implement `/ready` endpoint in core FraiseQL (this WP)

---

**End of WP-029**
