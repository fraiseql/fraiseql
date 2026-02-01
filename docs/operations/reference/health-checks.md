# FraiseQL v2 Health Checks Guide

**Version**: 1.0
**Last Updated**: 2026-01-31
**Audience**: DevOps engineers, SREs, operations teams

---

## Overview

This guide explains FraiseQL's three-tier health check system for production deployments. Health checks enable automated monitoring, graceful failover, and operational reliability.

## Table of Contents

1. [Quick Reference](#quick-reference)
2. [Health Endpoint Types](#health-endpoint-types)
3. [Integration Patterns](#integration-patterns)
4. [Troubleshooting](#troubleshooting)
5. [Best Practices](#best-practices)

---

## Quick Reference

| Endpoint | Purpose | HTTP Status | Use Case |
|----------|---------|-------------|----------|
| `/health` | Overall status | 200 | Monitoring, dashboards |
| `/ready` | Ready to accept traffic | 200/503 | Readiness probe, load balancer |
| `/live` | Process alive | 200 | Liveness probe, restart detection |

---

## Health Endpoint Types

### 1. `/health` - Overall System Health

**Purpose**: General health status monitoring. Returns quickly without dependency checks.

**Request**:
```bash
GET /health HTTP/1.1
Host: localhost:8000
```

**Response - Healthy** (HTTP 200):
```json
{
  "status": "healthy",
  "timestamp": 1706794800,
  "uptime_seconds": 3600
}
```

**Fields**:
- `status` (string): `"healthy"` or `"degraded"` or `"unhealthy"`
- `timestamp` (u64): Unix timestamp when checked
- `uptime_seconds` (u64): Seconds since server started

**Interpretation**:
- `200 OK` → System is running normally
- `500 Internal Server Error` → System encountered fatal error

**Typical Response Time**: < 5ms

---

### 2. `/ready` - Readiness Probe (Can Accept Requests?)

**Purpose**: Determines if the service can handle incoming requests. Checks critical dependencies.

**Request**:
```bash
GET /ready HTTP/1.1
Host: localhost:8000
```

**Response - Ready** (HTTP 200):
```json
{
  "ready": true,
  "database_connected": true,
  "cache_available": true,
  "reason": null
}
```

**Response - Not Ready** (HTTP 503 Service Unavailable):
```json
{
  "ready": false,
  "database_connected": false,
  "cache_available": true,
  "reason": "Database unavailable"
}
```

**Fields**:
- `ready` (bool): Can service accept requests?
- `database_connected` (bool): Can reach primary database?
- `cache_available` (bool): Is cache/Redis available?
- `reason` (string|null): Human-readable reason if not ready

**Interpretation**:
- `200 OK` → Service is ready, add to load balancer
- `503 Service Unavailable` → Service not ready, remove from load balancer

**Typical Response Time**: 50-200ms (includes dependency checks)

**When to Return 503**:
- Database connection fails
- Critical cache unavailable (if required by schema)
- Configuration invalid
- Startup not complete

---

### 3. `/live` - Liveness Probe (Process Alive?)

**Purpose**: Lightweight check that process is responsive. Used to detect hung processes.

**Request**:
```bash
GET /live HTTP/1.1
Host: localhost:8000
```

**Response** (HTTP 200):
```json
{
  "alive": true,
  "pid": 42157,
  "response_time_ms": 1
}
```

**Fields**:
- `alive` (bool): Is process responding?
- `pid` (u32): Process ID
- `response_time_ms` (u32): Time to respond in milliseconds

**Interpretation**:
- `200 OK` → Process is alive and responsive
- `5xx Server Error` → Process hung or crashed

**Typical Response Time**: 1-5ms

**What It Doesn't Check**:
- ❌ Database connectivity
- ❌ Cache availability
- ❌ Configuration validity
- ❌ Request queue depth

---

## Integration Patterns

### Pattern 1: Kubernetes Deployment

Complete Kubernetes deployment with all three probes:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-api
  namespace: default
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:2.0.0
        imagePullPolicy: IfNotPresent

        ports:
        - name: http
          containerPort: 8000
          protocol: TCP

        env:
        - name: RUST_LOG
          value: "info"
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        - name: CACHE_URL
          valueFrom:
            configMapKeyRef:
              name: fraiseql-config
              key: cache-url

        # Startup Probe (wait for app to start)
        # Only needed if startup takes > 30 seconds
        startupProbe:
          httpGet:
            path: /ready
            port: http
            scheme: HTTP
          initialDelaySeconds: 0
          periodSeconds: 10
          timeoutSeconds: 2
          failureThreshold: 30
          # Allows up to 300 seconds (30 * 10) for startup

        # Readiness Probe (when to add to load balancer)
        readinessProbe:
          httpGet:
            path: /ready
            port: http
            scheme: HTTP
          initialDelaySeconds: 5
          periodSeconds: 10
          timeoutSeconds: 2
          successThreshold: 1
          failureThreshold: 3
          # Removed from LB after 3 consecutive failures

        # Liveness Probe (when to restart)
        livenessProbe:
          httpGet:
            path: /live
            port: http
            scheme: HTTP
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 2
          failureThreshold: 3
          # Pod restarted after 3 consecutive failures (30 seconds of unresponsiveness)

        # Resources
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"

        # Graceful shutdown
        lifecycle:
          preStop:
            exec:
              command: ["/bin/sh", "-c", "sleep 10"]

      # Termination grace period should match server's shutdown timeout
      terminationGracePeriodSeconds: 30
```

### Pattern 2: Docker Compose

```yaml
version: '3.8'

services:
  fraiseql:
    image: fraiseql:2.0.0
    environment:
      DATABASE_URL: postgres://user:pass@postgres:5432/db
      RUST_LOG: info
    ports:
      - "8000:8000"

    # Docker healthcheck
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/ready"]
      interval: 10s
      timeout: 2s
      retries: 3
      start_period: 5s

    depends_on:
      postgres:
        condition: service_healthy

  postgres:
    image: postgres:15
    environment:
      POSTGRES_PASSWORD: password
      POSTGRES_DB: fraiseql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 10s
      timeout: 5s
      retries: 5
```

### Pattern 3: AWS Application Load Balancer

```hcl
resource "aws_lb_target_group" "fraiseql" {
  name        = "fraiseql-tg"
  port        = 8000
  protocol    = "HTTP"
  vpc_id      = aws_vpc.main.id

  # Health check configuration
  health_check {
    healthy_threshold   = 2
    unhealthy_threshold = 3
    timeout             = 2
    interval            = 10
    path                = "/ready"
    port                = "8000"
    protocol            = "HTTP"
    matcher             = "200"

    # Deregistration delay allows graceful shutdown
    deregistration_delay = 30
  }

  # Stickiness - maintain session if needed
  stickiness {
    type            = "lb_cookie"
    cookie_duration = 86400
    enabled         = false
  }
}
```

### Pattern 4: HAProxy

```
global
  log stdout local0
  maxconn 2048

defaults
  mode http
  log global
  retries 3
  timeout connect 5000ms
  timeout client 50000ms
  timeout server 50000ms

frontend fraiseql_fe
  bind *:80
  default_backend fraiseql_be

backend fraiseql_be
  # Health check using /ready endpoint
  option httpchk GET /ready HTTP/1.1

  # Server configuration
  server fraiseql-1 fraiseql-1:8000 check inter 10s fall 3 rise 2
  server fraiseql-2 fraiseql-2:8000 check inter 10s fall 3 rise 2
  server fraiseql-3 fraiseql-3:8000 check inter 10s fall 3 rise 2

  # Load balancing algorithm
  balance roundrobin

  # Session persistence
  cookie SERVERID insert indirect nocache
```

---

## Probe Semantics

### Startup vs. Liveness vs. Readiness

| Scenario | Startup | Readiness | Liveness | Action |
|----------|---------|-----------|----------|--------|
| App booting | Failing | Failing | Passing | Wait (don't restart) |
| Ready to serve | Passing | Passing | Passing | Accept traffic |
| Database down | Passing | Failing | Passing | Remove from LB (don't restart) |
| App hung | Passing | Passing | Failing | Restart pod |
| Graceful shutdown | Passing | Failing | Passing | Wait for drain |

### Failure Thresholds

**Readiness Probe**:
- `failureThreshold: 3` means remove after 3 consecutive failures
- At 10-second intervals = 30 seconds to remove from LB
- Good for: Database maintenance, brief unavailability

**Liveness Probe**:
- `failureThreshold: 3` means restart after 3 consecutive failures
- At 10-second intervals = 30 seconds before restart
- Good for: Detecting hung processes that don't respond

---

## Troubleshooting

### Issue: Readiness Probe Failing

**Symptoms**:
- Pod not added to service
- Cannot reach service from other pods
- Health check returns 503

**Diagnosis**:
```bash
# Check logs for error message
kubectl logs deployment/fraiseql -f

# Manually test readiness endpoint
kubectl exec -it pod/fraiseql-xxx -- \
  curl -v http://localhost:8000/ready

# Check database connectivity
kubectl exec -it pod/fraiseql-xxx -- \
  sh -c 'psql $DATABASE_URL -c "SELECT 1"'
```

**Solutions**:
1. Check database URL configuration: `echo $DATABASE_URL`
2. Verify database is accessible: `psql $DATABASE_URL`
3. Check database credentials
4. Verify database server is running and reachable
5. Check firewall rules allow port 5432 (or configured DB port)
6. Review startup logs for configuration errors

### Issue: Liveness Probe Failing (Pod Restarting)

**Symptoms**:
- Pod keeps restarting
- `kubectl get pod` shows `CrashLoopBackOff` or restarting
- Liveness probe returns 5xx error

**Diagnosis**:
```bash
# Check pod restart count
kubectl describe pod/fraiseql-xxx

# Check recent logs before crash
kubectl logs pod/fraiseql-xxx --previous

# Check events
kubectl describe pod/fraiseql-xxx
```

**Solutions**:
1. Increase `initialDelaySeconds` if app takes time to start
2. Check for memory/CPU limits causing throttling
3. Look for hanging transactions in logs
4. Check database connection pool exhaustion
5. Monitor CPU/memory usage during failure

### Issue: Health Checks Timing Out

**Symptoms**:
- Probe timeout errors
- Liveness probe failing but `/live` works manually

**Diagnosis**:
```bash
# Test endpoint manually
curl -v http://localhost:8000/live

# Check response time
time curl http://localhost:8000/live

# Monitor server metrics during health checks
# Look for:
# - High CPU usage
# - Database query locks
# - Memory pressure
```

**Solutions**:
1. Increase `timeoutSeconds` if network is slow
2. Increase container resources (CPU/memory limits)
3. Optimize database queries
4. Check for database connection pool exhaustion
5. Monitor for high CPU/memory usage

---

## Best Practices

### 1. Health Check Intervals

**Recommended Intervals** (Kubernetes):
- `/ready`: 10 second interval (detect failures quickly)
- `/live`: 10 second interval (restart dead processes)
- `initialDelaySeconds`: 5-30 (depends on startup time)

**Trade-offs**:
- Shorter intervals = Faster failure detection, more overhead
- Longer intervals = Less overhead, slower failure response

### 2. Timeout and Threshold Settings

```yaml
# Conservative (slow but stable)
periodSeconds: 30
timeoutSeconds: 5
failureThreshold: 3
# Allows up to 90 seconds (3 * 30) before action

# Aggressive (fast but risky)
periodSeconds: 5
timeoutSeconds: 1
failureThreshold: 2
# Reacts in 10 seconds (2 * 5) but may flap
```

### 3. Graceful Shutdown Coordination

```yaml
terminationGracePeriodSeconds: 30  # Matches server timeout
```

Ensure these align:
1. Server's request timeout: 30s
2. Container's termination grace period: 30s
3. Load balancer's connection drain: 30s

### 4. Monitoring Health Checks

Monitor the health check endpoints themselves:

```yaml
# Prometheus alerts
- alert: HighReadinessFailureRate
  expr: rate(readiness_failures_total[5m]) > 0.1
  for: 2m

- alert: HighLivenessRestartRate
  expr: increase(pod_restarts_total[5m]) > 0
  for: 1m
```

### 5. Testing Health Checks

```bash
# Test manually before deployment
curl -v http://localhost:8000/health
curl -v http://localhost:8000/ready
curl -v http://localhost:8000/live

# Test with timeout
timeout 1 curl http://localhost:8000/ready

# Test after stopping database
kill $(pidof postgres)
curl http://localhost:8000/ready  # Should return 503
```

---

## Common Configurations

### High-Reliability Cluster

```yaml
# For mission-critical services
livenessProbe:
  periodSeconds: 30
  failureThreshold: 2
  # 60 seconds to detect and restart

readinessProbe:
  periodSeconds: 5
  failureThreshold: 2
  # 10 seconds to detect and remove from LB
```

### Development Environment

```yaml
# For rapid iteration
livenessProbe:
  periodSeconds: 60
  failureThreshold: 5

readinessProbe:
  periodSeconds: 30
  failureThreshold: 3
```

### Gradual Rollout

```yaml
# For canary deployments
readinessProbe:
  periodSeconds: 10
  initialDelaySeconds: 30
  # Allows 30 seconds before first check
  # Checks every 10 seconds after
```

---

## References

- [Kubernetes Liveness, Readiness, and Startup Probes](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/)
- [Docker Healthcheck](https://docs.docker.com/engine/reference/builder/#healthcheck)
- [AWS Load Balancer Health Checks](https://docs.aws.amazon.com/elasticloadbalancing/latest/application/target-health-checks.html)
- [FraiseQL OPERATIONS_GUIDE.md](./OPERATIONS_GUIDE.md)

---

**Last Updated**: 2026-01-31
