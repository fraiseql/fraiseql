"""Comprehensive tests for full-text search operators."""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.fulltext import (
    build_matches_sql,
    build_phrase_query_sql,
    build_plain_query_sql,
    build_rank_cd_gt_sql,
    build_rank_cd_gte_sql,
    build_rank_cd_lt_sql,
    build_rank_cd_lte_sql,
    build_rank_gt_sql,
    build_rank_gte_sql,
    build_rank_lt_sql,
    build_rank_lte_sql,
    build_websearch_query_sql,
)


class TestBasicFullTextSearch:
    """Test basic full-text search operators."""

    def test_matches_basic(self):
        """Test basic full-text search with to_tsquery."""
        field_sql = SQL("document_tsv")
        result = build_matches_sql(field_sql, "database & optimization")
        sql_str = result.as_string(None)
        assert sql_str == "document_tsv @@ to_tsquery('english', 'database & optimization')"

    def test_matches_with_operators(self):
        """Test full-text search with boolean operators."""
        field_sql = SQL("content_tsv")
        result = build_matches_sql(field_sql, "python | javascript")
        sql_str = result.as_string(None)
        assert sql_str == "content_tsv @@ to_tsquery('english', 'python | javascript')"

    def test_matches_with_phrases(self):
        """Test full-text search with phrases."""
        field_sql = SQL("article_tsv")
        result = build_matches_sql(field_sql, '"machine learning"')
        sql_str = result.as_string(None)
        assert sql_str == "article_tsv @@ to_tsquery('english', '\"machine learning\"')"

    def test_plain_query_basic(self):
        """Test plain text query search."""
        field_sql = SQL("description_tsv")
        result = build_plain_query_sql(field_sql, "natural language processing")
        sql_str = result.as_string(None)
        assert (
            sql_str
            == "description_tsv @@ plainto_tsquery('english', 'natural language processing')"
        )

    def test_phrase_query_basic(self):
        """Test phrase query search."""
        field_sql = SQL("title_tsv")
        result = build_phrase_query_sql(field_sql, "artificial intelligence")
        sql_str = result.as_string(None)
        assert sql_str == "title_tsv @@ phraseto_tsquery('english', 'artificial intelligence')"

    def test_websearch_query_basic(self):
        """Test web search query."""
        field_sql = SQL("body_tsv")
        result = build_websearch_query_sql(field_sql, "machine learning OR AI")
        sql_str = result.as_string(None)
        assert sql_str == "body_tsv @@ websearch_to_tsquery('english', 'machine learning OR AI')"

    def test_websearch_with_quotes(self):
        """Test web search with quoted phrases."""
        field_sql = SQL("content_tsv")
        result = build_websearch_query_sql(field_sql, '"deep learning" -neural')
        sql_str = result.as_string(None)
        assert (
            sql_str == "content_tsv @@ websearch_to_tsquery('english', '\"deep learning\" -neural')"
        )


class TestRelevanceRankingOperators:
    """Test relevance ranking comparison operators."""

    def test_rank_gt_basic(self):
        """Test rank greater than comparison."""
        field_sql = SQL("document_tsv")
        result = build_rank_gt_sql(field_sql, "database:0.5")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank(document_tsv, to_tsquery('english', 'database')) > 0.5"

    def test_rank_gt_with_complex_query(self):
        """Test rank greater than with complex query."""
        field_sql = SQL("content_tsv")
        result = build_rank_gt_sql(field_sql, "machine & learning:0.3")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank(content_tsv, to_tsquery('english', 'machine & learning')) > 0.3"

    def test_rank_gte_basic(self):
        """Test rank greater than or equal comparison."""
        field_sql = SQL("article_tsv")
        result = build_rank_gte_sql(field_sql, "optimization:0.2")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank(article_tsv, to_tsquery('english', 'optimization')) >= 0.2"

    def test_rank_lt_basic(self):
        """Test rank less than comparison."""
        field_sql = SQL("title_tsv")
        result = build_rank_lt_sql(field_sql, "tutorial:0.8")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank(title_tsv, to_tsquery('english', 'tutorial')) < 0.8"

    def test_rank_lte_basic(self):
        """Test rank less than or equal comparison."""
        field_sql = SQL("description_tsv")
        result = build_rank_lte_sql(field_sql, "guide:0.1")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank(description_tsv, to_tsquery('english', 'guide')) <= 0.1"


class TestCoverDensityRankingOperators:
    """Test cover density ranking comparison operators."""

    def test_rank_cd_gt_basic(self):
        """Test cover density rank greater than comparison."""
        field_sql = SQL("document_tsv")
        result = build_rank_cd_gt_sql(field_sql, "database:0.4")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank_cd(document_tsv, to_tsquery('english', 'database')) > 0.4"

    def test_rank_cd_gte_basic(self):
        """Test cover density rank greater than or equal comparison."""
        field_sql = SQL("content_tsv")
        result = build_rank_cd_gte_sql(field_sql, "algorithm:0.6")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank_cd(content_tsv, to_tsquery('english', 'algorithm')) >= 0.6"

    def test_rank_cd_lt_basic(self):
        """Test cover density rank less than comparison."""
        field_sql = SQL("article_tsv")
        result = build_rank_cd_lt_sql(field_sql, "research:0.9")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank_cd(article_tsv, to_tsquery('english', 'research')) < 0.9"

    def test_rank_cd_lte_basic(self):
        """Test cover density rank less than or equal comparison."""
        field_sql = SQL("title_tsv")
        result = build_rank_cd_lte_sql(field_sql, "paper:0.7")
        sql_str = result.as_string(None)
        assert sql_str == "ts_rank_cd(title_tsv, to_tsquery('english', 'paper')) <= 0.7"


class TestFullTextEdgeCases:
    """Test edge cases for full-text search operators."""

    def test_matches_with_special_characters(self):
        """Test full-text search with special characters."""
        field_sql = SQL("content_tsv")
        result = build_matches_sql(field_sql, "C++ & Java")
        sql_str = result.as_string(None)
        assert "C++ & Java" in sql_str

    def test_plain_query_with_numbers(self):
        """Test plain query with numbers."""
        field_sql = SQL("description_tsv")
        result = build_plain_query_sql(field_sql, "version 2.1 update")
        sql_str = result.as_string(None)
        assert "version 2.1 update" in sql_str

    def test_phrase_query_with_hyphens(self):
        """Test phrase query with hyphens."""
        field_sql = SQL("title_tsv")
        result = build_phrase_query_sql(field_sql, "state-of-the-art")
        sql_str = result.as_string(None)
        assert "state-of-the-art" in sql_str

    def test_websearch_with_operators(self):
        """Test web search with various operators."""
        field_sql = SQL("body_tsv")
        result = build_websearch_query_sql(
            field_sql, '"exact phrase" OR (this AND that) NOT exclude'
        )
        sql_str = result.as_string(None)
        assert '"exact phrase" OR (this AND that) NOT exclude' in sql_str


class TestRankingErrorHandling:
    """Test error handling for ranking operators."""

    def test_rank_gt_missing_colon(self):
        """Test rank_gt with missing colon separator."""
        field_sql = SQL("document_tsv")
        with pytest.raises(ValueError, match="rank_gt requires format 'query:threshold'"):
            build_rank_gt_sql(field_sql, "database")

    def test_rank_gt_invalid_threshold(self):
        """Test rank_gt with invalid threshold."""
        field_sql = SQL("document_tsv")
        with pytest.raises(ValueError, match="could not convert string to float"):
            build_rank_gt_sql(field_sql, "database:invalid")  # type: ignore

    def test_rank_gte_missing_colon(self):
        """Test rank_gte with missing colon separator."""
        field_sql = SQL("content_tsv")
        with pytest.raises(ValueError, match="rank_gte requires format 'query:threshold'"):
            build_rank_gte_sql(field_sql, "algorithm")

    def test_rank_lt_missing_colon(self):
        """Test rank_lt with missing colon separator."""
        field_sql = SQL("title_tsv")
        with pytest.raises(ValueError, match="rank_lt requires format 'query:threshold'"):
            build_rank_lt_sql(field_sql, "tutorial")

    def test_rank_lte_missing_colon(self):
        """Test rank_lte with missing colon separator."""
        field_sql = SQL("description_tsv")
        with pytest.raises(ValueError, match="rank_lte requires format 'query:threshold'"):
            build_rank_lte_sql(field_sql, "guide")

    def test_rank_cd_gt_missing_colon(self):
        """Test rank_cd_gt with missing colon separator."""
        field_sql = SQL("document_tsv")
        with pytest.raises(ValueError, match="rank_cd_gt requires format 'query:threshold'"):
            build_rank_cd_gt_sql(field_sql, "database")

    def test_rank_cd_gte_missing_colon(self):
        """Test rank_cd_gte with missing colon separator."""
        field_sql = SQL("content_tsv")
        with pytest.raises(ValueError, match="rank_cd_gte requires format 'query:threshold'"):
            build_rank_cd_gte_sql(field_sql, "algorithm")

    def test_rank_cd_lt_missing_colon(self):
        """Test rank_cd_lt with missing colon separator."""
        field_sql = SQL("article_tsv")
        with pytest.raises(ValueError, match="rank_cd_lt requires format 'query:threshold'"):
            build_rank_cd_lt_sql(field_sql, "research")

    def test_rank_cd_lte_missing_colon(self):
        """Test rank_cd_lte with missing colon separator."""
        field_sql = SQL("title_tsv")
        with pytest.raises(ValueError, match="rank_cd_lte requires format 'query:threshold'"):
            build_rank_cd_lte_sql(field_sql, "paper")


class TestRankingBoundaryValues:
    """Test ranking operators with boundary values."""

    def test_rank_gt_zero_threshold(self):
        """Test rank greater than with zero threshold."""
        field_sql = SQL("document_tsv")
        result = build_rank_gt_sql(field_sql, "query:0.0")
        sql_str = result.as_string(None)
        assert ") > 0.0" in sql_str

    def test_rank_lt_one_threshold(self):
        """Test rank less than with one threshold."""
        field_sql = SQL("content_tsv")
        result = build_rank_lt_sql(field_sql, "search:1.0")
        sql_str = result.as_string(None)
        assert ") < 1.0" in sql_str

    def test_rank_cd_gte_high_threshold(self):
        """Test cover density rank greater than or equal with high threshold."""
        field_sql = SQL("article_tsv")
        result = build_rank_cd_gte_sql(field_sql, "term:0.95")
        sql_str = result.as_string(None)
        assert ") >= 0.95" in sql_str

    def test_rank_cd_lte_low_threshold(self):
        """Test cover density rank less than or equal with low threshold."""
        field_sql = SQL("title_tsv")
        result = build_rank_cd_lte_sql(field_sql, "word:0.05")
        sql_str = result.as_string(None)
        assert ") <= 0.05" in sql_str
