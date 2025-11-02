# Extracted from: docs/performance/coordinate_performance_guide.md
# Block number: 2
def validate_coordinates_batch(coordinates: list[tuple[float, float]]) -> list[tuple[float, float]]:
    validated = []
    for coord in coordinates:
        # Validate each coordinate
        validated.append(validate_coordinate(*coord))
    return validated
