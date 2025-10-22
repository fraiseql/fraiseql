"""Test coordinate SQL building for WHERE clauses (TDD Red Cycle).

These tests focus on the coordinate filtering operators: equality, distance calculations,
and PostgreSQL POINT type integration.
"""

import pytest
from psycopg.sql import SQL, Literal
from fraiseql.sql.where.operators.coordinate import (
    build_coordinate_eq_sql,
    build_coordinate_neq_sql,
    build_coordinate_in_sql,
    build_coordinate_notin_sql,
    build_coordinate_distance_within_sql,
)


class TestCoordinateSQLBuilding:
    """Test coordinate SQL building functionality."""

    def test_build_coordinate_equality_sql(self):
        """Should build proper POINT casting for coordinate equality."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'location')")
        result = build_coordinate_eq_sql(path_sql, (45.5, -122.6))

        # Should generate: ((data ->> 'location'))::point = POINT(-122.6, 45.5)
        sql_str = result.as_string(None)
        assert "::point = POINT(" in sql_str
        assert "data ->> 'location'" in sql_str
        assert "-122.6" in sql_str and "45.5" in sql_str  # PostgreSQL POINT uses (lng, lat) order

    def test_build_coordinate_inequality_sql(self):
        """Should build proper POINT casting for coordinate inequality."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'coordinates')")
        result = build_coordinate_neq_sql(path_sql, (47.6097, -122.3425))

        # Should generate: (data ->> 'coordinates')::point != POINT(-122.3425, 47.6097)
        sql_str = result.as_string(None)
        assert "data ->> 'coordinates'" in sql_str
        assert "::point != POINT(" in sql_str
        assert "-122.3425, 47.6097" in sql_str

    def test_build_coordinate_in_list_sql(self):
        """Should build proper POINT casting for coordinate IN lists."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'position')")
        coord_list = [(45.5, -122.6), (47.6097, -122.3425), (40.7128, -74.0060)]
        result = build_coordinate_in_sql(path_sql, coord_list)

        # Should generate: (data ->> 'position')::point IN (POINT(-122.6, 45.5), POINT(-122.3425, 47.6097), POINT(-74.0060, 40.7128))
        sql_str = result.as_string(None)
        assert "data ->> 'position'" in sql_str
        assert "IN (" in sql_str
        assert "POINT(" in sql_str and "-122.6" in sql_str and "45.5" in sql_str
        assert "POINT(" in sql_str and "-122.3425" in sql_str and "47.6097" in sql_str
        assert "POINT(" in sql_str and "-74.006" in sql_str and "40.7128" in sql_str

    def test_build_coordinate_not_in_list_sql(self):
        """Should build proper POINT casting for coordinate NOT IN lists."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'location')")
        coord_list = [(0, 0), (90, 180)]
        result = build_coordinate_notin_sql(path_sql, coord_list)

        # Should generate: (data ->> 'location')::point NOT IN (POINT(0, 0), POINT(180, 90))
        sql_str = result.as_string(None)
        assert "data ->> 'location'" in sql_str
        assert "NOT IN (" in sql_str
        assert "POINT(0, 0)" in sql_str
        assert "POINT(180, 90)" in sql_str

    def test_build_coordinate_distance_within_sql_postgis(self):
        """Should build PostGIS ST_DWithin for distance calculations."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'coordinates')")
        center = (45.5, -122.6)
        meters = 1000
        result = build_coordinate_distance_within_sql(path_sql, center, meters)

        # Should generate: ST_DWithin((data ->> 'coordinates')::point, POINT(-122.6, 45.5), 1000)
        sql_str = result.as_string(None)
        assert "ST_DWithin(" in sql_str
        assert "data ->> 'coordinates'" in sql_str
        assert "::point" in sql_str
        assert "POINT(" in sql_str and "-122.6" in sql_str and "45.5" in sql_str
        assert "1000" in sql_str

    def test_build_coordinate_distance_within_sql_haversine(self):
        """Should build Haversine formula for distance calculations (fallback)."""
        # Test the fallback Haversine implementation
        from fraiseql.sql.where.operators.coordinate import (
            build_coordinate_distance_within_sql_haversine,
        )

        path_sql = SQL("(data ->> 'location')")
        center = (47.6097, -122.3425)  # Seattle coordinates
        meters = 5000
        result = build_coordinate_distance_within_sql_haversine(path_sql, center, meters)

        # Should generate complex Haversine formula
        sql_str = result.as_string(None)
        assert "data ->> 'location'" in sql_str
        assert "ASIN" in sql_str  # Haversine uses ASIN
        assert "SIN" in sql_str  # Haversine uses SIN
        assert "COS" in sql_str  # Haversine uses COS
        assert "6371000" in sql_str  # Earth radius in meters

    def test_coordinate_point_order_conversion(self):
        """Test that coordinates are properly converted from (lat,lng) to POINT(lng,lat)."""
        # PostgreSQL POINT uses (x,y) which maps to (longitude, latitude)
        # But users provide coordinates as (latitude, longitude)
        # So (45.5, -122.6) should become POINT(-122.6, 45.5)

        path_sql = SQL("(data ->> 'coords')")
        result = build_coordinate_eq_sql(path_sql, (45.5, -122.6))

        sql_str = result.as_string(None)
        assert (
            "POINT(" in sql_str and "-122.6" in sql_str and "45.5" in sql_str
        )  # lng first, then lat
        assert "POINT(45.5, -122.6)" not in sql_str  # NOT lat first

    def test_coordinate_edge_cases(self):
        """Test coordinate edge cases like poles and date line."""
        # North pole
        path_sql = SQL("(data ->> 'position')")
        result = build_coordinate_eq_sql(path_sql, (90, 45))  # lat=90, lng=45
        sql_str = result.as_string(None)
        assert "POINT(45, 90)" in sql_str

        # South pole
        result = build_coordinate_eq_sql(path_sql, (-90, 135))  # lat=-90, lng=135
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "135" in sql_str and "-90" in sql_str

        # International date line (lng=180)
        result = build_coordinate_eq_sql(path_sql, (0, 180))  # lat=0, lng=180
        sql_str = result.as_string(None)
        assert "POINT(180, 0)" in sql_str

        # International date line negative (lng=-180)
        result = build_coordinate_eq_sql(path_sql, (0, -180))  # lat=0, lng=-180
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "-180" in sql_str and "0" in sql_str
