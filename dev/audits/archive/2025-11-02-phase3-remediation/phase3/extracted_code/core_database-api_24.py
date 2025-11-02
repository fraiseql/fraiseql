# Extracted from: docs/core/database-api.md
# Block number: 24
# Dictionary-based filtering
where = {
    "coordinates": {"eq": (45.5, -122.6)}  # (latitude, longitude)
}
results = await repo.find("locations", where=where)
# SQL: WHERE (data->>'coordinates')::point = POINT(-122.6, 45.5)
