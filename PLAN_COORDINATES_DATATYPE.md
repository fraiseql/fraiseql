# Coordinates Datatype Implementation - COMPLEX

**Complexity**: Complex | **Phased TDD Approach**

## Executive Summary
Add geographic coordinates (latitude/longitude) as a custom scalar type with validation for lat (-90 to +90) and lng (-180 to +180). This follows FraiseQL's existing scalar type pattern (IpAddressField, LTreeField, etc.) with PostgreSQL POINT type integration.

## Architecture Context

**Files to Understand**:
- `/home/lionel/code/fraiseql/src/fraiseql/types/scalars/ip_address.py` - Scalar validation pattern
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operator_strategies.py` - Operator strategy registration
- `/home/lionel/code/fraiseql/tests/unit/core/type_system/test_ltree.py` - Type system test pattern

**Pattern**:
1. GraphQL ScalarType with `serialize()`, `parse_value()`, `parse_literal()`
2. Python marker class inheriting from `ScalarMarker`
3. Optional OperatorStrategy for specialized filters
4. Registration in type system and operator registry

---

## PHASES

### Phase 1: Core Coordinate Scalar Type
**Objective**: Create validated coordinate scalar with lat/lng validation

#### TDD Cycle:

**1. RED**: Write failing test for coordinate validation
- Test file: `tests/unit/core/type_system/test_coordinates.py`
- Expected failures:
  - Valid coordinates accepted: `(45.5, -122.6)` ✓
  - Latitude bounds: `-90 ≤ lat ≤ 90` ✗
  - Longitude bounds: `-180 ≤ lng ≤ 180` ✗
  - Invalid formats rejected ✗

```python
# Test cases to write:
def test_valid_coordinates():
    # Valid coords: (0, 0), (45.5, -122.6), (-90, 180), (90, -180)
    assert parse_coordinate_value("45.5,-122.6") == (45.5, -122.6)

def test_latitude_bounds_validation():
    # Should raise: (91, 0), (-91, 0)
    with pytest.raises(GraphQLError, match="Latitude must be between -90 and 90"):
        parse_coordinate_value("91,0")

def test_longitude_bounds_validation():
    # Should raise: (0, 181), (0, -181)
    with pytest.raises(GraphQLError, match="Longitude must be between -180 and 180"):
        parse_coordinate_value("0,181")

def test_coordinate_formats():
    # Support: "45.5,-122.6", (45.5, -122.6), {"lat": 45.5, "lng": -122.6}
    # PostgreSQL POINT format: "(45.5,-122.6)"
```

**2. GREEN**: Implement minimal coordinate scalar
- File to create: `src/fraiseql/types/scalars/coordinates.py`
- Minimal implementation:
  - `serialize_coordinate()` - Convert to string `"lat,lng"`
  - `parse_coordinate_value()` - Parse string/tuple/dict with validation
  - `parse_coordinate_literal()` - Parse GraphQL AST
  - `CoordinateScalar` - GraphQL scalar type
  - `CoordinateField` - Python marker class

```python
# Key validation logic:
def parse_coordinate_value(value: Any) -> tuple[float, float]:
    lat, lng = _extract_lat_lng(value)

    # Validate latitude bounds
    if not (-90 <= lat <= 90):
        raise GraphQLError(f"Latitude must be between -90 and 90, got {lat}")

    # Validate longitude bounds
    if not (-180 <= lng <= 180):
        raise GraphQLError(f"Longitude must be between -180 and 180, got {lng}")

    return (lat, lng)
```

**3. REFACTOR**: Clean up coordinate parsing
- Support multiple input formats consistently
- Add PostgreSQL POINT type compatibility `POINT(lng, lat)` (note: PostGIS uses lng,lat order)
- Follow IpAddressField pattern for code organization
- Add comprehensive docstrings

**4. QA**: Verify coordinate scalar works
- [ ] All unit tests pass
- [ ] Validates latitude bounds: -90 to +90
- [ ] Validates longitude bounds: -180 to +180
- [ ] Accepts multiple input formats
- [ ] Follows FraiseQL scalar pattern

```bash
uv run pytest tests/unit/core/type_system/test_coordinates.py -v
```

---

### Phase 2: Type System Integration
**Objective**: Register coordinate type in FraiseQL's type system

#### TDD Cycle:

**1. RED**: Write failing test for type system registration
- Test file: `tests/unit/core/type_system/test_coordinates.py` (extend)
- Expected failure: CoordinateField not in type registry

```python
def test_coordinate_field_in_type_registry():
    from fraiseql.types import Coordinate, CoordinateField
    # Should be importable and usable in models
    assert Coordinate is not None
    assert CoordinateField is not None

def test_coordinate_field_graphql_scalar():
    from fraiseql.types.scalars.coordinates import CoordinateScalar
    # Should have proper GraphQL scalar
    assert CoordinateScalar.name == "Coordinate"
```

**2. GREEN**: Register coordinate type
- Files to modify:
  - `src/fraiseql/types/__init__.py` - Export Coordinate type
  - `src/fraiseql/types/scalars/__init__.py` - Export CoordinateField, CoordinateScalar

```python
# In src/fraiseql/types/__init__.py:
from fraiseql.types.scalars.coordinates import CoordinateField as Coordinate
from fraiseql.types.scalars.coordinates import CoordinateScalar

# In src/fraiseql/types/scalars/__init__.py:
from .coordinates import CoordinateField, CoordinateScalar
```

**3. REFACTOR**: Ensure consistent type naming
- Follow naming convention: `Coordinate` (GraphQL) vs `CoordinateField` (Python marker)
- Add type to documentation
- Verify imports work from expected paths

**4. QA**: Verify type registration
- [ ] Type importable from `fraiseql.types`
- [ ] GraphQL schema includes Coordinate scalar
- [ ] Follows naming conventions
- [ ] No circular import issues

```bash
uv run pytest tests/unit/core/type_system/ -v
```

---

### Phase 3: Coordinate Operator Strategy (Optional Enhancements)
**Objective**: Add geographic operators for coordinate filtering

#### TDD Cycle:

**1. RED**: Write failing tests for coordinate operators
- Test file: `tests/unit/sql/where/test_coordinate_operators_sql_building.py`
- Expected failures: Geographic operators not implemented

```python
def test_coordinate_equality():
    # (lat,lng) = (45.5,-122.6)
    path_sql = SQL("data->>'location'")
    result = coordinate.build_coordinate_eq_sql(path_sql, (45.5, -122.6))
    expected = "(data->>'location')::point = POINT(-122.6, 45.5)"  # PostGIS order
    assert result.as_string(None) == expected

def test_coordinate_distance_within():
    # location <-> POINT(lng,lat) < distance_meters
    # Using PostgreSQL earth_distance() or PostGIS ST_DWithin()
    path_sql = SQL("data->>'location'")
    center = (45.5, -122.6)
    meters = 1000

    result = coordinate.build_coordinate_distance_within_sql(path_sql, center, meters)
    # Should generate earth_distance or ST_DWithin query
```

**2. GREEN**: Implement coordinate operator strategy
- File to modify: `src/fraiseql/sql/operator_strategies.py`
- Add `CoordinateOperatorStrategy` class:

```python
class CoordinateOperatorStrategy(BaseOperatorStrategy):
    """Strategy for geographic coordinate operators with PostgreSQL POINT type."""

    def __init__(self) -> None:
        super().__init__([
            "eq", "neq", "in", "notin",  # Basic operations
            "distance_within",  # Distance filtering (PostGIS or earthdistance)
        ])

    def build_sql(self, path_sql: SQL, op: str, val: Any, field_type: type | None = None) -> Composed:
        if op == "eq":
            lat, lng = val
            # PostgreSQL POINT uses (lng, lat) order - opposite of common (lat, lng)
            casted_path = Composed([SQL("("), path_sql, SQL(")::point")])
            return Composed([casted_path, SQL(" = POINT("), Literal(lng), SQL(", "), Literal(lat), SQL(")")])

        elif op == "distance_within":
            # Requires PostGIS or earthdistance extension
            center, distance_meters = val
            # Implementation depends on available PostgreSQL extensions
```

**3. REFACTOR**: Implement configurable distance calculation strategy
- **Configurable Options**:
  - **PostGIS `ST_DWithin`** (most accurate, requires PostGIS extension)
  - **Manual Haversine formula** (good accuracy, no extension needed)
  - **PostgreSQL `earthdistance` module** (simpler, less accurate)
- Add app configuration to select distance calculation method
- Add proper distance operator support (`distance_within`, `distance_lte`, `distance_gte`)
- Validate chosen method availability at startup

**4. QA**: Verify coordinate operators
- [ ] Basic equality works with POINT casting
- [ ] Distance operators generate correct SQL
- [ ] Handles PostgreSQL (lng,lat) vs common (lat,lng) order correctly
- [ ] Integration tests pass with real database

```bash
uv run pytest tests/unit/sql/where/test_coordinate_operators_sql_building.py -v
uv run pytest tests/integration/database/sql/test_coordinate_filter_operations.py -v
```

---

### Phase 4: Integration Testing
**Objective**: End-to-end validation with real database operations

#### TDD Cycle:

**1. RED**: Write failing integration tests
- Test file: `tests/integration/database/sql/test_coordinate_filter_operations.py`
- Expected failures: Database operations not working

```python
@pytest.fixture
def location_model():
    class Location:
        id: int
        name: str
        coordinates: Coordinate  # Uses new CoordinateField type
    return Location

async def test_filter_by_exact_coordinates(db, location_model):
    # Insert test data with coordinates
    # Filter by exact match: where={coordinates: {eq: (45.5, -122.6)}}
    # Verify results match expected location

async def test_filter_by_coordinate_bounds(db, location_model):
    # Filter locations within lat/lng bounds
    # Verify all results are within valid coordinate ranges

async def test_invalid_coordinate_rejected(db, location_model):
    # Try to insert invalid coordinates (lat=95)
    # Should raise validation error
```

**2. GREEN**: Fix integration issues
- Ensure PostgreSQL POINT type storage works
- Fix any serialization/deserialization issues
- Handle edge cases (poles, date line crossing)

**3. REFACTOR**: Optimize coordinate operations
- Add database indexes for coordinate fields if needed
- Consider GiST index for spatial queries
- Optimize distance calculations

**4. QA**: Full integration validation
- [ ] CRUD operations work with coordinates
- [ ] Filters work correctly
- [ ] Invalid coordinates rejected at all layers
- [ ] Performance acceptable for coordinate queries
- [ ] PostgreSQL POINT type integration verified

```bash
uv run pytest tests/integration/database/sql/test_coordinate_filter_operations.py -v
uv run pytest --tb=short  # Full test suite
```

---

### Phase 5: Documentation and Examples
**Objective**: Document coordinate type usage and provide examples

#### TDD Cycle:

**1. RED**: Write failing documentation tests
- Test file: `tests/integration/examples/test_coordinate_examples.py`
- Expected failures: Example code doesn't run

**2. GREEN**: Create working examples
- File to create: `examples/coordinates_example.py`
- Minimal example:

```python
from fraiseql import Database
from fraiseql.types import Coordinate

class Restaurant:
    id: int
    name: str
    location: Coordinate

db = Database(...)

# Insert with coordinates
restaurant = await db.Restaurant.create({
    "name": "Pike Place Market",
    "location": (47.6097, -122.3425)  # Seattle
})

# Query by exact location
results = await db.Restaurant.find(where={
    "location": {"eq": (47.6097, -122.3425)}
})

# Find nearby (if distance operators implemented)
nearby = await db.Restaurant.find(where={
    "location": {"distance_within": ((47.6097, -122.3425), 5000)}  # 5km radius
})
```

**3. REFACTOR**: Add comprehensive documentation
- Update `docs/core/types-and-schema.md` with Coordinate type
- Add coordinate validation documentation
- Document PostgreSQL POINT type integration
- Add migration guide for existing location data

**4. QA**: Documentation quality check
- [ ] Examples run successfully
- [ ] Documentation accurate and clear
- [ ] Migration guide tested
- [ ] All coordinate features documented

```bash
uv run pytest tests/integration/examples/test_coordinate_examples.py -v
```

---

## Success Criteria

- [x] CoordinateField scalar type created with validation
- [x] Latitude validation: -90 ≤ lat ≤ 90
- [x] Longitude validation: -180 ≤ lng ≤ 180
- [x] Multiple input formats supported
- [x] Type registered in FraiseQL type system
- [x] PostgreSQL POINT type integration
- [x] Operator strategy for coordinate filtering (optional)
- [x] Configurable distance calculation (ST_DWithin vs Haversine)
- [x] Integration tests pass
- [x] Documentation complete
- [x] Examples working

## Implementation Notes

### PostgreSQL POINT Type
- PostgreSQL POINT uses `(x, y)` which maps to `(longitude, latitude)` - **opposite of common (lat, lng) order**
- Storage: `POINT(lng, lat)` in database
- Input/output: Accept `(lat, lng)` in GraphQL for user convenience

### Validation Edge Cases
- Poles: `(90, *)` and `(-90, *)` are valid
- Prime Meridian: `(*, 0)` is valid
- Date line: `(*, 180)` and `(*, -180)` are same meridian
- Precision: Support decimal degrees with arbitrary precision

### Distance Calculations (Configurable - Phase 3)
Distance calculation strategy will be **configurable in the app** with these options:

- **PostGIS `ST_DWithin`** (recommended, most accurate)
  - Requires PostGIS extension
  - Handles geography/spheroid calculations correctly
  - Best performance for complex spatial queries

- **Manual Haversine formula** (fallback, good accuracy)
  - No PostgreSQL extensions required
  - Pure Python implementation using spherical trigonometry
  - Good approximation for most use cases

- **PostgreSQL `earthdistance` module** (legacy option)
  - Requires earthdistance extension
  - Simpler but less accurate than Haversine
  - Limited to point-to-point distance calculations

**Configuration**: App setting to choose calculation method with validation that required extensions are available.

### Configuration Implementation
The distance calculation method will be configurable via app settings:

```python
# In app configuration
class AppConfig:
    coordinate_distance_method: Literal["postgis", "haversine", "earthdistance"] = "haversine"

# Validation at startup
def validate_distance_method(method: str) -> None:
    if method == "postgis":
        # Check PostGIS extension is available
        if not has_postgis_extension():
            raise ConfigurationError("PostGIS extension required for 'postgis' distance method")
    elif method == "earthdistance":
        # Check earthdistance extension is available
        if not has_earthdistance_extension():
            raise ConfigurationError("earthdistance extension required for 'earthdistance' distance method")
```

---



**Estimated Effort**: 8-12 hours
- Phase 1: 2-3 hours (core scalar)
- Phase 2: 1 hour (registration)
- Phase 3: 3-4 hours (operators - optional)
- Phase 4: 2-3 hours (integration)
- Phase 5: 1-2 hours (docs)

**Dependencies**:
- PostgreSQL database with POINT type support (built-in)
- Optional: PostGIS extension for advanced geographic queries

**Risk Areas**:
- PostgreSQL (lng,lat) vs common (lat,lng) order confusion
- Coordinate precision and floating-point comparison
- Distance calculation accuracy without PostGIS
- Configuration validation (ensuring chosen method's extensions are available)
- Performance differences between calculation methods
