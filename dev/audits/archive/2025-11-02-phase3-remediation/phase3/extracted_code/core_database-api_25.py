# Extracted from: docs/core/database-api.md
# Block number: 25
where = {
    "coordinates": {
        "in": [
            (45.5, -122.6),  # Seattle
            (47.6097, -122.3425),  # Pike Place
            (40.7128, -74.0060),  # NYC
        ]
    }
}
# SQL: WHERE (data->>'coordinates')::point IN (POINT(-122.6, 45.5), ...)
