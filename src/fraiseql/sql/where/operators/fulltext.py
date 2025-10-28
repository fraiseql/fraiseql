"""PostgreSQL full-text search operators for FraiseQL WHERE filtering."""

from typing import Optional

from psycopg.sql import SQL, Composed, Literal
from pydantic import BaseModel


def build_matches_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for full-text search using @@ operator with to_tsquery().

    Args:
        field_sql: The SQL for the tsvector field
        value: The search query string

    Returns:
        SQL fragment for the full-text search
    """
    return Composed([field_sql, SQL(" @@ to_tsquery('english', "), Literal(value), SQL(")")])


def build_plain_query_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for plain text query using @@ operator with plainto_tsquery().

    Args:
        field_sql: The SQL for the tsvector field
        value: The plain text search query

    Returns:
        SQL fragment for the plain text search
    """
    return Composed([field_sql, SQL(" @@ plainto_tsquery('english', "), Literal(value), SQL(")")])


def build_phrase_query_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for phrase search using @@ operator with phraseto_tsquery().

    Args:
        field_sql: The SQL for the tsvector field
        value: The phrase search query

    Returns:
        SQL fragment for the phrase search
    """
    return Composed([field_sql, SQL(" @@ phraseto_tsquery('english', "), Literal(value), SQL(")")])


def build_websearch_query_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for web search query using @@ operator with websearch_to_tsquery().

    Args:
        field_sql: The SQL for the tsvector field
        value: The web search query string

    Returns:
        SQL fragment for the web search
    """
    return Composed(
        [field_sql, SQL(" @@ websearch_to_tsquery('english', "), Literal(value), SQL(")")]
    )


def build_rank_gt_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for relevance ranking greater than comparison using ts_rank().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_gt requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) > "),
            Literal(float(threshold)),
        ]
    )


def build_rank_gte_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for relevance ranking greater than or equal comparison using ts_rank().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_gte requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) >= "),
            Literal(float(threshold)),
        ]
    )


def build_rank_lt_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for relevance ranking less than comparison using ts_rank().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_lt requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) < "),
            Literal(float(threshold)),
        ]
    )


def build_rank_lte_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for relevance ranking less than or equal comparison using ts_rank().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_lte requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) <= "),
            Literal(float(threshold)),
        ]
    )


def build_rank_cd_gt_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for cover density ranking greater than comparison using ts_rank_cd().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for cover density rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_cd_gt requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank_cd("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) > "),
            Literal(float(threshold)),
        ]
    )


def build_rank_cd_gte_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for cover density ranking greater than or equal comparison using ts_rank_cd().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for cover density rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_cd_gte requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank_cd("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) >= "),
            Literal(float(threshold)),
        ]
    )


def build_rank_cd_lt_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for cover density ranking less than comparison using ts_rank_cd().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for cover density rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_cd_lt requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank_cd("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) < "),
            Literal(float(threshold)),
        ]
    )


def build_rank_cd_lte_sql(field_sql: SQL | Composed, value: str) -> Composed:
    """Build SQL for cover density ranking less than or equal comparison using ts_rank_cd().

    Args:
        field_sql: The SQL for the tsvector field
        value: Query string and threshold in format "query:threshold"

    Returns:
        SQL fragment for cover density rank comparison
    """
    if ":" not in value:
        raise ValueError("rank_cd_lte requires format 'query:threshold'")
    query, threshold = value.split(":", 1)
    return Composed(
        [
            SQL("ts_rank_cd("),
            field_sql,
            SQL(", to_tsquery('english', "),
            Literal(query),
            SQL(")) <= "),
            Literal(float(threshold)),
        ]
    )


class FullTextFilter(BaseModel):
    """Full-text search filter operators for PostgreSQL tsvector columns.

    Supports PostgreSQL's full-text search capabilities including:
    - Basic text search with @@ operator
    - Plain text queries with plainto_tsquery()
    - Phrase queries with phraseto_tsquery()
    - Web search queries with websearch_to_tsquery()
    - Relevance ranking with ts_rank() and ts_rank_cd()
    """

    # Basic search operators
    matches: Optional[str] = None
    """Full-text search using @@ operator with to_tsquery()."""

    plain_query: Optional[str] = None
    """Plain text query using @@ operator with plainto_tsquery()."""

    # Advanced query types
    phrase_query: Optional[str] = None
    """Phrase search using @@ operator with phraseto_tsquery()."""

    websearch_query: Optional[str] = None
    """Web search query using @@ operator with websearch_to_tsquery()."""

    # Relevance ranking operators (format: "query:threshold")
    rank_gt: Optional[str] = None
    """Filter by relevance rank greater than threshold using ts_rank()."""

    rank_gte: Optional[str] = None
    """Filter by relevance rank greater than or equal to threshold using ts_rank()."""

    rank_lt: Optional[str] = None
    """Filter by relevance rank less than threshold using ts_rank()."""

    rank_lte: Optional[str] = None
    """Filter by relevance rank less than or equal to threshold using ts_rank()."""

    # Cover density ranking operators (format: "query:threshold")
    rank_cd_gt: Optional[str] = None
    """Filter by cover density rank greater than threshold using ts_rank_cd()."""

    rank_cd_gte: Optional[str] = None
    """Filter by cover density rank greater than or equal to threshold using ts_rank_cd()."""

    rank_cd_lt: Optional[str] = None
    """Filter by cover density rank less than threshold using ts_rank_cd()."""

    rank_cd_lte: Optional[str] = None
    """Filter by cover density rank less than or equal to threshold using ts_rank_cd()."""
