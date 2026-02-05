# Fraisier REST API Reference

**Version**: 0.1.0
**Base URL**: `http://localhost:8000/api/v1`
**Authentication**: Bearer token in `Authorization` header

## Quick Start

```bash
# Get authentication token
TOKEN=$(fraisier auth:token)

# List all deployments
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/deployments

# Deploy a service
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"version":"2.0.0"}' \
  http://localhost:8000/api/v1/deployments/my_api/production
```

---

## Authentication

### Bearer Token Format

```bash
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

### Get Authentication Token

```bash
# Generate token
fraisier auth:token

# OR via API (using username/password)
curl -X POST http://localhost:8000/api/v1/auth/token \
  -H "Content-Type: application/json" \
  -d '{"username":"user","password":"pass"}'
```

**Response** (200 OK):
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "Bearer",
  "expires_in": 3600
}
```

### Token Refresh

```bash
curl -X POST http://localhost:8000/api/v1/auth/refresh \
  -H "Authorization: Bearer $OLD_TOKEN"
```

**Response** (200 OK):
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_in": 3600
}
```

---

## HTTP Status Codes

| Code | Meaning | Common Causes |
|------|---------|---------------|
| 200 | Success | Request completed successfully |
| 201 | Created | New resource created |
| 202 | Accepted | Request accepted, processing async |
| 204 | No Content | Success, no response body |
| 400 | Bad Request | Invalid parameters or malformed JSON |
| 401 | Unauthorized | Missing or invalid authentication token |
| 403 | Forbidden | Insufficient permissions for resource |
| 404 | Not Found | Resource not found |
| 409 | Conflict | Resource already exists or state conflict |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Server Error | Internal server error |
| 503 | Service Unavailable | Server temporarily unavailable |

---

## Error Response Format

All error responses follow this format:

```json
{
  "error": {
    "code": "DEPLOYMENT_NOT_FOUND",
    "message": "Deployment dep_123 not found",
    "details": {
      "fraise": "my_api",
      "deployment_id": "dep_123"
    }
  }
}
```

### Common Error Codes

- `INVALID_PARAMETERS`: Request parameters are invalid
- `FRAISE_NOT_FOUND`: Fraise doesn't exist
- `ENVIRONMENT_NOT_FOUND`: Environment not configured
- `DEPLOYMENT_NOT_FOUND`: Deployment doesn't exist
- `DEPLOYMENT_IN_PROGRESS`: Cannot perform action, deployment in progress
- `DEPLOYMENT_FAILED`: Deployment failed
- `PROVIDER_ERROR`: Error communicating with provider
- `PERMISSION_DENIED`: User lacks required permissions
- `RATE_LIMIT_EXCEEDED`: Too many requests

---

## Deployments API

### List Deployments

```http
GET /deployments?environment=production&status=success&limit=10&offset=0
```

**Query Parameters**:

- `environment` (string, optional): Filter by environment (dev, staging, production, etc.)
- `status` (string, optional): Filter by status (pending, in_progress, success, failed, cancelled)
- `fraise` (string, optional): Filter by fraise name
- `limit` (integer, optional): Max results per page (default: 50, max: 500)
- `offset` (integer, optional): Pagination offset (default: 0)
- `sort` (string, optional): Sort by field (created_at, updated_at, status) with - prefix for descending
- `created_after` (string, optional): ISO 8601 timestamp (e.g., 2024-01-22T10:00:00Z)
- `created_before` (string, optional): ISO 8601 timestamp

**Response** (200 OK):
```json
{
  "deployments": [
    {
      "id": "dep_00001",
      "fraise": "my_api",
      "environment": "production",
      "status": "success",
      "triggered_by": "user_123",
      "version": "2.0.0",
      "previous_version": "1.9.0",
      "strategy": "rolling",
      "created_at": "2024-01-22T10:00:00Z",
      "started_at": "2024-01-22T10:00:05Z",
      "completed_at": "2024-01-22T10:05:30Z",
      "duration_seconds": 325,
      "health_check_status": "passing",
      "rollback_available": false,
      "events_count": 12,
      "metrics": {
        "success_rate": 100.0,
        "error_rate": 0.0,
        "latency_p99_ms": 150
      }
    }
  ],
  "pagination": {
    "total": 156,
    "limit": 10,
    "offset": 0,
    "pages": 16
  }
}
```

**Example**:
```bash
# List recent production deployments
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments?environment=production&limit=5&sort=-created_at"

# List failed deployments
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments?status=failed&limit=10"

# List deployments for specific fraise
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments?fraise=my_api&environment=staging"
```

---

### Get Deployment Details

```http
GET /deployments/{deployment_id}
```

**Path Parameters**:

- `deployment_id` (string, required): Deployment ID (e.g., dep_00001)

**Response** (200 OK):
```json
{
  "id": "dep_00001",
  "fraise": "my_api",
  "environment": "production",
  "status": "success",
  "triggered_by": "user_123",
  "trigger_type": "api",
  "version": "2.0.0",
  "previous_version": "1.9.0",
  "strategy": "rolling",
  "provider": "bare_metal",
  "created_at": "2024-01-22T10:00:00Z",
  "started_at": "2024-01-22T10:00:05Z",
  "completed_at": "2024-01-22T10:05:30Z",
  "duration_seconds": 325,
  "health_check_status": "passing",
  "health_check_details": {
    "checks_passed": 3,
    "checks_total": 3,
    "last_check_at": "2024-01-22T10:05:25Z"
  },
  "events": [
    {
      "id": "evt_001",
      "event_type": "deployment.started",
      "timestamp": "2024-01-22T10:00:05Z"
    }
  ],
  "logs": {
    "url": "/api/v1/deployments/dep_00001/logs",
    "lines": 147
  },
  "metrics": {
    "success_rate": 100.0,
    "error_rate": 0.0,
    "latency_p50_ms": 120,
    "latency_p99_ms": 150
  },
  "rollback_available": false
}
```

**Example**:
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/deployments/dep_00001
```

---

### Create Deployment

```http
POST /deployments/{fraise}/{environment}
```

**Path Parameters**:

- `fraise` (string, required): Fraise name (e.g., my_api)
- `environment` (string, required): Target environment (e.g., production)

**Request Body** (JSON):
```json
{
  "version": "2.0.0",
  "strategy": "rolling",
  "strategy_config": {
    "max_instances_down": 1,
    "health_check_delay": 10,
    "health_check_timeout": 30
  },
  "skip_health_check": false,
  "skip_backup": false,
  "wait": false,
  "timeout": 600,
  "metadata": {
    "ticket": "DEPLOY-123",
    "reason": "Bug fix for issue #456"
  }
}
```

**Request Parameters**:

- `version` (string, optional): Specific version to deploy (default: latest)
- `strategy` (string, optional): Deployment strategy
  - `rolling`: One instance at a time (default)
  - `blue_green`: Switch all at once
  - `canary`: Gradual rollout with metrics checking
- `strategy_config` (object, optional): Strategy-specific configuration
- `skip_health_check` (boolean, optional): Skip health checks after deployment (default: false)
- `skip_backup` (boolean, optional): Skip backup before deployment (default: false)
- `wait` (boolean, optional): Wait for completion before returning (default: false)
- `timeout` (integer, optional): Max seconds to wait if wait=true (default: 3600)
- `metadata` (object, optional): Custom metadata for audit logging

**Response** (202 Accepted):
```json
{
  "id": "dep_00002",
  "fraise": "my_api",
  "environment": "production",
  "status": "pending",
  "version": "2.0.0",
  "strategy": "rolling",
  "created_at": "2024-01-22T11:00:00Z",
  "status_url": "/api/v1/deployments/dep_00002",
  "events_url": "/api/v1/deployments/dep_00002/events"
}
```

**Example**:
```bash
# Deploy latest version with default rolling strategy
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  http://localhost:8000/api/v1/deployments/my_api/production

# Deploy specific version with blue-green strategy
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "version": "2.0.0",
    "strategy": "blue_green",
    "metadata": {"ticket": "DEPLOY-123"}
  }' \
  http://localhost:8000/api/v1/deployments/my_api/production

# Deploy with wait
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"wait": true, "timeout": 600}' \
  http://localhost:8000/api/v1/deployments/my_api/production
```

---

### Cancel Deployment

```http
DELETE /deployments/{deployment_id}
```

**Path Parameters**:

- `deployment_id` (string, required): Deployment ID to cancel

**Response** (202 Accepted):
```json
{
  "id": "dep_00001",
  "status": "cancelled",
  "cancelled_at": "2024-01-22T10:05:00Z",
  "cancelled_by": "user_456"
}
```

**Example**:
```bash
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/deployments/dep_00001
```

---

### Rollback Deployment

```http
POST /deployments/{fraise}/{environment}/rollback
```

**Path Parameters**:

- `fraise` (string, required): Fraise name
- `environment` (string, required): Target environment

**Request Body** (JSON):
```json
{
  "to_version": "1.9.0",
  "reason": "Critical bug in 2.0.0"
}
```

**Request Parameters**:

- `to_version` (string, optional): Specific version to rollback to (default: previous)
- `reason` (string, optional): Reason for rollback (for audit logging)

**Response** (202 Accepted):
```json
{
  "id": "dep_00003",
  "fraise": "my_api",
  "environment": "production",
  "status": "pending",
  "type": "rollback",
  "from_version": "2.0.0",
  "to_version": "1.9.0",
  "created_at": "2024-01-22T10:06:00Z"
}
```

**Example**:
```bash
# Rollback to previous version
curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/deployments/my_api/production/rollback

# Rollback to specific version with reason
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"to_version": "1.8.0", "reason": "Critical bug"}' \
  http://localhost:8000/api/v1/deployments/my_api/production/rollback
```

---

### Get Deployment Logs

```http
GET /deployments/{deployment_id}/logs?lines=100&follow=false
```

**Path Parameters**:

- `deployment_id` (string, required): Deployment ID

**Query Parameters**:

- `lines` (integer, optional): Number of lines to return (default: 100, max: 1000)
- `follow` (boolean, optional): Stream logs (SSE) (default: false)
- `level` (string, optional): Filter by log level (info, warn, error)
- `component` (string, optional): Filter by component (deployment, health_check, provider)

**Response** (200 OK):
```json
{
  "deployment_id": "dep_00001",
  "logs": [
    {
      "timestamp": "2024-01-22T10:00:05Z",
      "level": "info",
      "component": "deployment",
      "message": "Deployment started",
      "details": {}
    },
    {
      "timestamp": "2024-01-22T10:00:06Z",
      "level": "info",
      "component": "provider",
      "message": "Connecting to bare metal host",
      "details": {"host": "prod-1.example.com"}
    },
    {
      "timestamp": "2024-01-22T10:05:25Z",
      "level": "info",
      "component": "health_check",
      "message": "Health check passed",
      "details": {"duration_ms": 50}
    }
  ],
  "total_lines": 147,
  "truncated": false
}
```

**Example**:
```bash
# Get last 50 lines of logs
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments/dep_00001/logs?lines=50"

# Get error logs only
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments/dep_00001/logs?level=error"

# Stream deployment logs
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments/dep_00001/logs?follow=true"
```

---

### Get Deployment Events

```http
GET /deployments/{deployment_id}/events
```

**Path Parameters**:

- `deployment_id` (string, required): Deployment ID

**Response** (200 OK):
```json
{
  "deployment_id": "dep_00001",
  "events": [
    {
      "id": "evt_001",
      "event_type": "deployment.started",
      "timestamp": "2024-01-22T10:00:05Z",
      "data": {
        "fraise": "my_api",
        "environment": "production",
        "strategy": "rolling"
      }
    },
    {
      "id": "evt_002",
      "event_type": "health_check.passed",
      "timestamp": "2024-01-22T10:05:25Z",
      "data": {
        "duration_ms": 50,
        "checks_passed": 3
      }
    },
    {
      "id": "evt_003",
      "event_type": "deployment.completed",
      "timestamp": "2024-01-22T10:05:30Z",
      "data": {
        "status": "success",
        "duration_seconds": 325
      }
    }
  ],
  "total": 12
}
```

**Example**:
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/deployments/dep_00001/events
```

---

## Fraises API

### List All Fraises

```http
GET /fraises?environment=production&type=api&limit=50&offset=0
```

**Query Parameters**:

- `environment` (string, optional): Filter by environment
- `type` (string, optional): Filter by type (api, etl, scheduled)
- `limit` (integer, optional): Max results (default: 50)
- `offset` (integer, optional): Pagination offset (default: 0)

**Response** (200 OK):
```json
{
  "fraises": [
    {
      "name": "my_api",
      "type": "api",
      "description": "Main API service",
      "git_provider": "github",
      "git_repo": "my-org/my-api",
      "git_branch": "main",
      "status": "healthy",
      "environments": ["development", "staging", "production"],
      "current_versions": {
        "development": "2.1.0-dev",
        "staging": "2.0.0",
        "production": "1.9.0"
      },
      "last_deployment": {
        "id": "dep_00001",
        "timestamp": "2024-01-22T10:05:30Z",
        "status": "success"
      }
    }
  ],
  "pagination": {
    "total": 8,
    "limit": 50,
    "offset": 0,
    "pages": 1
  }
}
```

**Example**:
```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/fraises"

curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/fraises?type=api&environment=production"
```

---

### Get Fraise Details

```http
GET /fraises/{fraise_name}
```

**Path Parameters**:

- `fraise_name` (string, required): Fraise name

**Response** (200 OK):
```json
{
  "name": "my_api",
  "type": "api",
  "description": "Main API service",
  "git_provider": "github",
  "git_repo": "my-org/my-api",
  "git_branch": "main",
  "status": "healthy",
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-22T10:00:00Z",
  "environments": {
    "development": {
      "current_version": "2.1.0-dev",
      "previous_version": "2.0.1-dev",
      "status": "healthy",
      "deployment_strategy": "rolling",
      "last_deployment": {
        "id": "dep_00010",
        "timestamp": "2024-01-22T09:00:00Z",
        "status": "success"
      }
    },
    "staging": {
      "current_version": "2.0.0",
      "previous_version": "1.9.5",
      "status": "healthy",
      "deployment_strategy": "blue_green",
      "last_deployment": {
        "id": "dep_00005",
        "timestamp": "2024-01-20T14:30:00Z",
        "status": "success"
      }
    },
    "production": {
      "current_version": "1.9.0",
      "previous_version": "1.8.5",
      "status": "healthy",
      "deployment_strategy": "canary",
      "last_deployment": {
        "id": "dep_00001",
        "timestamp": "2024-01-22T10:05:30Z",
        "status": "success"
      }
    }
  }
}
```

**Example**:
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/api/v1/fraises/my_api
```

---

### Get Fraise Status

```http
GET /fraises/{fraise_name}/status?environment=production
```

**Path Parameters**:

- `fraise_name` (string, required): Fraise name

**Query Parameters**:

- `environment` (string, optional): Specific environment (returns all if not specified)

**Response** (200 OK):
```json
{
  "fraise": "my_api",
  "overall_status": "healthy",
  "environments": {
    "production": {
      "status": "healthy",
      "current_version": "1.9.0",
      "health_checks": {
        "status": "passing",
        "checks_passed": 3,
        "checks_total": 3,
        "last_check": "2024-01-22T10:58:30Z"
      },
      "metrics": {
        "success_rate": 99.95,
        "error_rate": 0.05,
        "latency_p50_ms": 120,
        "latency_p99_ms": 350
      },
      "instances": 4,
      "instances_healthy": 4
    }
  }
}
```

---

### Get Fraise Deployment History

```http
GET /fraises/{fraise_name}/history?environment=production&limit=20
```

**Path Parameters**:

- `fraise_name` (string, required): Fraise name

**Query Parameters**:

- `environment` (string, optional): Filter by environment
- `limit` (integer, optional): Max results (default: 20)
- `status` (string, optional): Filter by status (success, failed, cancelled)

**Response** (200 OK):
```json
{
  "fraise": "my_api",
  "environment": "production",
  "history": [
    {
      "id": "dep_00001",
      "version": "1.9.0",
      "previous_version": "1.8.5",
      "status": "success",
      "strategy": "canary",
      "triggered_by": "user_123",
      "created_at": "2024-01-22T10:00:00Z",
      "completed_at": "2024-01-22T10:05:30Z",
      "duration_seconds": 330
    },
    {
      "id": "dep_00000",
      "version": "1.8.5",
      "previous_version": "1.8.0",
      "status": "success",
      "strategy": "rolling",
      "triggered_by": "webhook",
      "created_at": "2024-01-21T14:00:00Z",
      "completed_at": "2024-01-21T14:08:45Z",
      "duration_seconds": 525
    }
  ],
  "total": 247
}
```

---

## Environments API

### List Environments

```http
GET /environments
```

**Response** (200 OK):
```json
{
  "environments": [
    {
      "name": "development",
      "display_name": "Development",
      "description": "Local development environment",
      "provider": "docker_compose",
      "fraises_deployed": 8,
      "status": "healthy"
    },
    {
      "name": "staging",
      "display_name": "Staging",
      "description": "Pre-production testing",
      "provider": "bare_metal",
      "fraises_deployed": 8,
      "status": "healthy"
    },
    {
      "name": "production",
      "display_name": "Production",
      "description": "Live environment",
      "provider": "bare_metal",
      "fraises_deployed": 8,
      "status": "healthy"
    }
  ]
}
```

---

### Get Environment Status

```http
GET /environments/{environment_name}/status
```

**Path Parameters**:

- `environment_name` (string, required): Environment name

**Response** (200 OK):
```json
{
  "name": "production",
  "status": "healthy",
  "provider": {
    "type": "bare_metal",
    "status": "connected",
    "hosts": 4,
    "hosts_healthy": 4
  },
  "database": {
    "status": "connected",
    "connections": 12,
    "max_connections": 100
  },
  "nats": {
    "status": "connected",
    "streams": 3,
    "events_published": 12345
  },
  "fraises": {
    "total": 8,
    "healthy": 8,
    "degraded": 0,
    "unhealthy": 0
  }
}
```

---

## Health & Status API

### System Health Check

```http
GET /health
```

**Response** (200 OK):
```json
{
  "status": "healthy",
  "timestamp": "2024-01-22T10:58:30Z",
  "components": {
    "database": {
      "status": "healthy",
      "response_time_ms": 5
    },
    "nats": {
      "status": "healthy",
      "connected": true,
      "streams": 3
    },
    "providers": {
      "status": "healthy",
      "available_providers": 3,
      "connected_providers": 3
    }
  }
}
```

---

### System Metrics

```http
GET /metrics
```

**Response** (200 OK - Prometheus format):
```
# HELP fraisier_deployments_total Total deployments
# TYPE fraisier_deployments_total counter
fraisier_deployments_total{status="success"} 156
fraisier_deployments_total{status="failed"} 12
fraisier_deployments_total{status="cancelled"} 3

# HELP fraisier_deployment_duration_seconds Deployment duration
# TYPE fraisier_deployment_duration_seconds histogram
fraisier_deployment_duration_seconds_bucket{le="60"} 23
fraisier_deployment_duration_seconds_bucket{le="300"} 124
fraisier_deployment_duration_seconds_bucket{le="600"} 156

# HELP fraisier_active_deployments Active deployments
# TYPE fraisier_active_deployments gauge
fraisier_active_deployments{environment="production"} 0
fraisier_active_deployments{environment="staging"} 1
```

---

## Rate Limiting

All API endpoints are subject to rate limiting:

**Default Limits**:

- Authenticated users: 1000 requests per hour
- Read operations: 2000 requests per hour
- Write operations: 100 requests per hour

**Rate Limit Headers**:
```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1705929510
```

**When Rate Limited** (429 Conflict):
```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Rate limit exceeded. 999 requests remaining.",
    "retry_after": 60
  }
}
```

---

## Pagination

List endpoints support cursor-based pagination:

**Query Parameters**:

- `limit` (integer): Results per page (default: 50, max: 500)
- `offset` (integer): Pagination offset (default: 0)

**Response**:
```json
{
  "items": [...],
  "pagination": {
    "total": 156,
    "limit": 50,
    "offset": 0,
    "pages": 4,
    "has_more": true
  }
}
```

---

## Filtering & Searching

List endpoints support filtering:

**Query Parameters**:

- `q` (string): Free-text search across name, description
- `sort` (string): Sort by field (- prefix for descending)
- `field` (string): Filter by specific field value

**Examples**:
```bash
# Search by name
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments?q=api"

# Sort by date descending
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments?sort=-created_at"

# Multiple filters
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments?environment=production&status=success&sort=-created_at"
```

---

## API Examples

### Complete Deployment Flow

```bash
#!/bin/bash
set -e

TOKEN=$(fraisier auth:token)
API="http://localhost:8000/api/v1"

# 1. Get fraise info
echo "Getting fraise info..."
curl -H "Authorization: Bearer $TOKEN" "$API/fraises/my_api"

# 2. Trigger deployment
echo "Triggering deployment..."
DEPLOY=$(curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"strategy":"rolling"}' \
  "$API/deployments/my_api/production")

DEPLOY_ID=$(echo $DEPLOY | jq -r '.id')
echo "Deployment started: $DEPLOY_ID"

# 3. Watch deployment progress
echo "Watching deployment..."
while true; do
  STATUS=$(curl -s -H "Authorization: Bearer $TOKEN" \
    "$API/deployments/$DEPLOY_ID" | jq -r '.status')

  if [ "$STATUS" = "success" ]; then
    echo "Deployment successful!"
    break
  elif [ "$STATUS" = "failed" ]; then
    echo "Deployment failed!"
    exit 1
  fi

  echo "Status: $STATUS"
  sleep 5
done

# 4. View final logs
echo "Deployment logs:"
curl -H "Authorization: Bearer $TOKEN" \
  "$API/deployments/$DEPLOY_ID/logs?lines=50"
```

---

## SDK Examples

### Python Example

```python
import requests

token = "your_bearer_token"
api = "http://localhost:8000/api/v1"
headers = {"Authorization": f"Bearer {token}"}

# List deployments
response = requests.get(
    f"{api}/deployments?environment=production",
    headers=headers
)
deployments = response.json()["deployments"]

# Trigger deployment
response = requests.post(
    f"{api}/deployments/my_api/production",
    headers=headers,
    json={
        "version": "2.0.0",
        "strategy": "blue_green"
    }
)
deployment = response.json()
print(f"Deployment {deployment['id']} started")
```

### JavaScript Example

```javascript
const token = "your_bearer_token";
const api = "http://localhost:8000/api/v1";

// List deployments
const response = await fetch(
  `${api}/deployments?environment=production`,
  {
    headers: { "Authorization": `Bearer ${token}` }
  }
);
const { deployments } = await response.json();

// Trigger deployment
const deployResponse = await fetch(
  `${api}/deployments/my_api/production`,
  {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${token}`,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({
      version: "2.0.0",
      strategy: "blue_green"
    })
  }
);
const deployment = await deployResponse.json();
console.log(`Deployment ${deployment.id} started`);
```

---

## Versioning

**Current API Version**: v1

**Compatibility**: Breaking changes will be released as v2, v3, etc.

**Deprecation Policy**: Deprecated features will be supported for at least 6 months before removal.

---

## Support

For API issues or questions:

- GitHub Issues: https://github.com/your-org/fraisier/issues
- Discord: https://discord.gg/your-invite
- Email: support@fraisier.dev
