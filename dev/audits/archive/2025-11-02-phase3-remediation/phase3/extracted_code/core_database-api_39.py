# Extracted from: docs/core/database-api.md
# Block number: 39
from fraiseql.db.sql_builder import build_filter_conditions_and_params

filters = {"status": "active", "price__min": 10.00, "tags__in": ["electronics", "gadgets"]}

conditions, params = build_filter_conditions_and_params(filters)
# conditions: ["status = %s", "price >= %s", "tags IN (%s, %s)"]
# params: ("active", 10.00, "electronics", "gadgets")
