# Extracted from: docs/strategic/V1_VISION.md
# Block number: 5
# Simple equality
where = {"status": "active"}
# → data->>'status' = 'active'

# Operators
where = {"age": {"gt": 18}, "name": {"contains": "john"}}
# → data->>'age' > '18' AND data->>'name' LIKE '%john%'
