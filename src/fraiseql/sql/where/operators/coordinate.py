"""Coordinate-specific SQL operators for PostgreSQL POINT type and geographic queries.

This module provides SQL building functions for coordinate-based filtering operations,
including exact equality, distance calculations, and PostgreSQL POINT type integration.

The operators handle the conversion from user-friendly (latitude, longitude) coordinate
tuples to PostgreSQL's POINT(lng, lat) format.
"""

from psycopg.sql import SQL, Composed, Literal


def build_coordinate_eq_sql(path_sql: SQL, val: tuple[float, float]) -> Composed:
    """Build SQL for coordinate equality comparison using PostgreSQL POINT type.

    Args:
        path_sql: SQL expression for the coordinate field path
        val: Coordinate tuple (latitude, longitude)

    Returns:
        Composed SQL expression for POINT equality

    Example:
        Input: (45.5, -122.6)
        Output: (path)::point = POINT(-122.6, 45.5)
    """
    lat, lng = val
    # PostgreSQL POINT uses (x,y) which maps to (longitude, latitude)
    # User provides (latitude, longitude), so we swap to (longitude, latitude)
    casted_path = Composed([SQL("("), path_sql, SQL(")::point")])
    return Composed(
        [casted_path, SQL(" = POINT("), Literal(lng), SQL(", "), Literal(lat), SQL(")")]
    )


def build_coordinate_neq_sql(path_sql: SQL, val: tuple[float, float]) -> Composed:
    """Build SQL for coordinate inequality comparison using PostgreSQL POINT type.

    Args:
        path_sql: SQL expression for the coordinate field path
        val: Coordinate tuple (latitude, longitude)

    Returns:
        Composed SQL expression for POINT inequality

    Example:
        Input: (45.5, -122.6)
        Output: (path)::point != POINT(-122.6, 45.5)
    """
    lat, lng = val
    casted_path = Composed([SQL("("), path_sql, SQL(")::point")])
    return Composed(
        [casted_path, SQL(" != POINT("), Literal(lng), SQL(", "), Literal(lat), SQL(")")]
    )


def build_coordinate_in_sql(path_sql: SQL, val: list[tuple[float, float]]) -> Composed:
    """Build SQL for coordinate IN list comparison using PostgreSQL POINT type.

    Args:
        path_sql: SQL expression for the coordinate field path
        val: List of coordinate tuples [(lat1, lng1), (lat2, lng2), ...]

    Returns:
        Composed SQL expression for POINT IN list

    Example:
        Input: [(45.5, -122.6), (47.6, -122.3)]
        Output: (path)::point IN (POINT(-122.6, 45.5), POINT(-122.3, 47.6))
    """
    if not isinstance(val, list):
        msg = f"'in' operator requires a list, got {type(val)}"
        raise TypeError(msg)

    casted_path = Composed([SQL("("), path_sql, SQL(")::point")])

    parts = [casted_path, SQL(" IN (")]
    for i, coord in enumerate(val):
        if i > 0:
            parts.append(SQL(", "))
        lat, lng = coord
        parts.extend([SQL("POINT("), Literal(lng), SQL(", "), Literal(lat), SQL(")")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_coordinate_notin_sql(path_sql: SQL, val: list[tuple[float, float]]) -> Composed:
    """Build SQL for coordinate NOT IN list comparison using PostgreSQL POINT type.

    Args:
        path_sql: SQL expression for the coordinate field path
        val: List of coordinate tuples [(lat1, lng1), (lat2, lng2), ...]

    Returns:
        Composed SQL expression for POINT NOT IN list

    Example:
        Input: [(45.5, -122.6), (47.6, -122.3)]
        Output: (path)::point NOT IN (POINT(-122.6, 45.5), POINT(-122.3, 47.6))
    """
    if not isinstance(val, list):
        msg = f"'notin' operator requires a list, got {type(val)}"
        raise TypeError(msg)

    casted_path = Composed([SQL("("), path_sql, SQL(")::point")])

    parts = [casted_path, SQL(" NOT IN (")]
    for i, coord in enumerate(val):
        if i > 0:
            parts.append(SQL(", "))
        lat, lng = coord
        parts.extend([SQL("POINT("), Literal(lng), SQL(", "), Literal(lat), SQL(")")])

    parts.append(SQL(")"))
    return Composed(parts)


def build_coordinate_distance_within_sql(
    path_sql: SQL, center: tuple[float, float], distance_meters: float
) -> Composed:
    """Build SQL for distance-based coordinate filtering using PostGIS ST_DWithin.

    This is the primary distance calculation method when PostGIS is available.
    ST_DWithin provides accurate geodesic distance calculations using the
    spheroid model of the Earth.

    Args:
        path_sql: SQL expression for the coordinate field path
        center: Center coordinate tuple (latitude, longitude)
        distance_meters: Search radius in meters

    Returns:
        Composed SQL expression using ST_DWithin

    Example:
        Input: center=(45.5, -122.6), distance=1000
        Output: ST_DWithin((path)::point, POINT(-122.6, 45.5), 1000)
    """
    lat, lng = center
    casted_path = Composed([SQL("("), path_sql, SQL(")::point")])

    return Composed(
        [
            SQL("ST_DWithin("),
            casted_path,
            SQL(", POINT("),
            Literal(lng),
            SQL(", "),
            Literal(lat),
            SQL("), "),
            Literal(distance_meters),
            SQL(")"),
        ]
    )


def build_coordinate_distance_within_sql_haversine(
    path_sql: SQL, center: tuple[float, float], distance_meters: float
) -> Composed:
    """Build SQL for distance-based coordinate filtering using Haversine formula.

    This is a fallback method when PostGIS is not available. It implements
    the Haversine formula directly in SQL for great-circle distance calculations
    on a spherical Earth model.

    Args:
        path_sql: SQL expression for the coordinate field path
        center: Center coordinate tuple (latitude, longitude)
        distance_meters: Search radius in meters

    Returns:
        Composed SQL expression using Haversine formula

    Note:
        This method is less accurate than PostGIS ST_DWithin but works
        with standard PostgreSQL installations.
    """
    center_lat, center_lng = center

    # Convert degrees to radians and extract lat/lng from POINT
    # PostgreSQL POINT(lng, lat), so we extract accordingly
    return Composed(
        [
            SQL("("),
            SQL("6371000 * 2 * ASIN(SQRT("),  # Earth radius * 2 * arcsin(sqrt(...))
            SQL("POWER(SIN(RADIANS("),
            Literal(center_lat),
            SQL(") - RADIANS(ST_Y(("),
            path_sql,
            SQL(")::point))), 2) / 2 + "),
            SQL("COS(RADIANS("),
            Literal(center_lat),
            SQL(")) * COS(RADIANS(ST_Y(("),
            path_sql,
            SQL(")::point))) * "),
            SQL("POWER(SIN(RADIANS("),
            Literal(center_lng),
            SQL(") - RADIANS(ST_X(("),
            path_sql,
            SQL(")::point))), 2) / 2"),
            SQL(")) <= "),
            Literal(distance_meters),
            SQL(")"),
        ]
    )


def build_coordinate_distance_within_sql_earthdistance(
    path_sql: SQL, center: tuple[float, float], distance_meters: float
) -> Composed:
    """Build SQL for distance-based coordinate filtering using earthdistance module.

    This method uses PostgreSQL's earthdistance extension for distance calculations.
    It's simpler than Haversine but less accurate.

    Args:
        path_sql: SQL expression for the coordinate field path
        center: Center coordinate tuple (latitude, longitude)
        distance_meters: Search radius in meters

    Returns:
        Composed SQL expression using earthdistance functions

    Note:
        Requires the earthdistance PostgreSQL extension to be installed.
    """
    lat, lng = center
    casted_path = Composed([SQL("("), path_sql, SQL(")::point")])

    return Composed(
        [
            SQL("earth_distance(ll_to_earth("),
            Literal(lat),
            SQL(", "),
            Literal(lng),
            SQL("), ll_to_earth(ST_Y("),
            casted_path,
            SQL("), ST_X("),
            casted_path,
            SQL("))) <= "),
            Literal(distance_meters),
        ]
    )
