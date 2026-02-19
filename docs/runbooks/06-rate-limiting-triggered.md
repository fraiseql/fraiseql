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
# Check rate limiting settings in compiled schema
jq '.security.rate_limiting' /etc/fraiseql/schema.compiled.json

# Get current limits
jq '.security.rate_limiting | {
  enabled: .enabled,
  auth_start_max_requests: .auth_start_max_requests,
  auth_start_window_secs: .auth_start_window_secs,
  auth_max_requests: .auth_max_requests,
  auth_max_window_secs: .auth_max_window_secs,
  anon_max_requests: .anon_max_requests,
  anon_max_window_secs: .anon_max_window_secs,
  backend: .backend
}' /etc/fraiseql/schema.compiled.json

# Check which backend is being used (redis or in-memory)
echo "Rate limiter backend: $(jq -r '.security.rate_limiting.backend' /etc/fraiseql/schema.compiled.json)"
```

### 2. Current Rate Limit State

```bash
# Check rate limit metrics
curl -s http://localhost:8815/metrics | grep "rate_limit"

# Example metrics:
# rate_limit_exceeded_total{client_type="authenticated"} 42
# rate_limit_exceeded_total{client_type="anonymous"} 128
# rate_limit_current_requests{ip="192.168.1.100"} 95
# rate_limit_window_remaining_secs 45

# Check which IPs are being rate limited
curl -s http://localhost:8815/metrics | grep "rate_limit_exceeded" | sort

# Monitor in real-time
watch -n 1 'curl -s http://localhost:8815/metrics | grep rate_limit'
```

### 3. Request Volume Analysis

```bash
# Check current request rate
curl -s http://localhost:8815/metrics | grep "requests_total"

# Calculate requests per second
# Sum recent increment in requests_total counter

# Check requests by client type
curl -s http://localhost:8815/metrics | grep "requests_" | grep -E "authenticated|anonymous"

# Identify top clients hitting rate limits
docker logs fraiseql-server | grep "rate limit" | tail -50 | \
  cut -d' ' -f1 | sort | uniq -c | sort -rn | head -10

# If using Redis, query rate limit state directly
redis-cli -u $REDIS_URL KEYS "rate_limit:*" | head -20
redis-cli -u $REDIS_URL GET "rate_limit:user:12345"
```

### 4. Backend State (Redis if configured)

```bash
# Check if Redis is connected
curl -s http://localhost:8815/metrics | grep "redis\|cache" | head -10

# If using Redis for rate limiting
REDIS_ADDR=$(echo "$REDIS_URL" | cut -d':' -f2 | tr -d '/')
REDIS_PORT=$(echo "$REDIS_URL" | cut -d':' -f3)

redis-cli -h $REDIS_ADDR -p $REDIS_PORT ping

# Check rate limit keys in Redis
redis-cli -u $REDIS_URL DBSIZE
redis-cli -u $REDIS_URL KEYS "rate_limit:*" | wc -l

# Check specific client's rate limit
redis-cli -u $REDIS_URL GET "rate_limit:client:my_client_id"
redis-cli -u $REDIS_URL TTL "rate_limit:client:my_client_id"  # Time until reset
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

1. **Increase rate limits temporarily** (if legitimate traffic)
   ```bash
   # Option 1: Update compiled schema and reload
   jq '.security.rate_limiting.auth_max_requests = 1000' \
      /etc/fraiseql/schema.compiled.json > /tmp/schema_updated.json
   mv /tmp/schema_updated.json /etc/fraiseql/schema.compiled.json

   docker restart fraiseql-server

   # Option 2: Set environment variable override
   export FRAISEQL_RATE_LIMIT_AUTH_MAX_REQUESTS=1000
   docker restart fraiseql-server
   ```

2. **Whitelist specific client or IP** (if legitimate)
   ```bash
   # Add to rate limit whitelist
   # Method depends on configuration, but typically:
   export FRAISEQL_RATE_LIMIT_WHITELIST="192.168.1.100,api_key_xyz"
   docker restart fraiseql-server

   # Or update compiled schema
   jq '.security.rate_limiting.whitelist += ["192.168.1.100"]' \
      /etc/fraiseql/schema.compiled.json > /tmp/schema.json
   mv /tmp/schema.json /etc/fraiseql/schema.compiled.json
   ```

3. **Clear rate limit state in Redis** (reset counters)
   ```bash
   # If using Redis backend, flush rate limit keys
   redis-cli -u $REDIS_URL FLUSHDB  # CAREFUL: Clears entire DB!

   # Safer: Clear only rate_limit keys
   redis-cli -u $REDIS_URL EVAL \
     'return redis.call("del", unpack(redis.call("keys", ARGV[1])))' \
     0 'rate_limit:*'
   ```

4. **Temporarily disable rate limiting** (emergency only)
   ```bash
   # Only if under attack or critical service outage
   export FRAISEQL_RATE_LIMITING_ENABLED=false
   docker restart fraiseql-server

   # Or set to very high limits
   export FRAISEQL_RATE_LIMIT_AUTH_MAX_REQUESTS=999999
   docker restart fraiseql-server
   ```

### Short-term (5-30 minutes)

5. **Block traffic from attack source**
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

6. **Enable stricter rate limiting for anonymous clients**
   ```bash
   # Separate limits for authenticated vs anonymous
   export FRAISEQL_RATE_LIMIT_ANON_MAX_REQUESTS=10    # Very strict
   export FRAISEQL_RATE_LIMIT_ANON_WINDOW_SECS=60

   export FRAISEQL_RATE_LIMIT_AUTH_MAX_REQUESTS=1000  # Generous for auth
   docker restart fraiseql-server
   ```

7. **Implement per-client rate limiting**
   ```bash
   # If specific API key is misbehaving, limit just that key
   redis-cli -u $REDIS_URL SET "rate_limit:key:bad_key" 5  # 5 requests
   redis-cli -u $REDIS_URL EXPIRE "rate_limit:key:bad_key" 3600  # 1 hour
   ```

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
curl -s http://localhost:8815/metrics | grep "requests_total"

# 3. Calculate current RPS
echo ""
echo "3. Calculating requests per second..."
BEFORE=$(curl -s http://localhost:8815/metrics | grep "requests_total\[^a-z\]" | head -1 | awk '{print $NF}')
sleep 10
AFTER=$(curl -s http://localhost:8815/metrics | grep "requests_total\[^a-z\]" | head -1 | awk '{print $NF}')
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
export FRAISEQL_RATE_LIMIT_ANON_MAX_REQUESTS=1
export FRAISEQL_RATE_LIMIT_ANON_WINDOW_SECS=60
docker restart fraiseql-server
```

## Prevention

### Monitoring and Alerting

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

# 2. Use Redis for distributed rate limiting
export REDIS_URL="redis://redis:6379/0"
export FRAISEQL_RATE_LIMIT_BACKEND="redis"

# 3. Implement smart rate limiting
# - Higher limits for authenticated users
# - Credential-based limits (API key tier)
# - Burst allowances (short spikes allowed)

# Example configuration:
export FRAISEQL_RATE_LIMIT_AUTH_MAX_REQUESTS=1000
export FRAISEQL_RATE_LIMIT_AUTH_WINDOW_SECS=60
export FRAISEQL_RATE_LIMIT_ANON_MAX_REQUESTS=100
export FRAISEQL_RATE_LIMIT_ANON_WINDOW_SECS=60

# 4. Monitor and adjust based on actual usage
# Review metrics monthly to update limits as traffic grows

# 5. Communicate limits to clients
# Document in API docs with clear Retry-After guidance
```

### Rate Limiting Maintenance

```bash
# Weekly: Monitor rate limit hit rate
curl -s http://localhost:8815/metrics | grep "rate_limit_exceeded_total"

# Monthly: Review limits based on traffic growth
curl -s http://localhost:8815/metrics | grep "requests_total"

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
