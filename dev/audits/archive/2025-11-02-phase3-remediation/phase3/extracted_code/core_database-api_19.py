# Extracted from: docs/core/database-api.md
# Block number: 19
options = QueryOptions(
    filters={
        "category": "electronics",
        "price__max": 500.00,
        "in_stock": True,
        "vendor__in": ["vendor-a", "vendor-b"],
    }
)
# SQL: WHERE category = 'electronics'
#      AND price <= 500.00
#      AND in_stock = TRUE
#      AND vendor IN ('vendor-a', 'vendor-b')
