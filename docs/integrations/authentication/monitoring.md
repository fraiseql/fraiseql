<!-- Skip to main content -->
---

title: Authentication Monitoring and Observability
description: This guide covers monitoring, logging, and observability for FraiseQL's authentication system.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# Authentication Monitoring and Observability

This guide covers monitoring, logging, and observability for FraiseQL's authentication system.

## Overview

FraiseQL provides built-in support for:

- Structured event logging
- Metrics collection
- Performance monitoring
- Error tracking

## Structured Logging

All authentication events are logged with structured data for easy analysis.

### Enabling Structured Logging

```rust
<!-- Code example in RUST -->
use fraiseql_server::auth::AuthEvent;
use tracing_subscriber;

fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(true)
        .init();

    // Now all auth events are automatically logged
}
```text
<!-- Code example in TEXT -->

### Log Format (JSON)

```json
<!-- Code example in JSON -->
{
  "timestamp": "2026-01-21T10:30:45Z",
  "level": "INFO",
  "event": "token_validated",
  "user_id": "user123",
  "provider": "google",
  "status": "success",
  "duration_ms": 2.5,
  "request_id": "req-abc123"
}
```text
<!-- Code example in TEXT -->

### Logging Auth Events

```rust
<!-- Code example in RUST -->
use fraiseql_server::auth::AuthEvent;

let event = AuthEvent::new("login")
    .with_user_id("user123".to_string())
    .with_provider("google".to_string())
    .with_request_id("req-abc123".to_string())
    .success(50.0);

event.log();
```text
<!-- Code example in TEXT -->

## Metrics Collection

Track authentication metrics for monitoring and alerting:

```rust
<!-- Code example in RUST -->
use fraiseql_server::auth::AuthMetrics;
use std::sync::Arc;
use std::sync::Mutex;

let metrics = Arc::new(Mutex::new(AuthMetrics::new()));

// Track authentication attempt
{
    let mut m = metrics.lock().unwrap();
    m.record_attempt();
    // ... auth logic
    m.record_success();
}

// Get metrics
{
    let m = metrics.lock().unwrap();
    println!("Success rate: {:.2}%", m.success_rate());
    println!("Total attempts: {}", m.total_auth_attempts);
}
```text
<!-- Code example in TEXT -->

### Available Metrics

```rust
<!-- Code example in RUST -->
pub struct AuthMetrics {
    pub total_auth_attempts: u64,           // Total login attempts
    pub successful_authentications: u64,    // Successful logins
    pub failed_authentications: u64,        // Failed logins
    pub tokens_issued: u64,                 // New tokens issued
    pub tokens_refreshed: u64,              // Tokens refreshed
    pub sessions_revoked: u64,              // Sessions revoked
}
```text
<!-- Code example in TEXT -->

### Accessing Metrics via HTTP

Create a metrics endpoint:

```rust
<!-- Code example in RUST -->
use axum::response::Json;

async fn metrics_handler(
    State(metrics): State<Arc<Mutex<AuthMetrics>>>,
) -> Json<AuthMetrics> {
    let m = metrics.lock().unwrap().clone();
    Json(m)
}

app.route("/metrics/auth", get(metrics_handler))
```text
<!-- Code example in TEXT -->

Response:

```bash
<!-- Code example in BASH -->
curl http://localhost:8000/metrics/auth

{
  "total_auth_attempts": 100,
  "successful_authentications": 95,
  "failed_authentications": 5,
  "tokens_issued": 95,
  "tokens_refreshed": 42,
  "sessions_revoked": 38
}
```text
<!-- Code example in TEXT -->

## Performance Monitoring

### Operation Timers

Measure operation duration:

```rust
<!-- Code example in RUST -->
use fraiseql_server::auth::OperationTimer;

async fn auth_callback() -> Result<impl IntoResponse> {
    let timer = OperationTimer::start("oauth_exchange");

    // ... OAuth logic

    let duration_ms = timer.elapsed_ms();
    // Logs: "Operation completed: oauth_exchange (45.2ms)"
    Ok(response)
}
```text
<!-- Code example in TEXT -->

### Expected Performance

| Operation | Duration | Alert Threshold |
|-----------|----------|-----------------|
| JWT Validation | 1-5ms | > 10ms |
| Session Lookup | 5-50ms | > 100ms |
| OAuth Token Exchange | 200-500ms | > 1000ms |
| User Info Retrieval | 100-300ms | > 500ms |

## Alerting Rules

### Prometheus Alerts

Create `alerts.yml`:

```yaml
<!-- Code example in YAML -->
groups:
  - name: fraiseql_auth
    interval: 30s
    rules:
      # High failure rate
      - alert: AuthHighFailureRate
        expr: |
          (fraiseql_auth_failures_total / fraiseql_auth_attempts_total) > 0.1
        for: 5m
        annotations:
          summary: "High authentication failure rate"
          description: "Auth failure rate > 10% for 5 minutes"

      # Slow validation
      - alert: SlowTokenValidation
        expr: |
          histogram_quantile(0.99, fraiseql_auth_validation_duration_ms) > 10
        for: 5m
        annotations:
          summary: "Token validation is slow"
          description: "p99 validation latency > 10ms"

      # High session revocation rate
      - alert: HighSessionRevocation
        expr: |
          increase(fraiseql_sessions_revoked_total[5m]) > 100
        annotations:
          summary: "Many sessions being revoked"
          description: "More than 100 sessions revoked in 5 minutes"
```text
<!-- Code example in TEXT -->

## Grafana Dashboard

Import the dashboard configuration:

### Dashboard JSON

```json
<!-- Code example in JSON -->
{
  "dashboard": {
    "title": "FraiseQL Authentication",
    "panels": [
      {
        "title": "Authentication Attempts",
        "targets": [
          {
            "expr": "rate(fraiseql_auth_attempts_total[5m])"
          }
        ]
      },
      {
        "title": "Success Rate",
        "targets": [
          {
            "expr": "fraiseql_auth_success_rate"
          }
        ]
      },
      {
        "title": "Token Validation Latency",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, fraiseql_auth_validation_duration_ms)"
          }
        ]
      },
      {
        "title": "OAuth Exchange Duration",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, fraiseql_oauth_exchange_duration_ms)"
          }
        ]
      }
    ]
  }
}
```text
<!-- Code example in TEXT -->

## Log Analysis

### Common Log Queries

**Failed authentication attempts:**

```bash
<!-- Code example in BASH -->
# In ELK, Datadog, or similar
status: "error" AND event: "token_validation"
```text
<!-- Code example in TEXT -->

**Slow OAuth exchanges:**

```bash
<!-- Code example in BASH -->
event: "oauth_exchange" AND duration_ms: > 500
```text
<!-- Code example in TEXT -->

**User lockout detection:**

```bash
<!-- Code example in BASH -->
user_id: "user123" AND status: "error" AND event: "login"
| stats count by user_id
```text
<!-- Code example in TEXT -->

## Health Checks

Create a health check endpoint:

```rust
<!-- Code example in RUST -->
use axum::response::Json;

#[derive(Serialize)]
struct HealthStatus {
    auth: String,
    db: String,
    oauth_provider: String,
}

async fn health_check(
    State(auth_state): State<AuthState>,
) -> Json<HealthStatus> {
    let oauth_ok = check_oauth_provider(&auth_state).await;
    let db_ok = check_database(&auth_state).await;

    Json(HealthStatus {
        auth: "healthy".to_string(),
        oauth_provider: if oauth_ok { "ok" } else { "error" }.to_string(),
        db: if db_ok { "ok" } else { "error" }.to_string(),
    })
}

app.route("/health/auth", get(health_check))
```text
<!-- Code example in TEXT -->

Health check response:

```bash
<!-- Code example in BASH -->
curl http://localhost:8000/health/auth

{
  "auth": "healthy",
  "oauth_provider": "ok",
  "db": "ok"
}
```text
<!-- Code example in TEXT -->

## Docker Compose with Monitoring

```yaml
<!-- Code example in YAML -->
version: '3.8'
services:
  FraiseQL:
    image: FraiseQL/server:latest
    environment:
      RUST_LOG: info,fraiseql_server::auth=debug
    ports:
      - "8000:8000"

  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
    ports:
      - "9090:9090"

  grafana:
    image: grafana/grafana:latest
    environment:
      GF_SECURITY_ADMIN_PASSWORD: admin
    ports:
      - "3000:3000"

  loki:
    image: grafana/loki:latest
    ports:
      - "3100:3100"

  promtail:
    image: grafana/promtail:latest
    volumes:
      - /var/log:/var/log
    command: -config.file=/etc/promtail/config.yml
```text
<!-- Code example in TEXT -->

## Best Practices

1. **Log in JSON format** for easy parsing
2. **Include request ID** in logs for tracing
3. **Monitor success rates** continuously
4. **Alert on anomalies** (sudden spikes/drops)
5. **Track latency percentiles** (p50, p95, p99)
6. **Audit sensitive events** (login, logout, admin actions)
7. **Retain logs** for compliance (90+ days)
8. **Anonymize PII** in logs (don't log passwords, tokens)
9. **Set up dashboards** for on-call teams
10. **Review logs regularly** for security incidents

## Troubleshooting with Logs

### User Can't Log In

Check logs for:

```text
<!-- Code example in TEXT -->
error: "invalid_state"  # State validation failed
error: "oauth_error"    # Provider error
error: "token_expired"  # Token already expired
```text
<!-- Code example in TEXT -->

### Slow Authentication

Check performance logs:

```text
<!-- Code example in TEXT -->
duration_ms: > 500  # Identify slow operations
event: "oauth_exchange"  # Likely provider latency
```text
<!-- Code example in TEXT -->

### Session Issues

Check session logs:

```text
<!-- Code example in TEXT -->
event: "session_revoked"  # Track revocations
event: "session_created"  # Track creation rate
```text
<!-- Code example in TEXT -->

## Metrics Integration

### Prometheus Integration

```rust
<!-- Code example in RUST -->
use prometheus::{Counter, Histogram, Registry};

pub struct AuthPrometheus {
    attempts: Counter,
    successes: Counter,
    failures: Counter,
    validation_duration: Histogram,
}

impl AuthPrometheus {
    pub fn new(registry: &Registry) -> Result<Self> {
        let attempts = Counter::new("fraiseql_auth_attempts_total", "Total auth attempts")?;
        let successes = Counter::new("fraiseql_auth_successes_total", "Successful auths")?;
        let failures = Counter::new("fraiseql_auth_failures_total", "Failed auths")?;
        let validation_duration = Histogram::new(
            "fraiseql_auth_validation_duration_ms",
            "Token validation duration",
        )?;

        registry.register(Box::new(attempts.clone()))?;
        registry.register(Box::new(successes.clone()))?;
        registry.register(Box::new(failures.clone()))?;
        registry.register(Box::new(validation_duration.clone()))?;

        Ok(Self {
            attempts,
            successes,
            failures,
            validation_duration,
        })
    }
}
```text
<!-- Code example in TEXT -->

## See Also

- [Deployment Guide](./deployment.md)
- [Security Checklist](./security-checklist.md)
- [API Reference](./api-reference.md)

---

**Next Step**: Set up monitoring dashboard and alerts for your deployment.
