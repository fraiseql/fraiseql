"""Integration tests for WHERE clause functionality.

This package contains integration tests organized by operator type:
- network/ - Network operator tests (IP, MAC, hostname, email, port)
- specialized/ - PostgreSQL-specific tests (ltree, fulltext)
- temporal/ - Time-related tests (date, datetime, daterange)
- spatial/ - Spatial/coordinate tests

Root level contains mixed-type and cross-cutting integration tests.
"""
