# Extracted from: docs/enterprise/RBAC_POSTGRESQL_ASSESSMENT.md
# Block number: 3
# Manual invalidation required
# 1. User updates role via GraphQL mutation
# 2. PostgreSQL function executes
# 3. Python code must manually call redis.delete()
# 4. Easy to forget = stale permission bugs
# 5. No CASCADE support = complex invalidation logic
