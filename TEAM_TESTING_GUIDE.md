# CamelForge Testing Guide for Teams

## 🎯 **Testing Strategies for Different Teams**

### **Strategy 1: Environment Variable Testing** (Safest)
Perfect for teams that want to test without code changes.

```bash
# Step 1: Enable beta testing
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_DEBUG=true
export FRAISEQL_CAMELFORGE_SAFE_MODE=true

# Step 2: Test one entity at a time
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server

# Step 3: Run your existing tests
npm test  # or your test command

# Step 4: Check logs for CamelForge activity
tail -f app.log | grep "CamelForge"
```

### **Strategy 2: Docker Testing** (Isolated)
Perfect for teams that want isolated testing environment.

```bash
# Step 1: Build testing environment
docker-compose -f docker-compose.camelforge-test.yml up -d

# Step 2: Run tests in container
docker-compose exec camelforge-test test-basic
docker-compose exec camelforge-test test-perf
docker-compose exec camelforge-test test-all

# Step 3: Clean up
docker-compose -f docker-compose.camelforge-test.yml down
```

### **Strategy 3: Branch Testing** (Comprehensive)
Perfect for teams that want full testing capabilities.

```bash
# Step 1: Switch to testing branch
git checkout feature/camelforge-integration

# Step 2: Run testing script
./test-camelforge.sh

# Step 3: Test with your application
python your_app.py  # with CamelForge env vars
```

## 🧪 **Testing Phases**

### **Phase 1: Smoke Testing** (5 minutes)
Verify nothing is broken with CamelForge disabled:

```bash
export FRAISEQL_CAMELFORGE_BETA=false
npm test  # Should pass exactly as before
```

### **Phase 2: Basic Functionality** (15 minutes)
Test CamelForge with minimal risk:

```bash
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server  # Replace with your safest entity
export FRAISEQL_CAMELFORGE_DEBUG=true

# Run your smallest, safest GraphQL queries
curl -X POST localhost:8000/graphql -d '{"query": "{ dnsServers { id } }"}'
```

### **Phase 3: Performance Testing** (30 minutes)
Compare performance before/after:

```bash
# Benchmark script
#!/bin/bash
echo "Performance comparison..."

# Before (standard)
export FRAISEQL_CAMELFORGE_BETA=false
start_time=$(date +%s%N)
curl -s localhost:8000/graphql -d '{"query": "{ dnsServers { id, ipAddress, name } }"}' > /dev/null
end_time=$(date +%s%N)
before_time=$((($end_time - $start_time) / 1000000))

# After (CamelForge)
export FRAISEQL_CAMELFORGE_BETA=true
start_time=$(date +%s%N)
curl -s localhost:8000/graphql -d '{"query": "{ dnsServers { id, ipAddress, name } }"}' > /dev/null
end_time=$(date +%s%N)
after_time=$((($end_time - $start_time) / 1000000))

echo "Standard: ${before_time}ms, CamelForge: ${after_time}ms"
```

### **Phase 4: Full Integration** (1 hour)
Test with your complete application:

```bash
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_DEBUG=false  # Disable debug for realistic testing
unset FRAISEQL_CAMELFORGE_ALLOWLIST     # Test all entities

# Run your full test suite
npm run test:integration
npm run test:e2e
```

## 📊 **What to Look For**

### ✅ **Success Indicators**
- All existing tests pass
- GraphQL responses are identical in format
- Performance is same or better for small queries
- No errors in application logs
- Debug logs show "Using CamelForge: true" for small queries
- Debug logs show "Using CamelForge: false" for large queries (fallback)

### ⚠️ **Warning Signs**
- Test failures that didn't exist before
- Different GraphQL response structure
- Errors mentioning "camelforge" or "entity_type"
- Significantly slower performance
- Database connection errors

### 🚨 **Red Flags** (Immediate Disable)
- Application crashes
- Database errors
- Data corruption
- Memory leaks
- Complete performance degradation

## 🛠️ **Debugging Common Issues**

### Issue: "entity_type is required when camelforge_enabled=True"
```bash
# Solution: Enable entity mapping
export FRAISEQL_CAMELFORGE_ENTITY_MAPPING=true

# Or disable CamelForge temporarily
export FRAISEQL_CAMELFORGE_BETA=false
```

### Issue: "function turbo.fn_camelforge does not exist"
```sql
-- Solution: Create the function in your database
CREATE OR REPLACE FUNCTION turbo.fn_camelforge(input_data JSONB, entity_type TEXT)
RETURNS JSONB AS $$
BEGIN
    -- For testing, return input unchanged
    RETURN input_data;
END;
$$ LANGUAGE plpgsql;
```

### Issue: Performance is slower
```bash
# Check if you're hitting the fallback threshold
export FRAISEQL_CAMELFORGE_DEBUG=true
# Look for "fallback=true" in logs

# Adjust threshold if needed
export FRAISEQL_JSONB_FIELD_LIMIT_THRESHOLD=50
```

### Issue: Different response format
```bash
# Enable comparison mode to see both SQL versions
export FRAISEQL_CAMELFORGE_COMPARE=true
# Check logs for SQL differences
```

## 📈 **Performance Expectations**

### **Small Queries** (1-10 fields)
- **Expected**: 10-50% faster response times
- **Best case**: Sub-millisecond responses
- **Minimum**: No performance degradation

### **Medium Queries** (11-20 fields)
- **Expected**: Similar or slightly better performance
- **CamelForge**: Should still be used (below threshold)

### **Large Queries** (21+ fields)
- **Expected**: Identical performance to before
- **Behavior**: Should automatically fall back to standard processing

## 🔄 **Rollback Procedures**

### **Immediate Disable**
```bash
export FRAISEQL_CAMELFORGE_BETA=false
# Restart your application
```

### **Partial Disable**
```bash
# Block problematic entities
export FRAISEQL_CAMELFORGE_BLOCKLIST=problematic_entity

# Or reduce to known-good entities
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server
```

### **Emergency Disable**
```bash
# Remove all CamelForge environment variables
unset FRAISEQL_CAMELFORGE_BETA
unset FRAISEQL_CAMELFORGE_DEBUG
unset FRAISEQL_CAMELFORGE_ALLOWLIST
unset FRAISEQL_CAMELFORGE_BLOCKLIST

# Restart application
systemctl restart your-app  # or docker-compose restart
```

## 📋 **Testing Checklist**

### **Pre-Testing**
- [ ] Backup your database (if testing with real data)
- [ ] Document current performance baselines
- [ ] Ensure you can quickly rollback
- [ ] Set up monitoring/logging

### **During Testing**
- [ ] Start with CamelForge disabled (verify baseline)
- [ ] Enable CamelForge with single entity
- [ ] Check debug logs for expected behavior
- [ ] Compare response formats (should be identical)
- [ ] Test performance with small queries
- [ ] Test large queries (should fallback)
- [ ] Run your existing test suite

### **Post-Testing**
- [ ] Document performance differences
- [ ] Note any issues or concerns
- [ ] Test rollback procedure
- [ ] Clean up test data/environments

## 🤝 **Getting Help**

If you encounter issues:

1. **Check the logs** with debug enabled
2. **Try safe mode** with fallback enabled
3. **Test single entity** with allowlist
4. **Compare SQL output** with comparison mode
5. **Share debug logs** with specific queries that failed

## 📊 **Example Testing Session**

```bash
#!/bin/bash
# Complete testing session example

echo "🧪 Starting CamelForge testing session..."

# Phase 1: Baseline
echo "Phase 1: Baseline testing..."
export FRAISEQL_CAMELFORGE_BETA=false
npm test
echo "✅ Baseline tests passed"

# Phase 2: Basic CamelForge
echo "Phase 2: Basic CamelForge testing..."
export FRAISEQL_CAMELFORGE_BETA=true
export FRAISEQL_CAMELFORGE_ALLOWLIST=dns_server
export FRAISEQL_CAMELFORGE_DEBUG=true
npm test
echo "✅ Basic CamelForge tests passed"

# Phase 3: Performance check
echo "Phase 3: Performance testing..."
export FRAISEQL_CAMELFORGE_COMPARE=true
curl -w "Time: %{time_total}s\n" localhost:8000/graphql \
  -d '{"query": "{ dnsServers { id, ipAddress } }"}'
echo "✅ Performance test completed"

# Phase 4: Full testing
echo "Phase 4: Full integration testing..."
unset FRAISEQL_CAMELFORGE_ALLOWLIST
export FRAISEQL_CAMELFORGE_DEBUG=false
npm run test:integration
echo "✅ Full integration tests passed"

echo "🎉 CamelForge testing session completed successfully!"
```

This comprehensive approach ensures teams can safely test CamelForge with minimal risk and maximum confidence.
