import pytest

"""Tests for query complexity analysis."""

from fraiseql.analysis import (
    ComplexityScore,
    analyze_query_complexity,
    calculate_cache_weight,
    should_cache_query,
)


@pytest.mark.unit
class TestComplexityScore:
    """Test the ComplexityScore calculations."""

    def test_simple_query_score(self) -> None:
        """Test scoring for a simple query."""
        score = ComplexityScore(field_count=3, max_depth=1, array_field_count=0, type_diversity=1)

        assert score.total_score < 10
        assert score.cache_weight == 0.1
        assert score.should_cache() is True

    def test_moderate_query_score(self) -> None:
        """Test scoring for a moderate complexity query."""
        score = ComplexityScore(
            field_count=10,
            max_depth=3,
            array_field_count=2,
            type_diversity=3,
            depth_score=15,
            array_score=20,
        )

        total = score.total_score
        assert 25 <= total < 100
        assert 0.5 <= score.cache_weight <= 2.0
        assert score.should_cache() is True

    def test_complex_query_score(self) -> None:
        """Test scoring for a complex query."""
        score = ComplexityScore(
            field_count=50,
            max_depth=5,
            array_field_count=10,
            type_diversity=8,
            depth_score=100,
            array_score=150,
        )

        total = score.total_score
        assert total > 200
        assert score.cache_weight >= 3.0
        assert score.should_cache() is False


class TestQueryComplexityAnalysis:
    """Test the query complexity analyzer."""

    def test_simple_query(self) -> None:
        """Test analysis of a simple query."""
        query = """
        query GetUser {
            user {
                id
                name
                email
            }
        }
        """
        score = analyze_query_complexity(query)

        assert score.field_count == 4  # user, id, name, email
        assert score.max_depth == 2  # query -> user -> fields
        assert score.array_field_count == 0
        assert score.total_score < 20

    def test_nested_query(self) -> None:
        """Test analysis of a nested query."""
        query = """
        query GetUserWithPosts {
            user {
                id
                name
                posts {
                    id
                    title
                    comments {
                        id
                        text
                        author {
                            name
                        }
                    }
                }
            }
        }
        """
        score = analyze_query_complexity(query)

        assert score.field_count >= 9
        assert score.max_depth >= 5
        assert score.array_field_count >= 2  # posts, comments
        assert score.total_score > 50

    def test_query_with_fragments(self) -> None:
        """Test analysis of a query with fragments."""
        query = """
        fragment UserInfo on User {
            id
            name
            email
        }

        query GetUsers {
            users {
                ...UserInfo
                posts {
                    id
                    title
                }
            }
        }
        """
        score = analyze_query_complexity(query)

        assert score.fragment_count == 1
        assert score.field_count >= 6
        assert score.array_field_count >= 1  # users

    def test_mutation_complexity(self) -> None:
        """Test analysis of a mutation."""
        query = """
        mutation CreateUser($input: CreateUserInput!) {
            createUser(input: $input) {
                success
                user {
                    id
                    name
                    email
                }
                errors {
                    field
                    message
                }
            }
        }
        """
        score = analyze_query_complexity(query)

        assert score.field_count >= 8
        assert score.type_diversity >= 1  # Mutation type

    def test_subscription_complexity(self) -> None:
        """Test analysis of a subscription."""
        query = """
        subscription OnCommentAdded($postId: ID!) {
            commentAdded(postId: $postId) {
                id
                text
                author {
                    id
                    name
                }
                post {
                    id
                    title
                }
            }
        }
        """
        score = analyze_query_complexity(query)

        assert score.field_count >= 8
        assert score.type_diversity >= 1  # Subscription type


class TestCacheDecisions:
    """Test cache decision functions."""

    def test_should_cache_simple_query(self) -> None:
        """Test that simple queries should be cached."""
        query = """
        query {
            user {
                id
                name
            }
        }
        """
        should_cache, score = should_cache_query(query)

        assert should_cache is True
        assert score.total_score < 50

    def test_should_not_cache_complex_query(self) -> None:
        """Test that overly complex queries should not be cached."""
        # Create a deeply nested query
        query = """
        query {
            users {
                posts {
                    comments {
                        replies {
                            reactions {
                                user {
                                    profile {
                                        settings {
                                            preferences {
                                                notifications {
                                                    email
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
        should_cache, score = should_cache_query(query, complexity_threshold=100)

        assert should_cache is False
        assert score.total_score > 100

    def test_cache_weight_calculation(self) -> None:
        """Test cache weight calculations for different queries."""
        simple_query = "query { user { name } }"
        moderate_query = """
        query {
            users {
                id
                name
                posts {
                    title
                }
            }
        }
        """
        simple_weight = calculate_cache_weight(simple_query)
        moderate_weight = calculate_cache_weight(moderate_query)

        assert simple_weight < moderate_weight
        assert simple_weight <= 0.5
        assert moderate_weight >= 0.5
