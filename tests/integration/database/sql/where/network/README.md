# Network Operator Integration Tests

Integration tests for network-related operators including IP addresses, MAC addresses, hostnames, email addresses, and ports.

## Tests

### IP Address Tests
- `test_ip_filtering.py` - End-to-end IP filtering with IPv4/IPv6
- `test_ip_operations.py` - IP operator validation (inSubnet, isPrivate, etc.)

### MAC Address Tests
- `test_mac_filtering.py` - MAC address filtering workflows
- `test_mac_operations.py` - MAC operator validation

### Cross-Network Tests
- `test_consistency.py` - Consistency across network operators
- `test_network_fixes.py` - Network operator bug fixes
- `test_production_bugs.py` - Production regression tests
- `test_jsonb_integration.py` - JSONB + network types integration

## Running Tests

```bash
# All network tests
uv run pytest tests/integration/database/sql/where/network/ -v

# Specific test
uv run pytest tests/integration/database/sql/where/network/test_ip_filtering.py -v
```

## Coverage

- IP operators: inSubnet, isPrivate, isPublic, isIPv4, isIPv6, etc.
- MAC operators: Equality, list operations
- JSONB integration: Network types stored in JSONB
- Production scenarios: Real-world bug regressions
