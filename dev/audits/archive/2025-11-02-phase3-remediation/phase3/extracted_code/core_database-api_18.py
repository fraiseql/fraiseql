# Extracted from: docs/core/database-api.md
# Block number: 18
options = QueryOptions(filters={"status__in": ["active", "pending", "processing"]})
# SQL: WHERE status IN ('active', 'pending', 'processing')
