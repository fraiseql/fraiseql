# Extracted from: docs/api-reference/database.md
# Block number: 14
where = {
    "age": {"gte": 18, "lt": 65},  # Greater than or equal, less than
    "status": {"in": ["active", "pending"]},  # IN operator
    "email": {"like": "%@example.com"},  # LIKE operator
    "deleted_at": {"is": None},  # IS NULL
    "score": {"between": [10, 20]},  # BETWEEN
}
