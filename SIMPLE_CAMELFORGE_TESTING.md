# Simple CamelForge Testing Guide

## **One-Line Testing**

The simplest way to test CamelForge:

```bash
# Enable CamelForge and run your tests
FRAISEQL_CAMELFORGE_ENABLED=true python your_app.py
```

## **Configuration Options**

### **Method 1: Environment Variables (Recommended for Testing)**

```bash
# Enable CamelForge
export FRAISEQL_CAMELFORGE_ENABLED=true

# Optional: Change the function name
export FRAISEQL_CAMELFORGE_FUNCTION=turbo.fn_camelforge

# Optional: Change field threshold
export FRAISEQL_CAMELFORGE_FIELD_THRESHOLD=30

# Run your app
python your_app.py
```

### **Method 2: Code Configuration**

```python
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://...",
    camelforge_enabled=True,                    # Enable CamelForge
    camelforge_function="turbo.fn_camelforge",  # Function name (optional)
    camelforge_field_threshold=20,              # Field threshold (optional)
)

app = create_fraiseql_app(types=[YourType], config=config)
```

### **Method 3: Mixed (Environment Overrides Config)**

```python
# In code - your default settings
config = FraiseQLConfig(
    database_url="postgresql://...",
    camelforge_enabled=False,  # Disabled by default
)

# Environment variables override config
# FRAISEQL_CAMELFORGE_ENABLED=true will enable it
```

## **Testing Workflow**

### **Step 1: Test Without CamelForge (Baseline)**
```bash
# Ensure CamelForge is disabled
export FRAISEQL_CAMELFORGE_ENABLED=false
python your_app.py

# Run your tests - save results
npm test > results_before.log
```

### **Step 2: Test With CamelForge**
```bash
# Enable CamelForge
export FRAISEQL_CAMELFORGE_ENABLED=true
python your_app.py

# Run same tests - compare results
npm test > results_after.log
```

### **Step 3: Compare Results**
```bash
# Results should be identical
diff results_before.log results_after.log
```

## **What CamelForge Does**

### **Small Queries** (≤ 20 fields by default)
```sql
-- Without CamelForge
SELECT jsonb_build_object('ipAddress', data->>'ip_address') AS result
FROM v_dns_server

-- With CamelForge
SELECT turbo.fn_camelforge(
    jsonb_build_object('ipAddress', data->>'ip_address'),
    'dns_server'
) AS result
FROM v_dns_server
```

### **Large Queries** (> 20 fields by default)
```sql
-- Both with and without CamelForge use the same fallback
SELECT data AS result
FROM v_dns_server
```

## **Performance Expectations**

- **Small queries**: Should be faster with CamelForge
- **Large queries**: Identical performance (automatic fallback)
- **Response format**: Exactly the same JSON structure

## **Troubleshooting**

### **Error: "function turbo.fn_camelforge does not exist"**
```sql
-- Create a simple test function in your database
CREATE OR REPLACE FUNCTION turbo.fn_camelforge(input_data JSONB, entity_type TEXT)
RETURNS JSONB AS $$
BEGIN
    -- For testing, just return the input unchanged
    RETURN input_data;
END;
$$ LANGUAGE plpgsql;
```

### **Performance is Slower**
```bash
# Increase the field threshold to use standard processing more often
export FRAISEQL_CAMELFORGE_FIELD_THRESHOLD=5
```

### **Different Response Format**
```bash
# Disable CamelForge immediately
export FRAISEQL_CAMELFORGE_ENABLED=false
```

## **Quick Rollback**

If anything goes wrong:
```bash
export FRAISEQL_CAMELFORGE_ENABLED=false
# Restart your application
```

## **Example Test Session**

```bash
#!/bin/bash
echo "Testing CamelForge..."

# Test 1: Baseline (disabled)
echo "✅ Testing baseline (CamelForge disabled)"
FRAISEQL_CAMELFORGE_ENABLED=false python -m pytest tests/ -v

# Test 2: CamelForge enabled
echo "✅ Testing with CamelForge enabled"
FRAISEQL_CAMELFORGE_ENABLED=true python -m pytest tests/ -v

echo "🎉 Testing complete!"
```

That's it! CamelForge testing is now as simple as setting one environment variable.
