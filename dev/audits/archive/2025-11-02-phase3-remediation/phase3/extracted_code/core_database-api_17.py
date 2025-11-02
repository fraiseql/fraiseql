# Extracted from: docs/core/database-api.md
# Block number: 17
options = QueryOptions(
    filters={
        "created_at__min": "2024-01-01",
        "created_at__max": "2024-12-31",
        "price__min": 10.00,
        "price__max": 100.00,
    }
)
# SQL: WHERE created_at >= '2024-01-01' AND created_at <= '2024-12-31'
#      AND price >= 10.00 AND price <= 100.00
