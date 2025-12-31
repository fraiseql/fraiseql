# Rate Limiting Triggered Runbook

**Last Updated**: 2025-12-29
**Severity**: MEDIUM
**MTTR Target**: 10 minutes

---

## 📋 Overview

This runbook guides you through investigating and responding to rate limiting events in FraiseQL applications. Rate limiting protects your application from abuse, but legitimate users may occasionally trigger limits.

---

## 🚨 Symptoms

### Primary Indicators
- Users receiving 429 Too Many Requests errors
- Increase in rate limit violations
- Spike in blocked requests
- Legitimate traffic patterns triggering limits

### Prometheus Metrics to Monitor

```promql
# Rate limit violations
rate(fraiseql_rate_limit_exceeded_total[5m])

# Blocked requests percentage
(rate(fraiseql_rate_limit_exceeded_total[5m])
/ rate(fraiseql_http_requests_total[5m])) * 100

# Rate limit violations by user
topk(10, sum by (user_id) (fraiseql_rate_limit_exceeded_total))
```

### Structured Logs Examples

```json
{
  "timestamp": "2025-12-29T12:30:45.123Z",
  "level": "WARNING",
  "event": "security.rate_limit",
  "message": "Rate limit exceeded for user",
  "context": {
    "user_id": "user_789",
    "ip_address": "203.0.113.42",
    "endpoint": "/graphql",
    "limit": 100,
    "window_seconds": 60,
    "current_count": 105,
    "exceeded_by": 5
  },
  "trace_id": "trace_rate123"
}
```

```json
{
  "timestamp": "2025-12-29T12:31:15.456Z",
  "level": "ERROR",
  "event": "security.rate_limit_abuse",
  "message": "Potential abuse detected - excessive rate limit violations",
  "context": {
    "user_id": "user_999",
    "ip_address": "198.51.100.23",
    "violations_count": 50,
    "time_window_minutes": 5,
    "action": "temporary_block"
  },
  "trace_id": "trace_abuse456"
}
```

---

## 🔍 Diagnostic Steps

### Step 1: Identify Affected Users

**Via Prometheus**:
```promql
# Top users triggering rate limits
topk(10,
  sum by (user_id) (
    rate(fraiseql_rate_limit_exceeded_total[5m])
  )
)

# Top IP addresses
topk(10,
  sum by (ip_address) (
    rate(fraiseql_rate_limit_exceeded_total[5m])
  )
)
```

**Via Structured Logs**:
```bash
# Find rate limit events in last hour
jq -r 'select(.event == "security.rate_limit") |
  "\(.timestamp) \(.context.user_id) \(.context.ip_address) \(.context.current_count)/\(.context.limit)"' \
  /var/log/fraiseql/app.log | tail -100

# Count violations per user
jq -r 'select(.event == "security.rate_limit") | .context.user_id' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn | head -20

# Count violations per IP
jq -r 'select(.event == "security.rate_limit") | .context.ip_address' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn | head -20
```

### Step 2: Analyze Traffic Patterns

**Check Request Rate**:
```promql
# Requests per second by endpoint
sum by (endpoint) (
  rate(fraiseql_http_requests_total[5m])
)

# Requests per second by user
sum by (user_id) (
  rate(fraiseql_http_requests_total[5m])
)
```

**Check Request Distribution**:
```bash
# Timeline of rate limit events
jq -r 'select(.event == "security.rate_limit") |
  "\(.timestamp) \(.context.user_id)"' \
  /var/log/fraiseql/app.log | \
  cut -d'T' -f2 | cut -d':' -f1-2 | uniq -c

# Example output:
#  15 12:30  # 15 events at 12:30
#  42 12:31  # 42 events at 12:31 (spike!)
#   8 12:32  # Back to normal
```

### Step 3: Classify Traffic Type

**Legitimate vs. Abuse**:
```bash
# Legitimate user pattern (gradual increase)
jq -r 'select(.event == "security.rate_limit" and .context.user_id == "user_789") |
  "\(.timestamp) \(.context.current_count)"' \
  /var/log/fraiseql/app.log

# Example legitimate:
# 12:30:10 85   # Approaching limit
# 12:30:45 101  # Slightly over
# 12:31:20 90   # Back under

# Abuse pattern (sustained high volume)
jq -r 'select(.event == "security.rate_limit" and .context.user_id == "user_999") |
  "\(.timestamp) \(.context.current_count)"' \
  /var/log/fraiseql/app.log

# Example abuse:
# 12:30:10 150  # Consistently over
# 12:30:15 145
# 12:30:20 152
# 12:30:25 148  # No reduction
```

**User Agent Analysis**:
```bash
# Check if rate-limited requests have unusual User-Agent
jq -r 'select(.event == "security.rate_limit") |
  .context.user_agent // "unknown"' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn

# Look for:
# - Bot/crawler agents
# - Automated tool signatures
# - Missing User-Agent headers
```

### Step 4: Review Current Rate Limits

**Check Configuration**:
```python
# Via application code
from fraiseql.security import RateLimiter

# Default limits (example)
limits = {
    "default": "100/minute",      # 100 requests per minute
    "authenticated": "500/minute", # Higher for authenticated users
    "graphql": "50/minute",       # GraphQL-specific limit
}
```

**Via Environment Variables**:
```bash
# Check current configuration
env | grep RATE_LIMIT

# Example output:
# FRAISEQL_RATE_LIMIT_DEFAULT=100/minute
# FRAISEQL_RATE_LIMIT_GRAPHQL=50/minute
# FRAISEQL_RATE_LIMIT_BURST=20
```

---

## 🔧 Resolution Steps

### For Legitimate Users (5 minutes)

#### 1. Temporarily Increase User Limit

```python
from fraiseql.security import RateLimiter

# Grant temporary exemption
rate_limiter = RateLimiter()
await rate_limiter.set_user_limit(
    user_id="user_789",
    limit=200,  # Increased from 100
    duration_seconds=3600,  # 1 hour
)
```

**Via Admin API** (if implemented):
```bash
curl -X POST http://localhost:8000/admin/rate-limits \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user_789",
    "limit": 200,
    "duration_seconds": 3600
  }'
```

#### 2. Whitelist Specific IP (If Safe)

```python
from fraiseql.security import RateLimiter

# Whitelist trusted IP
rate_limiter = RateLimiter()
await rate_limiter.add_to_whitelist(
    ip_address="203.0.113.42",
    reason="Legitimate high-volume integration",
    expiry_seconds=86400,  # 24 hours
)
```

**Configuration File**:
```yaml
# config/rate_limits.yml
whitelisted_ips:
  - ip: "203.0.113.42"
    reason: "Partner API integration"
    expires_at: "2025-12-30T12:00:00Z"
```

#### 3. Notify User

```python
# Send notification about rate limit
from fraiseql.notifications import send_notification

await send_notification(
    user_id="user_789",
    type="rate_limit_info",
    message="You've temporarily hit our rate limit. We've increased your limit for the next hour.",
    details={
        "current_limit": 200,
        "expires_at": "2025-12-29T13:30:00Z"
    }
)
```

### For Abusive Traffic (10 minutes)

#### 1. Temporary Block

```python
from fraiseql.security import RateLimiter

# Block user temporarily
rate_limiter = RateLimiter()
await rate_limiter.block_user(
    user_id="user_999",
    duration_seconds=3600,  # 1 hour
    reason="Excessive rate limit violations"
)
```

**Via Admin API**:
```bash
curl -X POST http://localhost:8000/admin/blocks \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user_999",
    "duration_seconds": 3600,
    "reason": "Excessive rate limit violations"
  }'
```

#### 2. IP-Based Blocking

```python
# Block IP address
await rate_limiter.block_ip(
    ip_address="198.51.100.23",
    duration_seconds=7200,  # 2 hours
    reason="Suspected bot activity"
)
```

**Firewall Rule** (if persistent abuse):
```bash
# Add iptables rule (requires root)
sudo iptables -A INPUT -s 198.51.100.23 -j DROP

# Or use cloud provider firewall
# AWS Security Group, GCP Firewall Rule, etc.
```

#### 3. CAPTCHA Challenge (If Available)

```python
# Require CAPTCHA for suspicious users
from fraiseql.security import CaptchaChallenge

await CaptchaChallenge.enable_for_user(
    user_id="user_999",
    duration_seconds=3600
)
```

### Adjusting Global Rate Limits (30 minutes)

#### 1. Analyze Legitimate Traffic Patterns

```promql
# 95th percentile request rate (legitimate users)
histogram_quantile(0.95,
  sum by (le) (
    rate(fraiseql_http_requests_total{
      status!="429"
    }[1h])
  )
)
```

```bash
# Average requests per user per minute
jq -r 'select(.event == "http.request" and .context.status != 429) |
  "\(.context.user_id) \(.timestamp)"' \
  /var/log/fraiseql/app.log | \
  awk '{print $1, substr($2,1,16)}' | \
  sort | uniq -c | \
  awk '{sum+=$1; count++} END {print sum/count}'
```

#### 2. Update Rate Limit Configuration

```python
# config/rate_limits.py
RATE_LIMITS = {
    # Increase if legitimate traffic is being blocked
    "default": "150/minute",  # Increased from 100

    # Different limits for different user tiers
    "free_tier": "50/minute",
    "pro_tier": "500/minute",
    "enterprise_tier": "5000/minute",

    # Endpoint-specific limits
    "graphql_query": "100/minute",
    "graphql_mutation": "50/minute",

    # Burst allowance
    "burst": 30,  # Allow short bursts above limit
}
```

**Environment Variables**:
```bash
# Update production configuration
export FRAISEQL_RATE_LIMIT_DEFAULT=150/minute
export FRAISEQL_RATE_LIMIT_GRAPHQL_QUERY=100/minute
export FRAISEQL_RATE_LIMIT_BURST=30
```

#### 3. Implement Tiered Rate Limits

```python
from fraiseql.security import RateLimiter

class TieredRateLimiter(RateLimiter):
    async def get_user_limit(self, user_id: str) -> int:
        # Check user's subscription tier
        user = await get_user(user_id)

        limits = {
            "free": 50,
            "pro": 500,
            "enterprise": 5000,
        }

        return limits.get(user.tier, 50)  # Default to free tier
```

---

## 📊 Monitoring & Alerts

### Prometheus Alert Rules

```yaml
# alerts/rate_limiting.yml
groups:
  - name: rate_limiting
    interval: 30s
    rules:
      - alert: HighRateLimitViolations
        expr: |
          rate(fraiseql_rate_limit_exceeded_total[5m]) > 5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High rate of rate limit violations"
          description: "{{ $value }} violations/sec in last 5 minutes"

      - alert: RateLimitAbuseDetected
        expr: |
          sum by (user_id) (
            rate(fraiseql_rate_limit_exceeded_total[5m])
          ) > 10
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Potential abuse detected for user {{ $labels.user_id }}"
          description: "User triggering {{ $value }} violations/sec"

      - alert: RateLimitingImpactingUsers
        expr: |
          (rate(fraiseql_rate_limit_exceeded_total[5m])
          / rate(fraiseql_http_requests_total[5m])) * 100 > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Rate limiting affecting {{ $value }}% of requests"
          description: "Consider increasing limits or investigating abuse"
```

### Grafana Dashboard Panels

**1. Rate Limit Violations Timeline**:
```promql
rate(fraiseql_rate_limit_exceeded_total[5m])
```

**2. Top Violators (Users)**:
```promql
topk(10,
  sum by (user_id) (fraiseql_rate_limit_exceeded_total)
)
```

**3. Top Violators (IPs)**:
```promql
topk(10,
  sum by (ip_address) (fraiseql_rate_limit_exceeded_total)
)
```

**4. Percentage of Blocked Requests**:
```promql
(rate(fraiseql_rate_limit_exceeded_total[5m])
/ rate(fraiseql_http_requests_total[5m])) * 100
```

---

## 🔍 Verification

After applying fixes, verify rate limiting is working correctly:

### 1. Check Violation Rate

```promql
# Should decrease after adjustments
rate(fraiseql_rate_limit_exceeded_total[5m])
```

### 2. Verify User Can Access

```bash
# Test as affected user
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer $USER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name } }"}'

# Should return 200, not 429
```

### 3. Monitor Logs

```bash
# Check for continued violations
jq -r 'select(.event == "security.rate_limit" and .context.user_id == "user_789")' \
  /var/log/fraiseql/app.log | tail -10

# Should see no recent events for whitelisted/increased limit users
```

---

## 📝 Post-Incident Review

After resolving the incident:

1. **Analyze Root Cause**:
   - Legitimate traffic spike?
   - Misconfigured limits?
   - Actual abuse attempt?
   - Integration misconfiguration?

2. **Update Rate Limits**:
   - Adjust limits based on real traffic patterns
   - Implement tiered limits for different user types
   - Add burst allowance for short spikes

3. **Improve Detection**:
   - Add monitoring for specific endpoints
   - Implement user behavior analytics
   - Set up automated abuse detection

4. **User Communication**:
   - Document rate limits in API docs
   - Add rate limit headers to responses
   - Provide user-facing rate limit dashboard

---

## 💡 Best Practices

### 1. Return Rate Limit Headers

```python
from fastapi import Response

@app.post("/graphql")
async def graphql_endpoint(response: Response):
    # Add rate limit headers
    response.headers["X-RateLimit-Limit"] = "100"
    response.headers["X-RateLimit-Remaining"] = "45"
    response.headers["X-RateLimit-Reset"] = "1640000000"

    # ... handle request ...
```

### 2. Implement Sliding Window

```python
from fraiseql.security import SlidingWindowRateLimiter

# More accurate than fixed window
rate_limiter = SlidingWindowRateLimiter(
    limit=100,
    window_seconds=60,
)
```

### 3. Use Distributed Rate Limiting

```python
from fraiseql.security import RedisRateLimiter

# For multi-instance deployments
rate_limiter = RedisRateLimiter(
    redis_url="redis://localhost:6379",
    limit=100,
    window_seconds=60,
)
```

### 4. Graceful Degradation

```python
@app.post("/graphql")
async def graphql_endpoint():
    if await rate_limiter.is_limited(user_id):
        # Return cached result instead of hard error
        cached_result = await cache.get(request_hash)
        if cached_result:
            return cached_result

    # Normal processing
    return await execute_query(...)
```

---

## 📚 Related Resources

- [Security Configuration](../security.md#rate-limiting)
- [User Authentication](./authentication-failures.md)
- [GraphQL Query DoS Protection](./graphql-query-dos.md)

---

## 🆘 Escalation

If issue persists after following this runbook:

1. **Gather Evidence**:
   - Rate limit violation graphs (last 24 hours)
   - Top violators list (users and IPs)
   - Traffic pattern analysis
   - Sample of rate-limited requests

2. **Escalate To**:
   - Security Team (for abuse investigation)
   - Product Team (for limit adjustment decisions)
   - Engineering Team (for implementation changes)

3. **Emergency Contact**:
   - Security On-call: [Contact info]
   - Product Manager: [Contact info]
   - Engineering Manager: [Contact info]

---

**Version**: 1.0
**Last Tested**: 2025-12-29
**Next Review**: 2026-03-29
