# Extracted from: docs/core/database-api.md
# Block number: 22
where = {"status": {"eq": "active"}, "machine": {"type": {"eq": "Server"}, "power": {"gte": 100}}}
# SQL: WHERE data->>'status' = 'active'
#      AND data->'machine'->>'type' = 'Server'
#      AND data->'machine'->>'power' >= 100
