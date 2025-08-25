"""Field threshold tests for FraiseQL.

This module contains tests for GraphQL field limit threshold functionality
which optimizes queries that request many fields by returning the full data
column instead of building individual JSONB objects.
"""
