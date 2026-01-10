# Authentication Failures Runbook

**Last Updated**: 2025-12-29
**Severity**: HIGH
**MTTR Target**: 15 minutes

---

## 📋 Overview

This runbook guides you through diagnosing and resolving authentication failures in FraiseQL applications. Authentication issues can indicate security attacks, misconfiguration, or service degradation.

---

## 🚨 Symptoms

### Primary Indicators
- Spike in 401 Unauthorized errors
- Increase in 403 Forbidden errors
- Valid users unable to access API
- Token validation failures
- Authentication service degradation

### Prometheus Metrics to Monitor

```promql
# Authentication failure rate
rate(fraiseql_auth_failures_total[5m])

# Failed logins by reason
sum by (failure_reason) (
  rate(fraiseql_auth_failures_total[5m])
)

# Percentage of failed auth requests
(rate(fraiseql_auth_failures_total[5m])
/ rate(fraiseql_http_requests_total[5m])) * 100

# Failed attempts by user (brute force indicator)
topk(10,
  sum by (user_id) (fraiseql_auth_failures_total)
)
```

### Structured Logs Examples

```json
{
  "timestamp": "2025-12-29T14:20:35.123Z",
  "level": "WARNING",
  "event": "security.auth_failure",
  "message": "Authentication failed - invalid token",
  "context": {
    "user_id": null,
    "ip_address": "198.51.100.75",
    "failure_reason": "invalid_token",
    "token_prefix": "eyJhbG...",
    "endpoint": "/graphql",
    "user_agent": "Mozilla/5.0"
  },
  "trace_id": "trace_auth123"
}
```

```json
{
  "timestamp": "2025-12-29T14:21:10.456Z",
  "level": "ERROR",
  "event": "security.brute_force_detected",
  "message": "Possible brute force attack detected",
  "context": {
    "user_id": "user_target",
    "ip_address": "203.0.113.88",
    "failed_attempts": 15,
    "time_window_seconds": 300,
    "action": "account_locked"
  },
  "trace_id": "trace_brute456"
}
```

```json
{
  "timestamp": "2025-12-29T14:22:05.789Z",
  "level": "WARNING",
  "event": "security.token_expired",
  "message": "Token expired",
  "context": {
    "user_id": "user_123",
    "ip_address": "192.0.2.100",
    "token_issued_at": "2025-12-29T12:22:05Z",
    "token_expires_at": "2025-12-29T14:22:05Z",
    "current_time": "2025-12-29T14:22:05Z"
  },
  "trace_id": "trace_expired789"
}
```

---

## 🔍 Diagnostic Steps

### Step 1: Classify Authentication Failures

**Via Prometheus - Failure Reasons**:
```promql
# Group failures by reason
sum by (failure_reason) (
  rate(fraiseql_auth_failures_total[5m])
)
```

**Via Structured Logs**:
```bash
# Count failures by reason
jq -r 'select(.event == "security.auth_failure") |
  .context.failure_reason' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn

# Example output:
#  450 invalid_token
#  120 token_expired
#   45 missing_token
#   30 invalid_signature
#   15 user_not_found
```

**Common Failure Reasons**:
- `invalid_token`: Malformed or tampered token
- `token_expired`: Valid token but expired
- `invalid_signature`: Token signature mismatch
- `missing_token`: No auth header provided
- `user_not_found`: Valid token but user doesn't exist
- `user_disabled`: User account disabled
- `insufficient_permissions`: Valid auth but lacks permissions (403)

### Step 2: Identify Affected Users

**Via Prometheus**:
```promql
# Top users with auth failures
topk(10,
  sum by (user_id) (fraiseql_auth_failures_total)
)

# Failures by IP address
topk(10,
  sum by (ip_address) (fraiseql_auth_failures_total)
)
```

**Via Structured Logs**:
```bash
# Failures by user
jq -r 'select(.event == "security.auth_failure") |
  .context.user_id // "anonymous"' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn | head -20

# Failures by IP
jq -r 'select(.event == "security.auth_failure") |
  .context.ip_address' \
  /var/log/fraiseql/app.log | sort | uniq -c | sort -rn | head -20

# Failures timeline
jq -r 'select(.event == "security.auth_failure") |
  "\(.timestamp) \(.context.failure_reason) \(.context.user_id // "anon")"' \
  /var/log/fraiseql/app.log | tail -50
```

### Step 3: Determine Root Cause Category

#### A. Token Expiration Issues

**Symptoms**:
- Many `token_expired` failures
- Concentrated around specific time period
- Affects multiple users simultaneously

**Check**:
```bash
# Count expired tokens
jq -r 'select(.event == "security.token_expired")' \
  /var/log/fraiseql/app.log | wc -l

# Check token lifetime configuration
echo $FRAISEQL_TOKEN_LIFETIME_SECONDS
```

**Likely Causes**:
- Token lifetime too short
- Frontend not refreshing tokens
- Clock skew between services

#### B. Token Signature Failures

**Symptoms**:
- Many `invalid_signature` failures
- Started after deployment or config change
- All tokens failing validation

**Check**:
```bash
# Check JWT signing key
echo $FRAISEQL_JWT_SECRET | wc -c  # Should be 32+ characters

# Check if key changed recently
git log --since="24 hours ago" --grep="JWT_SECRET" --oneline

# Verify key consistency across instances
# (All instances must use same secret!)
```

**Likely Causes**:
- JWT secret key mismatch between services
- Key rotation without grace period
- Configuration drift across instances

#### C. Brute Force Attack

**Symptoms**:
- Same IP trying multiple passwords
- Failed attempts for many different usernames
- Rapid succession of failures

**Check**:
```bash
# Detect brute force patterns
jq -r 'select(.event == "security.auth_failure") |
  .context.ip_address + " " + (.context.user_id // "anon") + " " + .timestamp' \
  /var/log/fraiseql/app.log | \
  sort | uniq | \
  cut -d' ' -f1 | uniq -c | sort -rn | head -10

# High count from single IP = brute force
```

**Likely Causes**:
- Credential stuffing attack
- Password spraying attack
- Compromised credential database

#### D. Service Degradation

**Symptoms**:
- All authentication failing suddenly
- Service health check failing
- External auth service unreachable

**Check**:
```bash
# Check auth service health
curl -s http://localhost:8000/health | jq .

# Check external OAuth provider (if used)
curl -s https://oauth.provider.com/.well-known/openid-configuration

# Check database connectivity
psql -c "SELECT 1" postgresql://localhost/fraiseql
```

**Likely Causes**:
- Database connectivity issues
- External OAuth provider down
- Redis/cache service unavailable
- Rate limiting on auth service

---

## 🔧 Resolution Steps

### For Token Expiration Issues (5-10 minutes)

#### 1. Increase Token Lifetime (If Too Short)

```python
# config/auth.py
TOKEN_LIFETIME_SECONDS = 3600  # Increase from 900 (15 min) to 3600 (1 hour)
REFRESH_TOKEN_LIFETIME_SECONDS = 86400  # 24 hours
```

**Environment Variable**:
```bash
export FRAISEQL_TOKEN_LIFETIME_SECONDS=3600
export FRAISEQL_REFRESH_TOKEN_LIFETIME_SECONDS=86400

# Restart application
systemctl restart fraiseql
```

#### 2. Implement Token Refresh Endpoint

```python
from fraiseql import FraiseQL
from fraiseql.auth import create_access_token, verify_refresh_token

app = FraiseQL()

@app.post("/auth/refresh")
async def refresh_token(refresh_token: str):
    # Verify refresh token
    payload = verify_refresh_token(refresh_token)
    if not payload:
        raise HTTPException(status_code=401, detail="Invalid refresh token")

    # Issue new access token
    new_access_token = create_access_token(
        user_id=payload["user_id"],
        expires_in=3600,
    )

    return {
        "access_token": new_access_token,
        "token_type": "bearer",
        "expires_in": 3600,
    }
```

#### 3. Implement Grace Period for Token Expiry

```python
from fraiseql.auth import TokenValidator

# Allow recently expired tokens (within 5 minutes)
validator = TokenValidator(
    grace_period_seconds=300,  # 5 minutes
)

# Tokens expired < 5 min ago still valid
```

### For Token Signature Issues (10-15 minutes)

#### 1. Verify JWT Secret Consistency

```bash
# Check current secret (all instances must match!)
echo $FRAISEQL_JWT_SECRET

# Update secret if needed (COORDINATED ROLLOUT REQUIRED)
export FRAISEQL_JWT_SECRET="new-secret-key-at-least-32-chars-long"

# Restart all instances simultaneously
kubectl rollout restart deployment fraiseql
```

#### 2. Implement Key Rotation with Grace Period

```python
from fraiseql.auth import JWTManager

# Support multiple keys during rotation
jwt_manager = JWTManager(
    primary_secret=os.getenv("FRAISEQL_JWT_SECRET"),
    fallback_secrets=[
        os.getenv("FRAISEQL_JWT_SECRET_OLD"),  # Accept old key during transition
    ],
    grace_period_hours=24,  # Accept old keys for 24 hours
)

# Sign with primary key
token = jwt_manager.encode(payload)

# Verify with primary OR fallback keys
payload = jwt_manager.decode(token)
```

**Rotation Process**:
```bash
# Step 1: Add new key as fallback (no restart needed)
export FRAISEQL_JWT_SECRET_NEW="new-key-here"

# Step 2: Wait 24 hours (all old tokens expire)

# Step 3: Promote new key to primary
export FRAISEQL_JWT_SECRET="$FRAISEQL_JWT_SECRET_NEW"
export FRAISEQL_JWT_SECRET_OLD="$FRAISEQL_JWT_SECRET"
unset FRAISEQL_JWT_SECRET_NEW

# Step 4: Restart application
kubectl rollout restart deployment fraiseql

# Step 5: Wait 24 hours, remove old key
unset FRAISEQL_JWT_SECRET_OLD
```

#### 3. Fix Clock Skew (If Present)

```bash
# Check clock skew between services
date -u  # On application server
date -u  # On auth service

# Sync time with NTP
sudo ntpdate -s time.nist.gov

# Enable NTP daemon
sudo systemctl enable ntpd
sudo systemctl start ntpd
```

### For Brute Force Attacks (5 minutes)

#### 1. Block Attacking IPs

```python
from fraiseql.security import BlockList

# Block IP immediately
await BlockList.add_ip(
    ip_address="203.0.113.88",
    duration_seconds=86400,  # 24 hours
    reason="Brute force attack detected"
)
```

**Via Firewall**:
```bash
# Block at firewall level (more efficient)
sudo iptables -A INPUT -s 203.0.113.88 -j DROP

# Or use cloud provider firewall
# AWS Security Group, GCP Firewall Rule, etc.
```

#### 2. Enable Account Lockout

```python
from fraiseql.auth import AccountLockout

# Lock account after 5 failed attempts
lockout = AccountLockout(
    max_attempts=5,
    lockout_duration_seconds=1800,  # 30 minutes
    window_seconds=300,  # 5-minute window
)

app.add_plugin(lockout)

# Locked accounts require admin unlock or automatic expiry
```

#### 3. Implement CAPTCHA for Failed Attempts

```python
from fraiseql.security import CaptchaChallenge

# Require CAPTCHA after 3 failed attempts
captcha = CaptchaChallenge(
    trigger_after_failures=3,
    captcha_provider="recaptcha",
    site_key=os.getenv("RECAPTCHA_SITE_KEY"),
    secret_key=os.getenv("RECAPTCHA_SECRET_KEY"),
)

app.add_plugin(captcha)
```

### For Service Degradation (15 minutes)

#### 1. Check External Dependencies

**OAuth Provider Health**:
```bash
# Check OAuth provider status
curl -s https://status.oauth-provider.com

# Test OAuth endpoints
curl -s https://oauth.provider.com/.well-known/openid-configuration
```

**Database Health**:
```bash
# Check database connectivity
psql -c "SELECT 1" $DATABASE_URL

# Check connection pool
curl -s http://localhost:8000/health/detailed | jq .database
```

**Cache/Redis Health**:
```bash
# Check Redis connectivity
redis-cli ping

# Check cache stats
curl -s http://localhost:8000/health/detailed | jq .cache
```

#### 2. Implement Fallback Authentication

```python
from fraiseql.auth import AuthManager

# Fallback to local auth if OAuth unavailable
auth_manager = AuthManager(
    primary_provider="oauth",
    fallback_provider="local",
    fallback_timeout_seconds=5,
)

@app.post("/auth/login")
async def login(credentials):
    try:
        # Try OAuth first
        return await auth_manager.authenticate(
            credentials,
            provider="oauth",
            timeout=5,
        )
    except TimeoutError:
        # Fallback to local auth
        return await auth_manager.authenticate(
            credentials,
            provider="local",
        )
```

#### 3. Implement Circuit Breaker

```python
from fraiseql.resilience import CircuitBreaker

# Prevent cascading failures
oauth_circuit = CircuitBreaker(
    failure_threshold=5,     # Open after 5 failures
    timeout_seconds=60,      # Stay open for 60 seconds
    expected_exception=ConnectionError,
)

@oauth_circuit.protected
async def oauth_authenticate(token):
    # This will fail fast if circuit is open
    return await oauth_provider.validate(token)
```

---

## 📊 Monitoring & Alerts

### Prometheus Alert Rules

```yaml
# alerts/authentication.yml
groups:
  - name: authentication
    interval: 30s
    rules:
      - alert: HighAuthFailureRate
        expr: |
          rate(fraiseql_auth_failures_total[5m]) > 5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High authentication failure rate"
          description: "{{ $value }} auth failures/sec"

      - alert: AuthenticationServiceDown
        expr: |
          (rate(fraiseql_auth_failures_total[5m])
          / rate(fraiseql_http_requests_total[5m])) > 0.5
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "50%+ of auth requests failing"
          description: "Possible auth service outage"

      - alert: BruteForceDetected
        expr: |
          sum by (ip_address) (
            rate(fraiseql_auth_failures_total[5m])
          ) > 10
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Brute force attack from {{ $labels.ip_address }}"
          description: "{{ $value }} failed attempts/sec"

      - alert: MassAccountLockout
        expr: |
          rate(fraiseql_account_lockouts_total[5m]) > 5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Multiple accounts being locked"
          description: "{{ $value }} accounts locked/sec - possible attack"
```

### Grafana Dashboard Panels

**1. Authentication Failure Rate**:
```promql
rate(fraiseql_auth_failures_total[5m])
```

**2. Failures by Reason**:
```promql
sum by (failure_reason) (
  rate(fraiseql_auth_failures_total[5m])
)
```

**3. Top Failing Users**:
```promql
topk(10,
  sum by (user_id) (fraiseql_auth_failures_total)
)
```

**4. Top Failing IPs**:
```promql
topk(10,
  sum by (ip_address) (fraiseql_auth_failures_total)
)
```

**5. Auth Success Rate**:
```promql
(rate(fraiseql_auth_success_total[5m])
/ (rate(fraiseql_auth_success_total[5m]) + rate(fraiseql_auth_failures_total[5m]))) * 100
```

---

## 🔍 Verification

After applying fixes, verify authentication is working:

### 1. Check Metrics

```promql
# Failure rate should decrease
rate(fraiseql_auth_failures_total[5m])

# Success rate should increase
rate(fraiseql_auth_success_total[5m])
```

### 2. Test Authentication

```bash
# Test login
curl -X POST http://localhost:8000/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "test", "password": "test123"}'

# Should return token
# {"access_token": "eyJ...", "token_type": "bearer"}

# Test authenticated request
curl http://localhost:8000/graphql \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"query": "{ users { id } }"}'

# Should return 200 OK
```

### 3. Check Logs

```bash
# Verify successful authentications
jq -r 'select(.event == "security.auth_success")' \
  /var/log/fraiseql/app.log | tail -10

# Verify no ongoing failures
jq -r 'select(.event == "security.auth_failure")' \
  /var/log/fraiseql/app.log | tail -10
```

---

## 📝 Post-Incident Review

After resolving the incident:

1. **Analyze Root Cause**:
   - Configuration issue?
   - External service degradation?
   - Security attack?
   - Code bug?

2. **Implement Preventive Measures**:
   - Add monitoring for early detection
   - Implement circuit breakers
   - Add fallback authentication
   - Improve key rotation process

3. **Security Improvements**:
   - Review token lifetime settings
   - Implement account lockout
   - Add brute force protection
   - Enable multi-factor authentication

4. **Update Documentation**:
   - Document token rotation process
   - Update troubleshooting guides
   - Share lessons learned

---

## 📚 Related Resources

- [Authentication Configuration](../security.md#authentication)
- [Rate Limiting](./rate-limiting-triggered.md)
- [Security Hardening](../security.md)

---

## 🆘 Escalation

If issue persists after following this runbook:

1. **Gather Evidence**:
   - Auth failure graphs (last 24 hours)
   - Top failing users/IPs
   - Sample failed requests
   - External service status

2. **Escalate To**:
   - Security Team (for attack investigation)
   - Platform Team (for service health)
   - Engineering Team (for code fixes)

3. **Emergency Contact**:
   - Security On-call: [Contact info]
   - Platform On-call: [Contact info]
   - Engineering Manager: [Contact info]

---

**Version**: 1.0
**Last Tested**: 2025-12-29
**Next Review**: 2026-03-29
