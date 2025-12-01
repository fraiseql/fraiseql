import pytest

"""Extended tests for query complexity analysis to improve coverage."""

from graphql import parse

from fraiseql.analysis import (
    ComplexityScore,
    QueryComplexityAnalyzer,
    analyze_query_complexity,
    calculate_cache_weight,
    should_cache_query,
)
from fraiseql.analysis.complexity_config import (
    BALANCED_CONFIG,
    RELAXED_CONFIG,
    STRICT_CONFIG,
    ComplexityConfig,
)


@pytest.mark.unit
class TestComplexityScoreExtended:
    """Extended tests for ComplexityScore class."""

    def test_cache_weight_boundaries(self) -> None:
        """Test cache weight at different complexity boundaries."""
        # Very simple query
        score = ComplexityScore(field_count=1, max_depth=1)
        assert score.cache_weight == 0.1

        # At simple/moderate boundary
        score = ComplexityScore(field_count=10, depth_score=0)
        assert 0.1 <= score.cache_weight <= 0.5

        # At moderate/complex boundary
        score = ComplexityScore(field_count=40, depth_score=10)
        assert score.cache_weight >= 0.5

        # Very complex query
        score = ComplexityScore(
            field_count=100, depth_score=200, array_score=100, type_diversity=20
        )
        assert score.cache_weight > 3.0

    def test_should_cache_custom_threshold(self) -> None:
        """Test should_cache with custom thresholds."""
        score = ComplexityScore(field_count=50, depth_score=50)

        # Should not cache with low threshold
        assert score.should_cache(threshold=50) is False

        # Should cache with high threshold
        assert score.should_cache(threshold=500) is True

    def test_total_score_components(self) -> None:
        """Test that all components contribute to total score."""
        # Base score
        score1 = ComplexityScore(field_count=10)
        base_total = score1.total_score

        # Add depth
        score2 = ComplexityScore(field_count=10, depth_score=20)
        assert score2.total_score > base_total

        # Add arrays
        score3 = ComplexityScore(field_count=10, depth_score=20, array_score=30)
        assert score3.total_score > score2.total_score

        # Add type diversity
        score4 = ComplexityScore(field_count=10, depth_score=20, array_score=30, type_diversity=5)
        assert score4.total_score > score3.total_score


class TestQueryComplexityAnalyzerExtended:
    """Extended tests for QueryComplexityAnalyzer class."""

    def test_analyzer_initialization(self) -> None:
        """Test analyzer initialization with different configs."""
        # Default initialization
        analyzer1 = QueryComplexityAnalyzer()
        assert analyzer1.schema is None
        assert analyzer1.config is not None
        assert analyzer1.score.field_count == 0

        # With custom config
        custom_config = ComplexityConfig(base_field_cost=2)
        analyzer2 = QueryComplexityAnalyzer(config=custom_config)
        assert analyzer2.config.base_field_cost == 2

        # State is properly initialized
        assert len(analyzer1.types_accessed) == 0
        assert len(analyzer1.fragments) == 0
        assert analyzer1.current_depth == 0

    def test_analyze_with_document_node(self) -> None:
        """Test analyze with pre-parsed DocumentNode."""
        query_str = "query { user { id name } }"
        document = parse(query_str)

        analyzer = QueryComplexityAnalyzer()
        score = analyzer.analyze(document)

        assert isinstance(score, ComplexityScore)
        assert score.field_count > 0

    def test_inline_fragments(self) -> None:
        """Test analysis of inline fragments."""
        query = """
        query GetContent {
            content {
                id
                ... on Article {
                    title
                    body
                }
                ... on Video {
                    title
                    url
                    duration
                }
            }
        }
        """
        score = analyze_query_complexity(query)

        # Should track type diversity from inline fragments
        assert score.type_diversity >= 2  # At least Article and Video
        assert score.field_count >= 6

    def test_multiple_operations(self) -> None:
        """Test document with multiple operations."""
        query = """
        query GetUser {
            user { id name }
        }

        mutation UpdateUser {
            updateUser { id name }
        }

        subscription UserUpdates {
            userUpdated { id name }
        }
        """
        analyzer = QueryComplexityAnalyzer()
        score = analyzer.analyze(query)

        # Should track all operation types
        assert "Query" in analyzer.types_accessed
        assert "Mutation" in analyzer.types_accessed
        assert "Subscription" in analyzer.types_accessed
        assert score.type_diversity >= 3

    def test_deeply_nested_arrays(self) -> None:
        """Test scoring of deeply nested array fields."""
        query = """
        query {
            organizations {
                departments {
                    teams {
                        members {
                            projects {
                                tasks {
                                    id
                                }
                            }
                        }
                    }
                }
            }
        }
        """
        score = analyze_query_complexity(query)

        assert score.array_field_count >= 5
        assert score.max_depth >= 6
        assert score.array_score > 100  # High due to nested arrays
        assert score.total_score > 150  # Adjust threshold based on actual scoring

    def test_fragment_spread_handling(self) -> None:
        """Test handling of fragment spreads."""
        query = """
        fragment UserFields on User {
            id
            name
            email
        }

        query GetUsers {
            users {
                ...UserFields
                posts {
                    id
                }
            }
            currentUser {
                ...UserFields
            }
        }
        """
        analyzer = QueryComplexityAnalyzer()
        score = analyzer.analyze(query)

        assert score.fragment_count == 1
        assert "UserFields" in analyzer.fragments
        # Fragment spread handling is simplified, but should still track fragments

    def test_array_field_detection_patterns(self) -> None:
        """Test array field pattern detection."""
        queries = [
            ("query { items { id } }", True),
            ("query { userList { id } }", True),
            ("query { allPosts { id } }", True),
            ("query { manyThings { id } }", True),
            ("query { collection { id } }", True),
            ("query { user { id } }", False),
            ("query { post { id } }", False),
        ]

        for query, should_have_arrays in queries:
            score = analyze_query_complexity(query)
            if should_have_arrays:
                assert score.array_field_count > 0
            else:
                assert score.array_field_count == 0


class TestComplexityConfigExtended:
    """Extended tests for ComplexityConfig class."""

    def test_preset_configs(self) -> None:
        """Test preset configuration values."""
        # Strict config
        assert STRICT_CONFIG.depth_multiplier == 2.0
        assert STRICT_CONFIG.array_field_multiplier == 15
        assert STRICT_CONFIG.complex_query_threshold == 150

        # Relaxed config
        assert RELAXED_CONFIG.depth_multiplier == 1.2
        assert RELAXED_CONFIG.array_field_multiplier == 5
        assert RELAXED_CONFIG.complex_query_threshold == 500

        # Balanced is default
        assert BALANCED_CONFIG.depth_multiplier == 1.5

    def test_custom_config_in_analysis(self) -> None:
        """Test using custom configs in analysis."""
        query = """
        query {
            users {
                posts {
                    comments {
                        id
                    }
                }
            }
        }
        """
        # Analyze with strict config
        strict_score = analyze_query_complexity(query, config=STRICT_CONFIG)

        # Analyze with relaxed config
        relaxed_score = analyze_query_complexity(query, config=RELAXED_CONFIG)

        # Strict should produce higher scores
        assert strict_score.total_score > relaxed_score.total_score
        assert strict_score.array_score > relaxed_score.array_score

    def test_config_singleton_pattern(self) -> None:
        """Test config singleton management."""
        # Get default
        default1 = ComplexityConfig.get_default()
        default2 = ComplexityConfig.get_default()
        assert default1 is default2

        # Set new default
        custom = ComplexityConfig(base_field_cost=5)
        ComplexityConfig.set_default(custom)

        # Verify it changed
        new_default = ComplexityConfig.get_default()
        assert new_default.base_field_cost == 5
        assert new_default is custom

        # Reset to normal default
        ComplexityConfig._default = None

    def test_depth_penalty_bounds(self) -> None:
        """Test depth penalty calculation with bounds."""
        config = ComplexityConfig(max_depth_penalty=50)

        # Normal depths
        assert config.calculate_depth_penalty(0) == 0
        assert config.calculate_depth_penalty(1) == 1
        assert config.calculate_depth_penalty(2) > 1

        # Very deep should hit max
        assert config.calculate_depth_penalty(100) == 50

    def test_array_penalty_calculation(self) -> None:
        """Test array penalty calculation."""
        config = ComplexityConfig()

        # No arrays
        assert config.calculate_array_penalty(0, 0) == 0
        assert config.calculate_array_penalty(5, 0) == 0

        # Arrays at different depths
        shallow = config.calculate_array_penalty(1, 2)
        deep = config.calculate_array_penalty(5, 2)
        assert deep > shallow

    def test_cache_weight_boundaries(self) -> None:
        """Test cache weight calculation at boundaries."""
        config = ComplexityConfig(
            simple_query_threshold=10, moderate_query_threshold=50, complex_query_threshold=200
        )

        # Below simple threshold
        assert config.get_cache_weight(5) == config.simple_query_weight

        # At boundaries
        assert config.get_cache_weight(10) == config.moderate_query_weight
        assert config.get_cache_weight(50) == config.complex_query_weight

        # Above complex threshold
        weight = config.get_cache_weight(400)
        assert weight > config.complex_query_weight
        assert weight == config.complex_query_weight * 2.0  # 400/200

    def test_array_field_patterns_extended(self) -> None:
        """Test extended array field pattern matching."""
        config = ComplexityConfig()

        # Test plural detection
        assert config.is_array_field("users") is True
        assert config.is_array_field("posts") is True
        assert config.is_array_field("as") is False  # Too short

        # Test pattern matching
        assert config.is_array_field("itemList") is True
        assert config.is_array_field("allUsers") is True
        assert config.is_array_field("userCollection") is True
        assert config.is_array_field("getManyThings") is True

        # Test non-array fields
        assert config.is_array_field("user") is False
        assert config.is_array_field("post") is False
        assert config.is_array_field("single") is False


class TestCachingDecisionsExtended:
    """Extended tests for caching decision functions."""

    def test_should_cache_with_configs(self) -> None:
        """Test cache decisions with different configs."""
        complex_query = """
        query {
            users {
                posts {
                    comments {
                        replies {
                            text
                        }
                    }
                }
            }
        }
        """
        # Should not cache with strict config
        should_cache_strict, _ = should_cache_query(complex_query, config=STRICT_CONFIG)
        assert should_cache_strict is False

        # Might cache with relaxed config
        _should_cache_relaxed, _ = should_cache_query(complex_query, config=RELAXED_CONFIG)
        # Relaxed has higher threshold

    def test_cache_weight_with_schema(self) -> None:
        """Test cache weight calculation with schema (None schema test)."""
        query = "query { user { name } }"

        # Without schema
        weight1 = calculate_cache_weight(query, schema=None)

        # With schema (None in this test, but validates the parameter)
        weight2 = calculate_cache_weight(query, schema=None, config=RELAXED_CONFIG)

        assert isinstance(weight1, float)
        assert isinstance(weight2, float)
        assert 0.1 <= weight1 <= 10.0

    def test_edge_case_queries(self) -> None:
        """Test edge case queries."""
        # Simple query (empty selection is invalid GraphQL)
        simple_query = "query { __typename }"
        score = analyze_query_complexity(simple_query)
        assert score.field_count >= 1

        # Query with only fragments
        fragment_only = """
        fragment UserInfo on User {
            id
            name
        }
        """
        score = analyze_query_complexity(fragment_only)
        assert score.fragment_count == 1

        # Malformed but parseable
        simple = "{ user }"
        score = analyze_query_complexity(simple)
        assert score.field_count >= 1


class TestAnalyzerStateManagement:
    """Test analyzer state management across multiple analyses."""

    def test_analyzer_state_reset(self) -> None:
        """Test that analyzer properly resets state between analyses."""
        analyzer = QueryComplexityAnalyzer()

        # First analysis
        query1 = """
        query GetUser {
            user { id name }
        }
        """
        score1 = analyzer.analyze(query1)

        # Second analysis with different query
        query2 = """
        mutation UpdateUser {
            updateUser { success }
        }
        """
        score2 = analyzer.analyze(query2)

        # Scores should be independent
        assert score1.field_count != score2.field_count
        assert len(analyzer.types_accessed) > 0  # Should have types from last query

        # Third analysis to verify proper reset
        query3 = "query { simple }"
        score3 = analyzer.analyze(query3)
        assert score3.field_count == 1
