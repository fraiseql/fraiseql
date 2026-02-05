# Rate Limiting

FraiseQL implements request rate limiting to prevent denial-of-service (DoS) attacks and resource exhaustion.

## Prerequisites

**Required Knowledge:**

- HTTP request fundamentals (status codes, headers)
- Rate limiting algorithms (token bucket, leaky bucket)
- TOML configuration file syntax
- Authentication concepts (IP-based vs user-based rate limiting)
- DoS attack patterns and mitigation strategies

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- A text editor for `fraiseql.toml` configuration
- curl or Postman (for testing rate limit headers)
- Bash or similar shell for configuration management

**Required Infrastructure:**

- FraiseQL server instance (configured with rate limiting enabled)
- PostgreSQL or similar database (for storing rate limit state)
- Network connectivity to test HTTP endpoints

**Optional but Recommended:**

- Monitoring tools (Prometheus, Grafana) to track rate limit violations
- API gateway (Kong, Tyk) for additional rate limiting at proxy level
- Distributed rate limiting backend (Redis) for multi-instance deployments
- Logging aggregation (ELK, Splunk) for rate limit event analysis

**Time Estimate:** 15-30 minutes for basic configuration, 1-2 hours for production tuning

## Overview

Rate limiting is implemented using a token bucket algorithm with support for:

- **Per-IP rate limiting**: Limits requests from individual client IPs
- **Per-user rate limiting**: Limits requests from authenticated users
- **Configurable burst capacity**: Allows temporary traffic spikes
- **Response headers**: Clients can check remaining quota via HTTP headers

## Configuration

Rate limiting configuration is defined in your server configuration file:

```toml
[rate_limit]
# Enable/disable rate limiting
enabled = true

# Requests per second per IP address
rps_per_ip = 100

# Requests per second per authenticated user
rps_per_user = 1000

# Maximum burst capacity (tokens accumulated)
burst_size = 500

# Cleanup interval for stale entries (seconds)
cleanup_interval_secs = 300
```text

### Default Values

- `enabled`: `true`
- `rps_per_ip`: 100 req/sec
- `rps_per_user`: 1000 req/sec
- `burst_size`: 500 requests
- `cleanup_interval_secs`: 300 seconds (5 minutes)

## Key Extraction Strategy

Rate limits are applied using the following key extraction logic:

### 1. Authenticated Requests

For requests with authentication credentials:

- **Key**: User ID
- **Limit**: `rps_per_user` (higher limit)
- **Use case**: Trusted authenticated users get higher quotas

### 2. Unauthenticated Requests

For requests without authentication:

- **Key**: Client IP address
- **Limit**: `rps_per_ip` (lower limit)
- **Use case**: Protects against anonymous abuse

### 3. IP Address Resolution

When extracting the client IP address:

1. **Direct Connections** (no proxy):
   - Use the source IP from the socket connection directly

2. **Behind Untrusted Proxies**:
   - Ignore `X-Forwarded-For` header
   - Use the proxy server's IP as the rate limit key
   - **Why**: Clients cannot spoof IPs through untrusted intermediaries

3. **Behind Trusted Proxies** (configured):
   - Trust the `X-Forwarded-For` header
   - Use the client's real IP from the header
   - **Why**: Trusted proxies (load balancers, reverse proxies) are configured to only forward legitimate client IPs

## Configuration for Proxy Environments

To support rate limiting through proxies, configure trusted proxy IP ranges:

```toml
[rate_limit]
enabled = true
rps_per_ip = 100
rps_per_user = 1000
# Trusted proxies that can set X-Forwarded-For header
trusted_proxies = [
    "10.0.0.0/8",          # Internal load balancer (10.0.0.0 - 10.255.255.255)
    "172.16.0.0/12",       # VPC CIDR (172.16.0.0 - 172.31.255.255)
    "203.0.113.0/24",      # CDN egress IPs
    "203.0.113.5/32",      # Specific trusted reverse proxy
]
```text

### Best Practices

1. **Whitelist only known proxies**: Only add IPs/CIDRs of proxies you control
2. **Use specific IPs when possible**: Prefer individual IPs over large CIDR blocks
3. **Monitor proxy configurations**: Update if infrastructure changes
4. **Test rate limiting**: Verify correct keys are used before deployment

## Response Headers

When a request is accepted, FraiseQL includes rate limit information in HTTP response headers:

```text
X-RateLimit-Limit: 100          # Maximum requests per second
X-RateLimit-Remaining: 45       # Remaining requests in current window
Retry-After: 60                 # Seconds to wait before retrying (when limited)
```text

Example client handling:

```python
import time
import requests

def graphql_request(url, query):
    while True:
        response = requests.post(url, json={"query": query})

        if response.status_code == 429:  # Too Many Requests
            wait_time = int(response.headers.get("Retry-After", "60"))
            print(f"Rate limited, retrying in {wait_time}s")
            time.sleep(wait_time)
            continue

        return response.json()
```text

## Token Bucket Algorithm

The implementation uses a token bucket algorithm:

1. Each IP/user gets a "bucket" of tokens
2. Bucket capacity = `burst_size`
3. Tokens refill at `rps_per_ip` or `rps_per_user` tokens per second
4. Each request costs 1 token
5. If bucket has tokens, request is allowed and 1 token is consumed
6. If bucket is empty, request is rejected with 429 status

### Example

With `rps_per_ip=100` and `burst_size=500`:

```text
Initial: [████████████████████] 500 tokens
After 1 request: [███████████████████] 499 tokens
After 100 requests: [████] 400 tokens
1 second later: [██████] 500 tokens (refilled)
```text

## Disabling Rate Limiting

To disable rate limiting (not recommended for production):

```toml
[rate_limit]
enabled = false
```text

Rate limiting can also be disabled at runtime by setting `enabled=false` in the configuration before server startup.

## Monitoring

Monitor rate limiting activity through logging:

```rust
// Debug level logs IP limit violations
debug!(ip = "192.168.1.100", "Rate limit exceeded for IP");

// Warn level logs user limit violations
warn!(user_id = "user123", "Rate limit exceeded for user");
```text

Enable debug logging to see rate limit activity:

```bash
RUST_LOG=fraiseql_server::middleware::rate_limit=debug
```text

## Security Considerations

1. **DoS Protection**: Rate limiting helps prevent DoS attacks but should be combined with other protections (firewall rules, WAF)

2. **Proxy Spoofing**: Always whitelist specific trusted proxies. Never trust `X-Forwarded-For` from untrusted sources

3. **Distributed Attacks**: Rate limiting is per-server instance. Use shared backend (Redis) for distributed rate limiting in multi-server deployments

4. **User ID Extraction**: Ensure user authentication is correct and user IDs cannot be forged

5. **Clock Skew**: Uses system time for token refill. Significant clock skew can affect accuracy

## Testing Rate Limiting

Test the implementation:

```bash
# Test IP-based limiting
for i in {1..110}; do
    curl -s http://localhost:4000/graphql -d '{"query":"{ users { id } }"}' \
         -H "Content-Type: application/json"
    echo "Request $i"
done
```text

The 101st+ requests should return HTTP 429 (Too Many Requests).

## References

- [RFC 6585 - HTTP 429 Too Many Requests](https://tools.ietf.org/html/rfc6585)
- [Token Bucket Algorithm](https://en.wikipedia.org/wiki/Token_bucket)
- [OWASP API Security - Rate Limiting](https://owasp.org/www-project-api-security/)
