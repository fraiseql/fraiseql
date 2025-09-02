# FraiseQL v0.6.0-beta.1: WHERE Refactor Beta Testing Guide

## 🎯 What We're Testing

This beta release contains a **complete refactor of WHERE clause functionality** implementing 84 SQL operators across 11 field types with comprehensive PostgreSQL integration.

### ⚠️ **BREAKING CHANGES**
This is a **major refactor** - please test thoroughly before deploying to production!

## 🚀 **New Capabilities**

### **Supported Field Types & Operators**
| Field Type | Operators | PostgreSQL Casting | New Features |
|------------|-----------|-------------------|---------------|
| **MAC Address** | `eq`, `neq`, `in`, `notin` | `::macaddr` | ✨ NEW - Full MAC address support |
| **LTree Hierarchical** | `eq`, `neq`, `ancestor_of`, `descendant_of`, `matches_lquery` | `::ltree` | ✨ NEW - Tree hierarchies |
| **DateRange** | `eq`, `neq`, `contains_date`, `overlaps`, `strictly_left`, `adjacent` | `::daterange` | ✨ NEW - Range operations |
| **IP Address** | `eq`, `neq`, `in`, `subnet_of`, `contains_ip` | `::inet` | 🔄 Enhanced subnet operations |
| **Port Numbers** | `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in` | `::integer` | ✨ NEW - Numeric comparisons |
| **Email** | `eq`, `neq`, `in`, `notin` | Text | ✨ NEW - Email validation |
| **Hostname** | `eq`, `neq`, `in`, `notin` | Text | ✨ NEW - DNS hostname support |
| **DateTime** | `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in` | `::timestamptz` | 🔄 Enhanced timezone support |
| **Date** | `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in` | `::date` | 🔄 Enhanced date operations |

### **Smart Field Detection**
- **Field Name Recognition**: Automatically detects field types from names (`server_ip`, `device_mac`, `created_at`)
- **Value Pattern Matching**: Analyzes actual values to determine appropriate operators
- **Type Hint Override**: Explicit `field_type` parameter for precise control

## 🧪 **Beta Testing Instructions**

### **1. Installation**

```bash
# Install beta version
pip install fraiseql==0.6.0b1

# Or upgrade existing installation
pip install --upgrade fraiseql==0.6.0b1

# Verify version
python -c "import fraiseql; print(fraiseql.__version__)"
# Should output: 0.6.0-beta.1
```

### **2. Testing Scenarios**

#### **Scenario A: Basic WHERE Compatibility**
Test that existing WHERE clauses still work:

```python
# Test your existing WHERE clauses
where_filters = {
    "name": {"eq": "test"},
    "status": {"in": ["active", "pending"]},
    "created_at": {"gte": "2023-01-01T00:00:00Z"}
}

# This should work exactly as before
results = await your_query_function(where=where_filters)
```

#### **Scenario B: New Field Types** ✨
Test the new field type capabilities:

```python
# MAC Address filtering
where_filters = {
    "device_mac": {"eq": "00:11:22:33:44:55"},
    "network_macs": {"in": ["aa:bb:cc:dd:ee:ff", "11:22:33:44:55:66"]}
}

# IP Address with subnet operations
where_filters = {
    "server_ip": {"subnet_of": "192.168.1.0/24"},
    "client_ip": {"contains_ip": "10.0.0.1"}
}

# Hierarchical data (categories, paths, etc.)
where_filters = {
    "category_path": {"ancestor_of": "electronics.computers"},
    "menu_path": {"descendant_of": "products"}
}

# Date ranges
where_filters = {
    "event_period": {"contains_date": "2023-07-15"},
    "booking_range": {"overlaps": "[2023-08-01,2023-08-31]"}
}

# Port number filtering
where_filters = {
    "server_port": {"gt": 8000},
    "service_ports": {"in": [80, 443, 8080]}
}
```

#### **Scenario C: Smart Field Detection** 🧠
Test automatic field type detection:

```python
# These should auto-detect field types from names:
where_filters = {
    "server_ip": {"eq": "192.168.1.1"},        # Auto-detects as IP
    "device_mac": {"eq": "00:11:22:33:44:55"}, # Auto-detects as MAC
    "user_email": {"eq": "test@example.com"},   # Auto-detects as Email
    "api_hostname": {"eq": "api.example.com"},  # Auto-detects as Hostname
    "server_port": {"eq": 8080},               # Auto-detects as Port
    "created_at": {"gte": "2023-01-01"},       # Auto-detects as DateTime
    "birth_date": {"eq": "1990-05-15"},        # Auto-detects as Date
    "category_path": {"ancestor_of": "a.b.c"}  # Auto-detects as LTree
}
```

### **3. Performance Testing**

```python
import time

# Test query performance with new operators
start_time = time.time()

# Large dataset filtering
where_filters = {
    "server_ip": {"subnet_of": "10.0.0.0/8"},
    "status": {"in": ["active", "running", "healthy"]},
    "last_seen": {"gte": "2023-01-01T00:00:00Z"},
    "port": {"gt": 1000}
}

results = await your_query_function(where=where_filters, limit=1000)
execution_time = time.time() - start_time

print(f"Query executed in {execution_time:.3f}s")
print(f"Results: {len(results)} records")
```

### **4. Error Testing**

```python
# Test error handling
try:
    # Invalid MAC address
    await your_query_function(where={"device_mac": {"eq": "invalid-mac"}})
except Exception as e:
    print(f"MAC validation error: {e}")

try:
    # Invalid date range
    await your_query_function(where={"period": {"contains_date": "invalid-date"}})
except Exception as e:
    print(f"Date validation error: {e}")

try:
    # Unsupported operator
    await your_query_function(where={"name": {"unsupported_op": "value"}})
except Exception as e:
    print(f"Operator validation error: {e}")
```

## 📊 **What to Test & Report**

### **✅ Functional Testing**
- [ ] **Existing WHERE clauses still work** (no regressions)
- [ ] **New field types work correctly** (MAC, LTree, DateRange, etc.)
- [ ] **Smart field detection accuracy** (correct auto-detection)
- [ ] **PostgreSQL casting** (check generated SQL is correct)
- [ ] **Error handling** (graceful failures with helpful messages)

### **⚡ Performance Testing**
- [ ] **Query execution time** (should be similar or better than v0.5.8)
- [ ] **Large dataset handling** (1000+ records with complex WHERE clauses)
- [ ] **Complex nested queries** (multiple field types in single WHERE clause)
- [ ] **Memory usage** (no significant memory leaks or spikes)

### **🔧 Integration Testing**
- [ ] **Database compatibility** (your specific PostgreSQL version)
- [ ] **Framework integration** (FastAPI, your web framework)
- [ ] **GraphQL query generation** (proper GraphQL-to-SQL translation)
- [ ] **Type safety** (TypeScript/Python type checking still works)

## 🐛 **How to Report Issues**

### **Issue Template**
```markdown
**FraiseQL Version**: 0.6.0-beta.1
**PostgreSQL Version**: [your version]
**Python Version**: [your version]

**Issue Type**: [Bug/Performance/Compatibility/Enhancement]

**Description**:
[Describe the issue]

**Expected Behavior**:
[What should happen]

**Actual Behavior**:
[What actually happens]

**Reproduction Steps**:
1. [Step 1]
2. [Step 2]
3. [Step 3]

**Code Sample**:
```python
# Minimal code to reproduce the issue
where_filters = {...}
result = await query_function(where=where_filters)
```

**Error Messages** (if any):
```
[Paste full error traceback here]
```

**Generated SQL** (if available):
```sql
-- The actual SQL generated by FraiseQL
```
```

### **Where to Report**
- **GitHub Issues**: [Create issue with "beta-testing" label](https://github.com/lionel-hamayon/fraiseql/issues)
- **Slack/Discord**: [Your team communication channel]
- **Email**: lionel.hamayon@evolution-digitale.fr

## 🎯 **Success Criteria**

We'll consider the beta successful if:

- [ ] **Zero breaking changes** for existing WHERE clause usage
- [ ] **New features work reliably** across different PostgreSQL versions
- [ ] **Performance is maintained or improved** compared to v0.5.8
- [ ] **Field detection accuracy >95%** in real-world scenarios
- [ ] **No critical security vulnerabilities** (SQL injection, etc.)

## ⏱️ **Beta Timeline**

- **Beta Start**: [Today's date]
- **Feedback Deadline**: [2 weeks from today]
- **Release Candidate**: [3 weeks from today]
- **Production Release**: [4 weeks from today]

## 🆘 **Emergency Rollback**

If you encounter critical issues:

```bash
# Rollback to stable version
pip install fraiseql==0.5.8

# Verify rollback
python -c "import fraiseql; print(fraiseql.__version__)"
# Should output: 0.5.8
```

---

## 📈 **What's Next**

After successful beta testing:
- **v0.6.0 Stable Release** with full WHERE refactor
- **Performance optimizations** based on beta feedback
- **Additional field types** (JSON, XML, Custom types)
- **Advanced query patterns** (subqueries, CTEs)

---

**Thank you for beta testing FraiseQL v0.6.0!**

Your feedback helps us deliver a production-ready, high-quality GraphQL framework.

**Questions?** Reach out anytime - we're here to help! 🚀
