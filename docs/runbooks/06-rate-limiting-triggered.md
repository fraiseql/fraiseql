# Runbook: Rate Limiting Triggered

## Symptoms

- GraphQL requests returning `429 Too Many Requests` status
- Clients receiving rate limit errors: `rate limit exceeded`
- Rapid increase in metrics: `rate_limit_exceeded_total` counter jumping
- Legitimate traffic being blocked
- API response includes `Retry-After` header
- Specific IP addresses or API keys hitting limits
- Sudden spike in request volume (legitimate or attack)

## Impact

- **Medium**: Some clients unable to use API (those hitting limits)
- Legitimate high-volume clients affected
- Background jobs may experience intermittent failures
- Real-time subscriptions may drop due to rate limits on WebSocket upgrades

## Investigation

### 1. Rate Limiting Configuration

```bash
# Check rate limiting settings in the compiled schema (request throttle +
# auth-endpoint brute-force limits)
jq '.security.rate_limiting' /etc/fraiseql/schema.compiled.json

# Key fields: enabled, requests_per_second (per-IP), burst_size,
# trust_proxy_headers, trusted_proxy_cidrs, max_buckets, and the per-auth-endpoint
# pairs auth_start_max_requests / auth_start_window_secs (likewise auth_callback_*,
# auth_refresh_*, auth_logout_*, failed_login_*).
#
# max_buckets is a MEMORY ceiling on each tracking map, not a throttle: reaching it
# evicts the least-recently-used bucket, so a client resumes with a full one. If it
# is too low you will see limits under-enforced, never over-enforced.

# The server [rate_limiting] table (server.toml) and the env overrides
# (FRAISEQL_RATE_LIMITING_ENABLED, FRAISEQL_RATE_LIMIT_RPS_PER_IP,
# FRAISEQL_RATE_LIMIT_RPS_PER_USER, FRAISEQL_RATE_LIMIT_BURST_SIZE,
# FRAISEQL_RATE_LIMIT_MAX_BUCKETS) layer on top: server table < compiled schema <
# env, guards run on the result. ⚠ "<" means REPLACED, not merged — a compiled
# [security.rate_limiting] section makes the server.toml table inert in full.
env | grep FRAISEQL_RATE
grep -A8 '^\[rate_limiting\]' /etc/fraiseql/server.toml 2>/dev/null || echo "(no [rate_limiting] table in server.toml)"
```

### 2. Current Rate Limit State

> **Illustrative alert rules** — verify metric names against what your build's
> `/metrics` endpoint actually exports (`curl -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" …/metrics`).
> Names below that FraiseQL does not export directly (e.g. request/auth failure
> counters) must be derived from your load balancer, access logs, or exporters.

```bash
# Rate-limit related metrics the server exports (Redis backend errors only —
# there is no per-IP exceeded counter; count 429s at your load balancer):
curl -s -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" http://localhost:8000/metrics \
  | grep "rate_limit"

# Which callers are being limited: the server logs each 429; aggregate from logs
docker logs fraiseql-server 2>&1 | grep -c "429"
```

### 3. Request Volume Analysis

```bash
# Check current request rate
curl -s http://localhost:8000/metrics | grep "requests_total"

# Calculate requests per second
# Sum recent increment in requests_total counter

# Check requests by client type
curl -s http://localhost:8000/metrics | grep "requests_" | grep -E "authenticated|anonymous"

# Identify top clients hitting rate limits
docker logs fraiseql-server | grep "rate limit" | tail -50 | \
  cut -d' ' -f1 | sort | uniq -c | sort -rn | head -10

# Note: the request throttle's token buckets are in process memory — there is
# no Redis state to inspect for it. (The opt-in redis-rate-limiting feature
# covers the auth endpoints only.)
```

### 4. Backend State

The request throttle keeps its token buckets **in process memory** — there is no
Redis state to inspect, and a server restart resets every bucket. Only the opt-in
`redis-rate-limiting` build feature (auth endpoints) touches Redis; if enabled,
check its health via the exported error counter:

```bash
curl -s -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" http://localhost:8000/metrics \
  | grep "fraiseql_rate_limit_redis_errors_total"
```

### 5. Check for DDoS/Attack

```bash
# Identify source IPs of rate-limited requests
docker logs fraiseql-server | grep "rate limit" | \
  grep -oE "([0-9]{1,3}\.){3}[0-9]{1,3}" | sort | uniq -c | sort -rn | head -20

# Check if traffic is coming from expected sources
# Compare to whitelist/expected client IPs

# Check request patterns
docker logs fraiseql-server | grep "rate limit" | \
  cut -d' ' -f5- | cut -d'?' -f1 | sort | uniq -c | sort -rn | head -10

# If attack suspected: Check firewall rules
sudo ufw status
sudo iptables -L -n | grep DROP

# Sample malicious pattern: Same query repeated from different IPs
# Legitimate pattern: Different queries from same API client
```

### 6. Identify Legitimate High-Volume Client

```bash
# Check which clients/users are hitting limits
# Query metrics tagged with client_id or user_id if available

# If identifiable from request headers
docker logs fraiseql-server | grep "429\|rate limit" | \
  grep -oE "client_id=|api_key=|user=[^&]*" | cut -d'=' -f2 | sort | uniq -c | sort -rn

# Check their typical request rate
# Should match application's expected load

# Verify token/key belongs to known client
# Check client registration database or auth system
```

## Mitigation

### Immediate Actions (< 5 minutes)

1. **Increase request-throttle limits temporarily** (if legitimate traffic)

   The env overrides below are read by the server and win over both the config file
   and the compiled schema:

   ```bash
   export FRAISEQL_RATE_LIMIT_RPS_PER_IP=500
   export FRAISEQL_RATE_LIMIT_RPS_PER_USER=5000
   export FRAISEQL_RATE_LIMIT_BURST_SIZE=1000
   docker restart fraiseql-server
   ```

   The **auth-endpoint brute-force limits** (`auth_start_max_requests`, …) are compiled
   settings with no runtime override: change `[fraiseql.security.rate_limiting]` in the
   project `fraiseql.toml`, re-run `fraiseql compile`, and redeploy the compiled schema.
   Do not hand-edit `schema.compiled.json`.

2. **Allow a specific client or IP** (if legitimate)

   There is **no application-level whitelist** — allow-listing is done in front of the
   server (load balancer, WAF, or firewall rules), or by authenticating the client so it
   is limited per-user (`rps_per_user`) instead of sharing the per-IP budget.

3. **Reset rate-limit counters**

   The request throttle's token buckets are held in process memory — a restart clears
   them:

   ```bash
   docker restart fraiseql-server
   ```

4. **Temporarily disable rate limiting** (emergency only)

   ```bash
   # Only if under attack or critical service outage
   export FRAISEQL_RATE_LIMITING_ENABLED=false
   docker restart fraiseql-server
   ```

### Short-term (5-30 minutes)

1. **Block traffic from attack source**

   ```bash
   # If DDoS attack detected
   ATTACK_IP="192.168.1.50"

   # Using iptables
   sudo iptables -A INPUT -s $ATTACK_IP -j DROP

   # Using UFW
   sudo ufw deny from $ATTACK_IP

   # Using cloud provider (AWS security group, Azure NSG, etc.)
   # Update firewall rules to drop traffic from attack source
   ```

2. **Tighten the anonymous budget, keep the authenticated one generous**

   Anonymous clients share the per-IP budget; authenticated clients get the per-user
   budget. "Authenticated" means the bearer token **verifies** against the deployment's
   configured validator (`[auth]` or `[auth_hs256]`) — a request whose token is absent,
   expired or forged shares the per-IP budget, and a deployment with no authentication
   configured has no per-user budget at all (#1171):

   ```bash
   export FRAISEQL_RATE_LIMIT_RPS_PER_IP=10      # strict for anonymous traffic
   export FRAISEQL_RATE_LIMIT_RPS_PER_USER=1000  # generous for authenticated clients
   docker restart fraiseql-server
   ```

3. **Per-client (per-key) limits** are not a runtime knob — if one API client is
   misbehaving, revoke or rotate its credential, or block it upstream at the load
   balancer/WAF.

## Resolution

### Determine if Legitimate Traffic

```bash
#!/bin/bash
set -e

echo "=== Rate Limit Analysis ==="

# 1. Get the affected clients
echo "1. Clients hitting rate limits:"
docker logs fraiseql-server | grep "rate limit" | tail -20

# 2. Check request volume
echo ""
echo "2. Request volume metrics:"
curl -s http://localhost:8000/metrics | grep "requests_total"

# 3. Calculate current RPS
echo ""
echo "3. Calculating requests per second..."
BEFORE=$(curl -s http://localhost:8000/metrics | grep "requests_total\[^a-z\]" | head -1 | awk '{print $NF}')
sleep 10
AFTER=$(curl -s http://localhost:8000/metrics | grep "requests_total\[^a-z\]" | head -1 | awk '{print $NF}')
RPS=$(echo "scale=2; ($AFTER - $BEFORE) / 10" | bc)
echo "Current rate: ${RPS} requests/sec"

# 4. Check if within expected limits
echo ""
echo "4. Configured limits:"
jq '.security.rate_limiting | {auth_max_requests, auth_max_window_secs}' \
   /etc/fraiseql/schema.compiled.json
MAX_REQ=$(jq '.security.rate_limiting.auth_max_requests' /etc/fraiseql/schema.compiled.json)
WINDOW=$(jq '.security.rate_limiting.auth_max_window_secs' /etc/fraiseql/schema.compiled.json)
EXPECTED_RPS=$(echo "scale=2; $MAX_REQ / $WINDOW" | bc)
echo "Expected sustainable rate: ${EXPECTED_RPS} requests/sec"

# 5. Determine action
if (( $(echo "$RPS <= $EXPECTED_RPS * 1.5" | bc -l) )); then
    echo "✓ Traffic is expected - likely legitimate"
    echo "  Recommend: Check rate limit settings and adjust if needed"
else
    echo "✗ Traffic exceeds expected by $(echo "scale=0; $RPS / $EXPECTED_RPS" | bc)x"
    echo "  Recommend: Investigate for attack or misconfigured client"
fi
```

### Fix for Legitimate High-Volume Client

```bash
# 1. Identify the client
CLIENT_KEY="api_key_abc123"

# 2. Verify in client database
# Check: Is this API key valid? Who owns it? What's their plan/quota?

# 3. Update limits for this specific client
# Options depend on rate limiting backend:

# Option A: Per-key limits in compiled schema
jq ".security.rate_limiting.per_key_limits[\"$CLIENT_KEY\"] = {
  max_requests: 5000,
  window_secs: 60
}" /etc/fraiseql/schema.compiled.json > /tmp/schema.json
mv /tmp/schema.json /etc/fraiseql/schema.compiled.json

# Option B: Using Redis for more dynamic limits
redis-cli -u $REDIS_URL SET "rate_limit:key:$CLIENT_KEY:limit" 5000
redis-cli -u $REDIS_URL SET "rate_limit:key:$CLIENT_KEY:window" 60

# 4. Deploy and verify
docker restart fraiseql-server
sleep 3

# 5. Monitor this client's requests
watch -n 2 "docker logs fraiseql-server | grep '$CLIENT_KEY' | tail -5"
```

### Fix for Attack/DDoS

```bash
# 1. Identify attack patterns
echo "Attack sources:"
docker logs fraiseql-server | grep "rate limit" | \
  grep -oE "([0-9]{1,3}\.){3}[0-9]{1,3}" | sort | uniq -c | sort -rn

echo ""
echo "Attack targets (endpoints being hit):"
docker logs fraiseql-server | grep "rate limit" | \
  grep -oE "POST.*HTTP|GET.*HTTP" | sort | uniq -c | sort -rn

# 2. Block at firewall
ATTACK_IPS=$(docker logs fraiseql-server | grep "rate limit" | \
  grep -oE "([0-9]{1,3}\.){3}[0-9]{1,3}" | sort -u)

for IP in $ATTACK_IPS; do
    echo "Blocking $IP"
    sudo ufw deny from $IP
done

# 3. Add WAF rules (if available)
# Example: AWS WAF, Cloudflare, etc.
# Block IPs making > 100 requests/minute

# 4. Enable DDoS protection
# Contact cloud provider for DDoS mitigation

# 5. Temporary: Reduce rate limits to minimum
export FRAISEQL_RATE_LIMITING_ENABLED=true
export FRAISEQL_RATE_LIMIT_RPS_PER_IP=1
export FRAISEQL_RATE_LIMIT_BURST_SIZE=1
docker restart fraiseql-server
```

## Prevention

### Monitoring and Alerting

> **Illustrative alert rules** — verify metric names against what your build's
> `/metrics` endpoint actually exports (`curl -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" …/metrics`).
> Names below that FraiseQL does not export directly (e.g. request/auth failure
> counters) must be derived from your load balancer, access logs, or exporters.

```bash
# Prometheus alert rules for rate limiting
cat > /etc/prometheus/rules/fraiseql-rate-limiting.yml << 'EOF'
groups:
  - name: fraiseql_rate_limiting
    rules:
      - alert: HighRateLimitExceeded
        expr: rate(rate_limit_exceeded_total[5m]) > 0.1
        for: 5m
        action: notify

      - alert: RateLimitingDisabled
        expr: rate_limiting_enabled == 0
        for: 1m
        action: page

      - alert: RequestSpike
        expr: rate(requests_total[1m]) > avg_over_time(rate(requests_total[5m])[1h]) * 2
        for: 2m
        action: notify
EOF
```

### Rate Limiting Best Practices

```bash
# 1. Set appropriate limits based on tier
# - Free tier: 100 req/min
# - Standard: 1000 req/min
# - Premium: 10000 req/min

# 2. Distributed enforcement: the request throttle is per-process — with N replicas
# the effective limit is N × the configured budget. Size the per-replica budget
# accordingly, and set FRAISEQL_RATE_LIMIT_WARN_SINGLE_NODE=true to get a startup
# reminder when no distributed backend is configured.

# 3. Implement smart rate limiting
# - Higher per-user budgets for authenticated users (rps_per_user)
# - Strict per-IP budgets for anonymous traffic (rps_per_ip)
# - Burst allowances via burst_size (short spikes allowed)

# Example configuration:
export FRAISEQL_RATE_LIMIT_RPS_PER_USER=1000
export FRAISEQL_RATE_LIMIT_RPS_PER_IP=100
export FRAISEQL_RATE_LIMIT_BURST_SIZE=200

# 4. Monitor and adjust based on actual usage
# Review metrics monthly to update limits as traffic grows

# 5. Communicate limits to clients
# Document in API docs with clear Retry-After guidance
```

### Rate Limiting Maintenance

> **Illustrative alert rules** — verify metric names against what your build's
> `/metrics` endpoint actually exports (`curl -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" …/metrics`).
> Names below that FraiseQL does not export directly (e.g. request/auth failure
> counters) must be derived from your load balancer, access logs, or exporters.

```bash
# Weekly: Monitor rate limit hit rate
curl -s http://localhost:8000/metrics | grep "rate_limit_exceeded_total"

# Monthly: Review limits based on traffic growth
curl -s http://localhost:8000/metrics | grep "requests_total"

# Quarterly: Audit rate limit configuration
jq '.security.rate_limiting' /etc/fraiseql/schema.compiled.json

# Annually: Review and update limits based on business growth
# Update compiled schema with new tier definitions
```

## Escalation

- **Legitimate client hitting limits**: Sales/Account team (upgrade their plan)
- **DDoS attack**: Infrastructure / Security team + incident response
- **Rate limiting configuration issues**: Platform / DevOps team
- **Redis backend issues**: Infrastructure team (see runbook 09)
- **Rate limiting bugs in FraiseQL**: Application team
