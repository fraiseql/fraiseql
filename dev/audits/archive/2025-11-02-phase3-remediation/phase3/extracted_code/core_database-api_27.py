# Extracted from: docs/core/database-api.md
# Block number: 27
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    coordinate_distance_method="haversine",  # default
    # or "postgis" for production
    # or "earthdistance" for legacy systems
)
