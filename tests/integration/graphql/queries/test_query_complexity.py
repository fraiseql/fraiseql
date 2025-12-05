"""Tests for GraphQL query complexity analysis and limiting.

Following TDD principles, these tests are written before the implementation.
"""

import pytest

from fraiseql.gql.complexity import (
    ComplexityConfig,
    ComplexityError,
    QueryComplexityAnalyzer,
    calculate_field_complexity,
)

pytestmark = pytest.mark.integration


class TestFieldComplexity:
    """Test field-level complexity calculations."""

    def test_scalar_field_complexity(self) -> None:
        """Test complexity of scalar fields."""
        # Scalar fields have base complexity of 1
        assert calculate_field_complexity("id", is_list=False) == 1
        assert calculate_field_complexity("name", is_list=False) == 1
        assert calculate_field_complexity("age", is_list=False) == 1

    def test_list_field_complexity(self) -> None:
        """Test complexity of list fields."""
        # List fields without nested = 1 + (size * 0) = 1
        assert calculate_field_complexity("items", is_list=True, estimated_size=10) == 1
        # With nested complexity
        assert (
            calculate_field_complexity(
                "users", is_list=True, estimated_size=10, nested_complexity=3
            )
            == 31
        )

    def test_list_field_with_limit(self) -> None:
        """Test complexity when list has explicit limit."""
        # Should use actual limit instead of estimate
        assert calculate_field_complexity("items", is_list=True, limit=5) == 1
        # With nested complexity
        assert calculate_field_complexity("users", is_list=True, limit=5, nested_complexity=2) == 11

    def test_nested_object_complexity(self) -> None:
        """Test complexity of nested objects."""
        # Object fields add their child complexity
        nested_complexity = 5  # Sum of nested field complexities
        assert (
            calculate_field_complexity(
                "author", is_object=True, nested_complexity=nested_complexity
            )
            == 6
        )

    def test_custom_multiplier(self) -> None:
        """Test field with custom complexity multiplier."""
        # Some fields might be more expensive (e.g., computed fields)
        assert calculate_field_complexity("computed_field", multiplier=3) == 3
        # List with multiplier: base(2) + limit * nested(0) = 2
        assert (
            calculate_field_complexity("expensive_list", is_list=True, limit=10, multiplier=2) == 2
        )
        # List with multiplier and nested: base(2) + 10 * 1 = 12
        assert (
            calculate_field_complexity(
                "expensive_list", is_list=True, limit=10, multiplier=2, nested_complexity=1
            )
            == 12
        )


class TestQueryComplexityAnalyzer:
    """Test the main query complexity analyzer."""

    @pytest.fixture
    def analyzer(self) -> None:
        """Create analyzer with default config."""
        config = ComplexityConfig(
            max_complexity=1000,
            max_depth=10,
            default_list_size=10,
            field_multipliers={
                "search": 5,  # Search is expensive
                "aggregate": 10,  # Aggregations are very expensive
            },
        )
        return QueryComplexityAnalyzer(config)

    def test_simple_query_complexity(self, analyzer) -> None:
        """Test complexity of simple query."""
        query = """
        query {
            user(id: "123") {
                id
                name
                email
            }
        }
        """
        complexity = analyzer.analyze(query)

        # user(1) + id(1) + name(1) + email(1) = 4
        assert complexity.total_score == 4
        assert complexity.depth == 2
        assert complexity.field_count == 4

    def test_list_query_complexity(self, analyzer) -> None:
        """Test complexity of query with lists."""
        query = """
        query {
            users(limit: 50) {
                id
                name
                posts {
                    id
                    title
                }
            }
        }
        """
        # This query exceeds the default limit of 1000
        with pytest.raises(ComplexityError) as exc_info:
            analyzer.analyze(query)

        assert exc_info.value.complexity == 1151
        assert "exceeds maximum complexity" in str(exc_info.value)

    def test_query_exceeds_max_complexity(self, analyzer) -> None:
        """Test query that exceeds maximum complexity."""
        query = """
        query {
            users {
                id
                posts {
                    id
                    comments {
                        id
                        replies {
                            id
                            text
                        }
                    }
                }
            }
        }
        """
        with pytest.raises(ComplexityError) as exc_info:
            analyzer.analyze(query)

        assert "exceeds maximum complexity" in str(exc_info.value)
        assert exc_info.value.complexity > 1000

    def test_query_exceeds_max_depth(self, analyzer) -> None:
        """Test query that exceeds maximum depth."""
        query = """
        query {
            level1 {
                level2 {
                    level3 {
                        level4 {
                            level5 {
                                level6 {
                                    level7 {
                                        level8 {
                                            level9 {
                                                level10 {
                                                    level11 {
                                                        tooDeep
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        """
        with pytest.raises(ComplexityError) as exc_info:
            analyzer.analyze(query)

        assert "exceeds maximum depth" in str(exc_info.value)
        assert exc_info.value.depth > 10

    def test_fragment_complexity(self, analyzer) -> None:
        """Test complexity calculation with fragments."""
        query = """
        query {
            users(limit: 20) {
                ...UserFields
                posts {
                    ...PostFields
                }
            }
        }

        fragment UserFields on User {
            id
            name
            email
        }

        fragment PostFields on Post {
            id
            title
            content
        }
        """
        complexity = analyzer.analyze(query)

        # Should properly calculate fragment fields
        # users(1) + 20 * (UserFields(3) + posts(1) + 10 * PostFields(3))
        # With the new calculation:
        # users is a list: 1 + 20 * (3 + 1 + 10 * 3) = 1 + 20 * 34 = 681
        assert complexity.total_score == 681

    def test_custom_field_multipliers(self, analyzer) -> None:
        """Test fields with custom complexity multipliers."""
        query = """
        query {
            search(query: "test", limit: 10) {
                id
                name
            }
            aggregate(groupBy: "category") {
                count
                sum
            }
        }
        """
        complexity = analyzer.analyze(query)

        # search is a list with multiplier of 5
        # search base = 5, nested = 2 (id + name)
        # search total = 5 + 10 * 2 = 25
        # aggregate has multiplier of 10
        # aggregate base = 10, nested = 2 (count + sum)
        # aggregate total = 10 + 2 = 12
        # Total: 25 + 12 = 37
        assert complexity.total_score == 37

    def test_introspection_query_blocked(self, analyzer) -> None:
        """Test that introspection queries can be blocked."""
        analyzer.config.allow_introspection = False

        query = """
        query {
            __schema {
                types {
                    name
                }
            }
        }
        """
        with pytest.raises(ComplexityError) as exc_info:
            analyzer.analyze(query)

        assert "Introspection queries are not allowed" in str(exc_info.value)

    def test_variables_in_complexity(self, analyzer) -> None:
        """Test complexity calculation with variables."""
        query = """
        query GetUsers($limit: Int = 10) {
            users(limit: $limit) {
                id
                name
            }
        }
        """
        # With no variables provided, should use default from query
        complexity = analyzer.analyze(query)
        # users is a list, so: 1 + 10 * 2 = 21
        assert complexity.total_score == 21

        # With variables provided
        complexity = analyzer.analyze(query, variables={"limit": 50})
        # users is a list, so: 1 + 50 * 2 = 101
        assert complexity.total_score == 101

    def test_complexity_info_object(self, analyzer) -> None:
        """Test the complexity info object returned."""
        query = """
        query {
            users(limit: 5) {
                id
                posts(limit: 3) {
                    id
                }
            }
        }
        """
        complexity = analyzer.analyze(query)

        # users is a list: 1 + 5 * (id(1) + posts(1 + 3 * id(1)))
        # = 1 + 5 * (1 + 4) = 1 + 5 * 5 = 26
        assert complexity.total_score == 26
        assert complexity.depth == 3
        assert complexity.field_count == 4  # users + id + posts + id (inside posts)
        assert len(complexity.field_scores) > 0
        assert "users" in complexity.field_scores


class TestComplexityIntegration:
    """Test integration with FraiseQL execution."""

    @pytest.mark.asyncio
    async def test_query_rejected_before_execution(self) -> None:
        """Test that complex queries are rejected before database execution."""
        # This would test actual integration with query execution
        # For now, it's a placeholder showing intended behavior

    @pytest.mark.asyncio
    async def test_complexity_included_in_response_extensions(self) -> None:
        """Test that complexity info can be included in GraphQL response."""
        # This would test that complexity calculations can be exposed
        # in the response extensions for debugging


class TestQueryComplexityConfig:
    """Test configuration options for complexity analysis."""

    def test_default_config(self) -> None:
        """Test default configuration values."""
        config = ComplexityConfig()

        assert config.max_complexity == 1000
        assert config.max_depth == 10
        assert config.default_list_size == 10
        assert config.enabled is True
        assert config.include_in_response is False

    def test_custom_config(self) -> None:
        """Test custom configuration."""
        config = ComplexityConfig(
            max_complexity=5000,
            max_depth=15,
            default_list_size=50,
            field_multipliers={"search": 10},
            enabled=True,
            include_in_response=True,
            allow_introspection=False,
        )

        assert config.max_complexity == 5000
        assert config.max_depth == 15
        assert config.default_list_size == 50
        assert config.field_multipliers["search"] == 10
        assert config.enabled is True
        assert config.include_in_response is True
        assert config.allow_introspection is False

    def test_disabled_complexity_checking(self) -> None:
        """Test that complexity checking can be disabled."""
        config = ComplexityConfig(enabled=False)
        analyzer = QueryComplexityAnalyzer(config)

        # Should not raise even for complex query
        query = """
        query {
            users {
                posts {
                    comments {
                        replies {
                            nested {
                                veryDeep
                            }
                        }
                    }
                }
            }
        }
        """
        complexity = analyzer.analyze(query)
        assert complexity is not None  # Should still calculate but not enforce
