# Extracted from: docs/performance/coordinate_performance_guide.md
# Block number: 1
from functools import lru_cache


@lru_cache(maxsize=1000)
def validate_coordinate_cached(lat: float, lng: float) -> tuple[float, float]:
    # Your validation logic here
    return lat, lng
