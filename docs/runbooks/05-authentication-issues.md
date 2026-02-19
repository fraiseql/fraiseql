# Runbook: Authentication Issues (JWT/OIDC/OAuth Failures)

## Symptoms

- GraphQL requests fail with `401 Unauthorized` or `403 Forbidden`
- JWT validation errors in logs: `invalid token`, `expired token`, `invalid signature`
- OIDC/OAuth discovery endpoint unreachable
- Token introspection failures
- Increased authentication failure rate (metrics show `auth_failures_total` increasing)
- Claims missing from token that are required for authorization
- Token refresh fails with `invalid_grant` or similar OAuth error

## Impact

- Users cannot authenticate, all GraphQL requests denied
- API becomes unusable for authenticated clients
- Background services cannot renew expired tokens
- Webhooks cannot be signed (no credentials)
- Real-time subscriptions disconnect

## Investigation

### 1. Authentication Configuration

```bash
# Check authentication environment variables
env | grep -E "^(AUTH_|JWT_|OIDC_|OAUTH_|SECRET_)"

# View authentication configuration in compiled schema
jq '.security.authentication' /etc/fraiseql/schema.compiled.json

# Check if authentication is enabled
jq '.security.authentication.enabled' /etc/fraiseql/schema.compiled.json

# Get configured auth providers
jq '.security.authentication.providers' /etc/fraiseql/schema.compiled.json
```

### 2. JWT Token Validation

```bash
# Decode a JWT token to inspect claims (from authorization header)
# Usage: TOKEN="<jwt_token>" bash decode_jwt.sh

cat > /tmp/decode_jwt.sh << 'EOF'
#!/bin/bash
TOKEN=$1
IFS='.' read -ra PARTS <<< "$TOKEN"
# Decode header and payload (not signature, that's verified server-side)
echo "Header:"
echo ${PARTS[0]} | base64 -d 2>/dev/null | jq . || echo "Invalid header"
echo "Payload:"
echo ${PARTS[1]} | base64 -d 2>/dev/null | jq . || echo "Invalid payload"
EOF

chmod +x /tmp/decode_jwt.sh

# Get a test token
TEST_TOKEN="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
/tmp/decode_jwt.sh "$TEST_TOKEN"

# Check token expiry
curl -s http://localhost:8815/metrics | grep "auth_token_expiry"

# Check token cache hit rate (if caching enabled)
curl -s http://localhost:8815/metrics | grep "auth_cache" | head -10
```

### 3. OIDC/OAuth Provider Connectivity

```bash
# Check OIDC discovery endpoint
OIDC_PROVIDER="https://accounts.example.com"
curl -s "${OIDC_PROVIDER}/.well-known/openid-configuration" | jq '.issuer, .authorization_endpoint, .token_endpoint'

# Test token endpoint (simulated)
curl -v -X POST "${OIDC_PROVIDER}/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials&client_id=${CLIENT_ID}&client_secret=${CLIENT_SECRET}"

# Check JWKS endpoint (for token verification)
curl -s "https://accounts.example.com/.well-known/jwks.json" | jq '.keys[]'

# Check if certificate is valid and not expired
openssl s_client -connect accounts.example.com:443 < /dev/null 2>&1 | grep -A5 "Verify return code"
```

### 4. FraiseQL Authentication Logs

```bash
# Enable debug logging for auth module
export RUST_LOG=fraiseql_auth=debug,fraiseql=debug
docker restart fraiseql-server

# Collect recent auth errors
docker logs fraiseql-server | grep -i "auth\|token\|jwt\|oidc" | tail -50

# Check specific authentication errors
docker logs fraiseql-server | grep -E "invalid token|expired|signature" | head -20

# Monitor auth failure metrics
curl -s http://localhost:8815/metrics | grep "auth_" | sort

# Example metrics:
# auth_failures_total{reason="invalid_signature"} 42
# auth_failures_total{reason="expired"} 15
# auth_failures_total{reason="missing_claim"} 8
```

### 5. Token Signature Verification

```bash
# For OIDC, verify JWKS is being fetched
docker logs fraiseql-server | grep "jwks\|public key" | tail -10

# Check if cached JWKS is stale
# JWKS should be refreshed periodically (typically hourly)
curl -s http://localhost:8815/metrics | grep "jwks_cache"

# Manual JWKS verification
ISSUER="https://accounts.example.com"
curl -s "${ISSUER}/.well-known/openid-configuration" | jq '.jwks_uri'

# Get JWKS
curl -s "https://accounts.example.com/.well-known/jwks.json" | jq '.keys | length'
```

### 6. Authorization Claims Validation

```bash
# Check what claims are required in compiled schema
jq '.security.authentication.required_claims' /etc/fraiseql/schema.compiled.json

# Example output:
# ["sub", "aud", "iat", "exp"]

# Verify token contains required claims
TOKEN="<jwt_token>"
echo "${TOKEN}" | cut -d'.' -f2 | base64 -d | jq '.sub, .aud, .iat, .exp'

# Check for custom claims validation
jq '.security.authentication.claim_mappings' /etc/fraiseql/schema.compiled.json
```

## Mitigation

### For JWT Signature Validation Failures

1. **Check JWT secret key**
   ```bash
   # If using symmetric signing (HS256), verify secret matches
   # The secret should be in Vault or environment

   # Check if key is accessible
   echo "JWT_SECRET length: ${#JWT_SECRET}"

   # For asymmetric signing, verify certificate chain
   # Check certificate expiry (common cause of token failures)
   openssl x509 -in /etc/fraiseql/jwt_cert.pem -text -noout | grep -A2 "Validity"

   # If certificate expired, obtain new one
   # Update JWT_CERT_PATH environment variable
   export JWT_CERT_PATH="/etc/fraiseql/jwt_cert_new.pem"
   docker restart fraiseql-server
   ```

2. **Refresh JWKS from OIDC provider**
   ```bash
   # Force JWKS cache refresh (trigger reload)
   curl -X POST http://localhost:8815/admin/auth/refresh-jwks

   # Or restart to clear cache
   docker restart fraiseql-server
   sleep 3

   # Verify new JWKS loaded
   curl -s http://localhost:8815/metrics | grep "jwks_last_updated"
   ```

### For Token Expiry Issues

3. **Check server clock skew**
   ```bash
   # JWT validation checks: token_exp > current_time
   # If server clock is wrong, all tokens fail

   # Check server time
   date -u

   # Compare to NTP server
   ntpdate -q pool.ntp.org

   # If skew detected, synchronize clock
   sudo systemctl restart ntp
   # or
   sudo chronyc makestep
   ```

4. **Increase token lifetime (temporary)**
   ```bash
   # If tokens are legitimately expiring too fast
   # Update token generation config in OIDC provider
   # Or in fraiseql config:
   export AUTH_TOKEN_TTL=3600  # 1 hour instead of default

   docker restart fraiseql-server
   ```

### For OIDC/OAuth Provider Unavailable

5. **Use cached JWKS/allow stale keys**
   ```bash
   # If OIDC provider is temporarily down, allow using cached JWKS
   export AUTH_ALLOW_STALE_JWKS=true
   export AUTH_JWKS_CACHE_TTL=86400  # 24 hours

   docker restart fraiseql-server
   ```

6. **Switch to backup authentication method**
   ```bash
   # If OIDC is down, can temporarily use different auth:
   export AUTH_PROVIDER="jwt_local"  # Use pre-shared JWT secret
   export JWT_SECRET="backup-secret-key"

   docker restart fraiseql-server
   ```

### For Missing Required Claims

7. **Update compiled schema with claim mappings**
   ```bash
   # Check what claims are required vs provided
   jq '.security.authentication.required_claims' /etc/fraiseql/schema.compiled.json

   # Update schema if claims mapping changed in OIDC provider
   # Recompile schema with new claim requirements
   fraiseql-cli compile schema.json --output schema.compiled.json

   # Deploy new schema
   cp schema.compiled.json /etc/fraiseql/
   docker restart fraiseql-server
   ```

## Resolution

### Complete Authentication Debugging Workflow

```bash
#!/bin/bash
set -e

echo "=== Authentication Troubleshooting ==="

# 1. Identify auth failure type
echo "1. Checking recent auth failures..."
docker logs fraiseql-server | grep -i "auth.*fail\|invalid.*token" | tail -10

# 2. Extract failure reasons
echo ""
echo "2. Auth failure breakdown:"
curl -s http://localhost:8815/metrics | grep "auth_failures_total"

# 3. Check provider connectivity
echo ""
echo "3. Checking OIDC provider:"
OIDC_ADDR=$(jq -r '.security.authentication.providers[0].issuer' /etc/fraiseql/schema.compiled.json)
echo "Provider: $OIDC_ADDR"

if curl -s "${OIDC_ADDR}/.well-known/openid-configuration" > /dev/null 2>&1; then
    echo "✓ Provider reachable"
else
    echo "✗ Provider unreachable"
    echo "  Check network connectivity and provider status"
fi

# 4. Check JWKS freshness
echo ""
echo "4. Checking JWKS cache:"
JWKS_UPDATED=$(curl -s http://localhost:8815/metrics | grep "jwks_last_updated" | cut -d' ' -f2)
echo "JWKS last updated: $(date -d @$JWKS_UPDATED 2>/dev/null || echo 'unknown')"

# 5. Check token validation settings
echo ""
echo "5. Auth configuration:"
jq '.security.authentication' /etc/fraiseql/schema.compiled.json | head -20

# 6. Test with sample token
echo ""
echo "6. Testing with sample token..."
SAMPLE_TOKEN="..."  # Obtain valid token from your OIDC provider
RESPONSE=$(curl -s -H "Authorization: Bearer $SAMPLE_TOKEN" http://localhost:8815/graphql -d '{"query": "{ __typename }"}')
echo "Response: $RESPONSE" | jq '.' 2>/dev/null || echo "$RESPONSE"
```

### JWT Token Validation Fix

```bash
# 1. Verify JWT parameters in schema
jq '.security.authentication | {
  algorithm: .jwt_algorithm,
  issuer: .jwt_issuer,
  audience: .jwt_audience,
  required_claims: .required_claims
}' /etc/fraiseql/schema.compiled.json

# 2. Generate test token with correct parameters
# Use a tool like https://jwt.io to create token or from your provider

# 3. Verify token manually
TOKEN="<your_token>"
HEADER=$(echo "$TOKEN" | cut -d'.' -f1 | base64 -d 2>/dev/null | jq .)
PAYLOAD=$(echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq .)

echo "Token Header:"
echo "$HEADER"
echo ""
echo "Token Payload:"
echo "$PAYLOAD"

# Check expiry
exp=$(echo "$PAYLOAD" | jq '.exp')
NOW=$(date +%s)
if [ $exp -lt $NOW ]; then
    echo "✗ Token is EXPIRED (exp=$exp, now=$NOW)"
else
    echo "✓ Token is valid (expires in $(( exp - NOW )) seconds)"
fi

# 4. Test token against FraiseQL
echo ""
echo "Testing token against FraiseQL:"
curl -v -H "Authorization: Bearer $TOKEN" http://localhost:8815/health
```

### OIDC Provider Recovery

```bash
# 1. Confirm OIDC provider is back online
OIDC_PROVIDER="$(jq -r '.security.authentication.providers[0].issuer' /etc/fraiseql/schema.compiled.json)"

until curl -s "${OIDC_PROVIDER}/.well-known/openid-configuration" > /dev/null; do
    echo "Waiting for provider to come online..."
    sleep 10
done
echo "✓ Provider online"

# 2. Force JWKS refresh
curl -s http://localhost:8815/admin/auth/refresh-jwks

# 3. Verify JWKS loaded
sleep 2
curl -s http://localhost:8815/metrics | grep "jwks_keys_loaded"

# 4. Test authentication
curl -H "Authorization: Bearer <valid_token>" http://localhost:8815/health
```

## Prevention

### Monitoring Setup

```bash
# Prometheus alerts for auth failures
cat > /etc/prometheus/rules/fraiseql-auth.yml << 'EOF'
groups:
  - name: fraiseql_auth
    rules:
      - alert: HighAuthFailureRate
        expr: rate(auth_failures_total[5m]) > 0.1
        for: 5m
        action: page

      - alert: AuthProviderUnreachable
        expr: auth_provider_reachable == 0
        for: 2m
        action: page

      - alert: JWKSStale
        expr: |
          (time() - auth_jwks_last_updated_seconds) > 86400
        for: 10m
        action: notify

      - alert: TokenSignatureFailures
        expr: |
          rate(auth_failures_total{reason="invalid_signature"}[5m]) > 0.05
        for: 5m
        action: page
EOF

# Alerting webhook or notification service integration
```

### Best Practices

- **Token validation caching**: Cache JWKS and validated tokens to reduce provider load
  ```bash
  export AUTH_JWKS_CACHE_TTL=3600  # 1 hour
  export AUTH_TOKEN_CACHE_TTL=300   # 5 minutes
  ```

- **Graceful degradation**: Allow stale JWKS during provider outages
  ```bash
  export AUTH_ALLOW_STALE_JWKS=true
  ```

- **Clock synchronization**: Ensure all servers use NTP for accurate time
  ```bash
  sudo systemctl enable ntp
  sudo systemctl start ntp
  ```

- **Certificate rotation**: Monitor JWT signing certificate expiry
  ```bash
  openssl x509 -in cert.pem -text -noout | grep -A2 "Not After"
  ```

- **OIDC configuration validation**: Verify config after any provider updates
  ```bash
  curl -s "$(jq -r '.security.authentication.providers[0].issuer' schema.compiled.json)/.well-known/openid-configuration" | jq .
  ```

### Regular Checks

```bash
# Weekly: Verify OIDC provider certificate validity
openssl s_client -connect accounts.example.com:443 -servername accounts.example.com < /dev/null 2>&1 | \
  openssl x509 -text -noout | grep -E "Not Before|Not After"

# Weekly: Check token failure trends
curl -s http://localhost:8815/metrics | grep "auth_failures_total" | sort

# Monthly: Test token refresh flow
# Generate new token and verify it works

# Quarterly: Update JWKS from OIDC provider manually
curl -s "https://provider/.well-known/jwks.json" > /etc/fraiseql/jwks_backup.json
```

## Escalation

- **OIDC provider issues**: Identity provider team / Auth service team
- **JWT secret/certificate issues**: Security team
- **Token generation failures**: Identity provider team
- **FraiseQL configuration issues**: Application team
- **Network connectivity to provider**: Infrastructure / Network team
- **Enterprise IdP integration issues**: Identity management team
