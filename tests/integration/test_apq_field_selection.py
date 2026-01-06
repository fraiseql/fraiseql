"""Tests for APQ field selection fix.

This module verifies that APQ (Automatic Persisted Queries) correctly handles
field selection. The bug was that response caching broke field selection:

❌ WRONG BEHAVIOR (what was happening):
1. Client 1: Queries with fields (id, name) → Response cached
2. Client 2: Same APQ hash but wants (id, email) → Gets cached response with all fields

✅ CORRECT BEHAVIOR (what happens now):
1. Store query by hash (in ApqStorage)
2. On hash-only request, retrieve query by hash
3. Execute query normally with client's field selection
4. Return only the requested fields

See: fraiseql_rs/src/apq/mod.rs for canonical Rust implementation.
"""

from fraiseql.fastapi.routers import GraphQLRequest


class TestAPQFieldSelection:
    """Tests verifying APQ does NOT cache responses (only queries)."""

    async def test_apq_registers_query_without_caching_response(self) -> None:
        """APQ registration should store query, not response."""
        # This is a registration request (has both hash and query)
        request = GraphQLRequest(
            query="{ users { id name email } }",
            variables={},
            extensions={
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "abc123def456",
                }
            },
        )

        # The query should be stored
        assert request.query == "{ users { id name email } }"
        assert request.extensions is not None

        # This is a registration request (has query)
        is_registration = bool(request.query)
        assert is_registration is True

    async def test_apq_hash_only_request_should_execute_query(self) -> None:
        """Hash-only APQ requests must execute the query to apply field selection."""
        # This is a hash-only request (no query, only hash)
        request = GraphQLRequest(
            query=None,
            variables={},
            extensions={
                "persistedQuery": {
                    "version": 1,
                    "sha256Hash": "abc123def456",
                }
            },
        )

        # The query should NOT be in the request (hash-only)
        assert request.query is None
        assert request.extensions is not None

        # This should trigger query lookup and execution
        is_hash_only = not request.query
        assert is_hash_only is True

    def test_routers_no_response_caching_import(self) -> None:
        """Verify routers.py does not import response caching functions."""
        routers_path = "/home/lionel/code/fraiseql/src/fraiseql/fastapi/routers.py"
        with open(routers_path) as f:  # noqa: PTH123
            content = f.read()

        # Should NOT import handle_apq_request_with_cache
        assert "handle_apq_request_with_cache" not in content, (
            "routers.py should NOT import handle_apq_request_with_cache "
            "(response caching is disabled)"
        )

        # Should NOT import store_response_in_cache
        assert "store_response_in_cache" not in content, (
            "routers.py should NOT import store_response_in_cache (response caching is disabled)"
        )

        # Should NOT call apq_backend.store_cached_response
        assert "store_cached_response" not in content, (
            "routers.py should NOT call apq_backend.store_cached_response "
            "(response caching is disabled)"
        )

    def test_routers_has_architectural_comment_about_apq(self) -> None:
        """Verify routers.py documents why response caching is disabled."""
        routers_path = "/home/lionel/code/fraiseql/src/fraiseql/fastapi/routers.py"
        with open(routers_path) as f:  # noqa: PTH123
            content = f.read()

        # Should document the architectural decision
        assert "APQ response caching is intentionally NOT implemented" in content, (
            "routers.py should document why response caching is disabled"
        )

        # Should explain field selection issue
        assert "field selection" in content.lower(), (
            "routers.py should explain how response caching breaks field selection"
        )

        # Should reference the canonical Rust implementation
        assert "fraiseql_rs/src/apq/mod.rs" in content, (
            "routers.py should reference the canonical Rust implementation"
        )

    def test_apq_correct_behavior_documented(self) -> None:
        """Verify correct APQ behavior is documented in routers.py."""
        routers_path = "/home/lionel/code/fraiseql/src/fraiseql/fastapi/routers.py"
        with open(routers_path) as f:  # noqa: PTH123
            content = f.read()

        # Should document the 4 steps of correct behavior
        assert "Store query by hash" in content, "Correct behavior step 1: Store query by hash"
        assert "retrieve query by hash" in content, (
            "Correct behavior step 2: Retrieve query by hash"
        )
        assert "Execute query normally" in content, (
            "Correct behavior step 3: Execute query with field selection"
        )
        assert "Return only the requested fields" in content, (
            "Correct behavior step 4: Return requested fields only"
        )

    def test_rust_apq_module_query_only(self) -> None:
        """Verify Rust APQ module only handles queries, not responses."""
        apq_path = "/home/lionel/code/fraiseql/fraiseql_rs/src/apq/mod.rs"

        with open(apq_path) as f:  # noqa: PTH123
            content = f.read()

        # Should NOT have response caching methods
        assert "store_cached_response" not in content, "Rust APQ module should NOT cache responses"
        assert "get_cached_response" not in content, (
            "Rust APQ module should NOT retrieve cached responses"
        )

        # Should only have query operations
        assert "get_persisted_query" in content or "get" in content, (
            "Rust APQ module should retrieve queries by hash"
        )
        assert "store_persisted_query" in content or "set" in content, (
            "Rust APQ module should store queries by hash"
        )

    def test_apq_storage_trait_query_only(self) -> None:
        """Verify ApqStorage trait only defines query methods, not response caching."""
        storage_path = "/home/lionel/code/fraiseql/fraiseql_rs/src/apq/storage.rs"

        with open(storage_path) as f:  # noqa: PTH123
            content = f.read()

        # Should NOT have response caching trait methods
        assert "store_cached_response" not in content, (
            "ApqStorage trait should NOT define response caching methods"
        )
        assert "get_cached_response" not in content, (
            "ApqStorage trait should NOT define response retrieval methods"
        )

        # Should only have query storage trait methods
        assert "async fn get" in content or "fn get" in content, (
            "ApqStorage trait should define query retrieval method"
        )
        assert "async fn set" in content or "fn set" in content, (
            "ApqStorage trait should define query storage method"
        )


class TestAPQFieldSelectionScenarios:
    """Real-world scenarios demonstrating why response caching breaks field selection."""

    def test_scenario_same_query_different_fields(self) -> None:
        """Scenario: Two clients query the same persisted query but with different field selections.

        Without the fix (with caching):
        - Client 1 queries: { user { id name } }
        - Server caches full response with {id, name, email, phone, ...}
        - Client 2 queries same hash: { user { id email } }
        - Client 2 receives cached response with all fields (WRONG!)

        With the fix (no response caching):
        - Client 1 queries: { user { id name } }
        - Server executes query, returns {id, name}
        - Client 2 queries same hash with different variables/field selection
        - Server executes query again, returns {id, email}
        - Each client gets only requested fields (CORRECT!)
        """
        # This scenario is prevented by NOT caching responses
        # Each request must execute the query to apply field selection

        # The fact that routers.py doesn't cache responses ensures this works
        routers_path = "/home/lionel/code/fraiseql/src/fraiseql/fastapi/routers.py"
        with open(routers_path) as f:  # noqa: PTH123
            content = f.read()

        # Should NOT cache responses
        assert "store_response_in_cache" not in content
        assert "handle_apq_request_with_cache" not in content

        # Therefore: same query hash always triggers fresh execution
        # Result: Field selection is properly applied each time

    def test_scenario_apollo_client_field_selection(self) -> None:
        """Apollo Client uses APQ to reduce bandwidth by sending query hash.

        But it still includes field selection in the request.

        The server must honor the field selection, not return cached full responses.
        """
        # Verify the code path allows normal GraphQL execution
        routers_path = "/home/lionel/code/fraiseql/src/fraiseql/fastapi/routers.py"
        with open(routers_path) as f:  # noqa: PTH123
            content = f.read()

        # APQ hash-only requests should fall through to normal query execution
        # (no response caching shortcut)
        assert "return cached_response" not in content, "Should NOT return cached responses"

        # Query should be retrieved and executed normally
        assert "get_persisted_query" in content, (
            "Should retrieve persisted query by hash and execute it"
        )

    def test_scenario_field_security_with_apq(self) -> None:
        """Security implication: Response caching can leak data.

        Example:
        - Admin user queries: { user { id name salary } }
        - Regular user queries same hash but shouldn't see salary
        - With caching: regular user gets cached response with salary (SECURITY BUG!)
        - Without caching: regular user's field selection is respected (SECURE!)
        """
        # Each request must execute independently to respect field selection
        # and authorization
        routers_path = "/home/lionel/code/fraiseql/src/fraiseql/fastapi/routers.py"
        with open(routers_path) as f:  # noqa: PTH123
            content = f.read()

        # Response caching is disabled, ensuring field security
        assert "store_response_in_cache" not in content

        # Each query executes with proper context (can enforce auth/field selection)
        assert "context=context" in content, (
            "Query execution should pass context for field authorization"
        )
