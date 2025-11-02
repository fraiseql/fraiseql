# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 19
# GOOD: tenant_id first in WHERE clause
SELECT * FROM orders
WHERE tenant_id = 'uuid' AND status = 'completed'
ORDER BY created_at DESC
LIMIT 10;

# BAD: Missing tenant_id filter
SELECT * FROM orders
WHERE user_id = 'uuid'
ORDER BY created_at DESC;

# GOOD: Explicit tenant_id
SELECT * FROM orders
WHERE tenant_id = 'uuid' AND user_id = 'uuid'
ORDER BY created_at DESC;
