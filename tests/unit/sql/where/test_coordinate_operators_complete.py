"""Comprehensive tests for coordinate operator SQL building."""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.coordinate import (
    build_coordinate_distance_within_sql,
    build_coordinate_distance_within_sql_earthdistance,
    build_coordinate_distance_within_sql_haversine,
    build_coordinate_eq_sql,
    build_coordinate_in_sql,
    build_coordinate_neq_sql,
    build_coordinate_notin_sql,
)


class TestCoordinateBasicOperators:
    """Test basic coordinate comparison operators."""

    def test_eq_coordinate(self):
        """Test coordinate equality."""
        path_sql = SQL("location")
        result = build_coordinate_eq_sql(path_sql, (45.5, -122.6))
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "-122.6" in sql_str and "45.5" in sql_str
        assert "::point" in sql_str
        assert "=" in sql_str

    def test_neq_coordinate(self):
        """Test coordinate inequality."""
        path_sql = SQL("location")
        result = build_coordinate_neq_sql(path_sql, (47.6, -122.3))
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "-122.3" in sql_str and "47.6" in sql_str
        assert "::point" in sql_str
        assert "!=" in sql_str

    def test_in_coordinates(self):
        """Test coordinate IN list."""
        path_sql = SQL("location")
        coords = [(45.5, -122.6), (47.6, -122.3)]
        result = build_coordinate_in_sql(path_sql, coords)
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "-122.6" in sql_str and "45.5" in sql_str
        assert "POINT(" in sql_str and "-122.3" in sql_str and "47.6" in sql_str
        assert "::point" in sql_str
        assert "IN" in sql_str

    def test_notin_coordinates(self):
        """Test coordinate NOT IN list."""
        path_sql = SQL("location")
        coords = [(40.7, -74.0), (34.0, -118.2)]
        result = build_coordinate_notin_sql(path_sql, coords)
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "-74.0" in sql_str and "40.7" in sql_str
        assert "POINT(" in sql_str and "-118.2" in sql_str and "34.0" in sql_str
        assert "::point" in sql_str
        assert "NOT IN" in sql_str


class TestCoordinateDistancePostGIS:
    """Test PostGIS distance calculations."""

    def test_distance_within_postgis(self):
        """Test distance within using PostGIS ST_DWithin."""
        path_sql = SQL("location")
        center = (45.5, -122.6)
        distance = 1000.0
        result = build_coordinate_distance_within_sql(path_sql, center, distance)
        sql_str = result.as_string(None)
        assert "ST_DWithin" in sql_str
        assert "POINT(" in sql_str and "-122.6" in sql_str and "45.5" in sql_str
        assert "1000.0" in sql_str
        assert "::point" in sql_str

    def test_distance_within_postgis_zero_distance(self):
        """Test distance within with zero distance."""
        path_sql = SQL("location")
        center = (0.0, 0.0)
        distance = 0.0
        result = build_coordinate_distance_within_sql(path_sql, center, distance)
        sql_str = result.as_string(None)
        assert "ST_DWithin" in sql_str
        assert "POINT(" in sql_str and "0.0" in sql_str
        assert "0.0" in sql_str


class TestCoordinateDistanceHaversine:
    """Test Haversine distance calculations."""

    def test_distance_within_haversine(self):
        """Test distance within using Haversine formula."""
        path_sql = SQL("location")
        center = (45.5, -122.6)
        distance = 5000.0
        result = build_coordinate_distance_within_sql_haversine(path_sql, center, distance)
        sql_str = result.as_string(None)
        assert "6371000" in sql_str  # Earth radius
        assert "ASIN" in sql_str
        assert "SQRT" in sql_str
        assert "RADIANS" in sql_str
        assert "ST_Y" in sql_str
        assert "ST_X" in sql_str
        assert "5000.0" in sql_str

    def test_distance_within_haversine_equator(self):
        """Test distance within at equator."""
        path_sql = SQL("location")
        center = (0.0, 0.0)
        distance = 10000.0
        result = build_coordinate_distance_within_sql_haversine(path_sql, center, distance)
        sql_str = result.as_string(None)
        assert "RADIANS(0.0)" in sql_str
        assert "10000.0" in sql_str


class TestCoordinateDistanceEarthDistance:
    """Test earthdistance module calculations."""

    def test_distance_within_earthdistance(self):
        """Test distance within using earthdistance extension."""
        path_sql = SQL("location")
        center = (40.7, -74.0)
        distance = 2000.0
        result = build_coordinate_distance_within_sql_earthdistance(path_sql, center, distance)
        sql_str = result.as_string(None)
        assert "earth_distance" in sql_str
        assert "ll_to_earth" in sql_str
        assert "40.7" in sql_str
        assert "-74.0" in sql_str
        assert "2000.0" in sql_str
        assert "ST_Y" in sql_str
        assert "ST_X" in sql_str


class TestCoordinateEdgeCases:
    """Test coordinate operator edge cases."""

    def test_in_requires_list(self):
        """Test that IN operator requires a list."""
        path_sql = SQL("location")
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_coordinate_in_sql(path_sql, "not-a-list")  # type: ignore

    def test_notin_requires_list(self):
        """Test that NOT IN operator requires a list."""
        path_sql = SQL("location")
        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_coordinate_notin_sql(path_sql, "not-a-list")  # type: ignore

    def test_empty_coordinate_list(self):
        """Test empty coordinate list."""
        path_sql = SQL("location")
        result = build_coordinate_in_sql(path_sql, [])
        sql_str = result.as_string(None)
        assert "::point" in sql_str
        assert "IN ()" in sql_str

    def test_single_coordinate_in_list(self):
        """Test single coordinate in list."""
        path_sql = SQL("location")
        coords = [(51.5, -0.1)]  # London
        result = build_coordinate_in_sql(path_sql, coords)
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "-0.1" in sql_str and "51.5" in sql_str
        assert "::point" in sql_str


class TestCoordinateBoundaryValues:
    """Test coordinate boundary values."""

    def test_north_pole(self):
        """Test North Pole coordinates."""
        path_sql = SQL("location")
        result = build_coordinate_eq_sql(path_sql, (90.0, 0.0))
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "0.0" in sql_str and "90.0" in sql_str

    def test_south_pole(self):
        """Test South Pole coordinates."""
        path_sql = SQL("location")
        result = build_coordinate_eq_sql(path_sql, (-90.0, 0.0))
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "0.0" in sql_str and "-90.0" in sql_str

    def test_prime_meridian(self):
        """Test Prime Meridian coordinates."""
        path_sql = SQL("location")
        result = build_coordinate_eq_sql(path_sql, (0.0, 0.0))
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "0.0" in sql_str

    def test_international_date_line(self):
        """Test International Date Line coordinates."""
        path_sql = SQL("location")
        result = build_coordinate_eq_sql(path_sql, (0.0, 180.0))
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "180.0" in sql_str and "0.0" in sql_str

    def test_negative_longitude(self):
        """Test negative longitude (Western Hemisphere)."""
        path_sql = SQL("location")
        result = build_coordinate_eq_sql(path_sql, (40.7, -74.0))  # New York
        sql_str = result.as_string(None)
        assert "POINT(" in sql_str and "-74.0" in sql_str and "40.7" in sql_str
