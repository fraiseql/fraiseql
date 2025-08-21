# Testing CamelForge Integration - Safe Testing Guide

## Quick Start for Other Teams

### 1. **Beta Testing Environment Variables**

```bash
# Enable CamelForge beta testing
export FRAISEQL_CAMELFORGE_BETA=true

# Enable debug logging to see what's happening
export FRAISEQL_CAMELFORGE_DEBUG=true

# Safe mode: fall back to standard processing on any error
export FRAISEQL_CAMELFORGE_SAFE_MODE=true

# Start with specific entities only
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server,contract

# Performance comparison mode
export FRAISEQL_CAMELFORGE_COMPARE=true
```

### 2. **Gradual Testing Approach**

#### Phase 1: Single Entity Testing
```bash
# Test only DNS servers
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server
export FRAISEQL_CAMELFORGE_DEBUG=true

# Run your existing tests
npm test # or pytest, depending on your setup
```

#### Phase 2: Multiple Entity Testing
```bash
# Expand to more entities
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server,contract,allocation

# Run full test suite
npm run test:integration
```

#### Phase 3: Production-like Testing
```bash
# Remove allowlist restrictions
unset FRAISEQL_CAMELFORGE_ALLOWLIST

# Disable debug logging
export FRAISEQL_CAMELFORGE_DEBUG=false

# Performance testing
export FRAISEQL_CAMELFORGE_PERF=true
```

### 3. **A/B Testing Setup**

Test CamelForge vs. standard processing side-by-side:

```bash
# Enable comparison mode - generates both SQL versions
export FRAISEQL_CAMELFORGE_COMPARE=true
export FRAISEQL_CAMELFORGE_BETA=true
```

This will log both SQL queries for performance comparison.

### 4. **Safety Mechanisms**

#### Automatic Fallback
```bash
# Safe mode (default): any CamelForge error falls back to standard
export FRAISEQL_CAMELFORGE_SAFE_MODE=true
```

#### Entity Blocking
```bash
# Block specific entities if issues found
export FRAISEQL_CAMELFORGE_BLOCKLIST=problematic_entity,another_entity
```

#### Disable Anytime
```bash
# Instant disable
export FRAISEQL_CAMELFORGE_BETA=false
# or
unset FRAISEQL_CAMELFORGE_BETA
```

## Testing Checklist

### ✅ **Functional Testing**

1. **Verify GraphQL Responses**
   ```bash
   # Test simple query
   curl -X POST localhost:8000/graphql \
     -H "Content-Type: application/json" \
     -d '{"query": "{ dnsServers { id, ipAddress } }"}'
   ```

2. **Compare Response Formats**
   ```bash
   # Without CamelForge
   FRAISEQL_CAMELFORGE_BETA=false your_test_command

   # With CamelForge
   FRAISEQL_CAMELFORGE_BETA=true your_test_command

   # Responses should be identical
   ```

### ✅ **Performance Testing**

1. **Response Time Comparison**
   ```bash
   # Enable performance tracking
   export FRAISEQL_CAMELFORGE_PERF=true

   # Check logs for timing data
   tail -f app.log | grep "camelforge_timing"
   ```

2. **Load Testing**
   ```bash
   # Run load test with CamelForge
   export FRAISEQL_CAMELFORGE_BETA=true
   artillery quick --count 100 --num 10 http://localhost:8000/graphql

   # Run load test without CamelForge
   export FRAISEQL_CAMELFORGE_BETA=false
   artillery quick --count 100 --num 10 http://localhost:8000/graphql
   ```

### ✅ **Edge Case Testing**

1. **Large Query Testing**
   ```bash
   # Test queries with 50+ fields (should fall back automatically)
   export FRAISEQL_CAMELFORGE_BETA=true
   export FRAISEQL_CAMELFORGE_DEBUG=true

   # Run your largest GraphQL queries
   ```

2. **Error Handling**
   ```bash
   # Test with non-existent CamelForge function
   export FRAISEQL_CAMELFORGE_FUNCTION=non_existent_function

   # Should fall back gracefully
   ```

## Monitoring & Debugging

### Debug Logging

When `FRAISEQL_CAMELFORGE_DEBUG=true`, you'll see:

```
[DEBUG] CamelForge: Entity dns_server, Field count: 3, Using CamelForge: true
[DEBUG] CamelForge SQL: SELECT turbo.fn_camelforge(jsonb_build_object(...), 'dns_server')
[DEBUG] CamelForge: Response time: 0.8ms
```

### Performance Metrics

When `FRAISEQL_CAMELFORGE_PERF=true`, you'll see:

```
[INFO] camelforge_timing: entity=dns_server fields=3 duration=0.8ms fallback=false
[INFO] camelforge_timing: entity=contract fields=25 duration=15.2ms fallback=true
```

### Comparison Mode

When `FRAISEQL_CAMELFORGE_COMPARE=true`, you'll see:

```
[INFO] SQL_COMPARISON:
Standard: SELECT data FROM v_dns_server WHERE ...
CamelForge: SELECT turbo.fn_camelforge(...) FROM v_dns_server WHERE ...
```

## Rollback Plan

If issues are discovered:

```bash
# Immediate disable
export FRAISEQL_CAMELFORGE_BETA=false

# Or block specific entities
export FRAISEQL_CAMELFORGE_BLOCKLIST=problematic_entity

# Or restart application without feature flags
unset FRAISEQL_CAMELFORGE_BETA
systemctl restart your-app
```

## Common Testing Scenarios

### Scenario 1: API Compatibility Test
```bash
# Save current responses
FRAISEQL_CAMELFORGE_BETA=false npm run test:api > responses_before.json

# Test with CamelForge
FRAISEQL_CAMELFORGE_BETA=true npm run test:api > responses_after.json

# Compare (should be identical)
diff responses_before.json responses_after.json
```

### Scenario 2: Performance Benchmark
```bash
# Benchmark script
#!/bin/bash
echo "Testing performance with/without CamelForge..."

# Without CamelForge
export FRAISEQL_CAMELFORGE_BETA=false
time_before=$(curl -w "%{time_total}" -s -o /dev/null localhost:8000/graphql -d '{"query":"..."}')

# With CamelForge
export FRAISEQL_CAMELFORGE_BETA=true
time_after=$(curl -w "%{time_total}" -s -o /dev/null localhost:8000/graphql -d '{"query":"..."}')

echo "Before: ${time_before}s, After: ${time_after}s"
```

### Scenario 3: Integration Test
```bash
# Full application test with CamelForge
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server  # Start small

# Run your existing integration test suite
npm run test:integration

# If passes, expand
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server,contract,allocation
npm run test:integration
```

## Support & Questions

If you encounter issues during testing:

1. **Check Debug Logs**: Enable `FRAISEQL_CAMELFORGE_DEBUG=true`
2. **Use Safe Mode**: Ensure `FRAISEQL_CAMELFORGE_SAFE_MODE=true`
3. **Start Small**: Use allowlist to test one entity at a time
4. **Compare Results**: Use comparison mode to verify behavior
5. **Report Issues**: Include debug logs and specific query that failed

Remember: CamelForge is completely opt-in and falls back safely to standard processing if any issues occur.
