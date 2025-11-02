# Extracted from: docs/core/database-api.md
# Block number: 20
# Dictionary-based filtering
where = {"machine": {"name": {"eq": "Server-01"}}}
results = await repo.find("allocations", where=where)
# SQL: WHERE data->'machine'->>'name' = 'Server-01'
