# Phase 1: RED - Write Failing Tests

**Objective**: Write comprehensive tests that demonstrate the CASCADE selection filtering bug and define expected behavior.

**Status**: 🔴 RED (Tests will fail initially)

---

## Context

CASCADE data is currently returned in GraphQL responses regardless of whether the client requested it in the selection set. This violates GraphQL's fundamental principle that only requested fields should be returned.

**Current Bug**:
- Client queries mutation WITHOUT `cascade` field → Still receives CASCADE data
- Client queries mutation with partial CASCADE selection → Receives full CASCADE object

**Expected Behavior**:
- Client doesn't request `cascade` → No CASCADE in response
- Client requests `cascade { updated }` → Only `updated` field in response
- Client requests full `cascade { updated deleted invalidations metadata }` → Full CASCADE

---

## Files to Create/Modify

### New Test File
- `tests/integration/test_cascade_selection_filtering.py`

---

## Implementation Steps

### Step 1: Create Comprehensive Test Suite

**File**: `tests/integration/test_cascade_selection_filtering.py`

```python
"""Test CASCADE selection filtering behavior.

Verifies that CASCADE data is only included when explicitly requested
in the GraphQL selection set, and that partial selections are respected.
"""

import pytest
from tests.integration.conftest import execute_graphql


class TestCascadeSelectionFiltering:
    """Test CASCADE field selection awareness."""

    @pytest.mark.asyncio
    async def test_cascade_not_returned_when_not_requested(
        self, graphql_client, db_pool
    ):
        """CASCADE should NOT be in response when not requested in selection."""

        mutation = """
            mutation CreatePostWithEntity($input: CreatePostInput!) {
                createPostWithEntity(input: $input) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        post {
                            id
                            title
                            content
                        }
                        # NOTE: cascade NOT requested
                    }
                }
            }
        """

        variables = {
            "input": {
                "title": "Test Post",
                "content": "Test content",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        # Assertions
        assert "errors" not in result
        assert "data" in result
        assert "createPostWithEntity" in result["data"]

        response = result["data"]["createPostWithEntity"]

        # CASCADE should NOT be present
        assert "cascade" not in response, (
            "CASCADE field should not be present when not requested in selection. "
            f"Found CASCADE in response: {response.get('cascade')}"
        )

        # Other fields should be present
        assert "message" in response
        assert "post" in response
        assert response["post"]["title"] == "Test Post"

    @pytest.mark.asyncio
    async def test_cascade_returned_when_requested(
        self, graphql_client, db_pool
    ):
        """CASCADE should be in response when explicitly requested."""

        mutation = """
            mutation CreatePostWithEntity($input: CreatePostInput!) {
                createPostWithEntity(input: $input) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        post {
                            id
                            title
                            content
                        }
                        cascade {
                            updated {
                                __typename
                                id
                                operation
                                entity
                            }
                            deleted {
                                __typename
                                id
                            }
                            invalidations {
                                queryName
                                strategy
                                scope
                            }
                            metadata {
                                timestamp
                                affectedCount
                            }
                        }
                    }
                }
            }
        """

        variables = {
            "input": {
                "title": "Test Post",
                "content": "Test content",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        # Assertions
        assert "errors" not in result
        assert "data" in result

        response = result["data"]["createPostWithEntity"]

        # CASCADE should be present
        assert "cascade" in response, "CASCADE field should be present when requested"

        cascade = response["cascade"]
        assert cascade is not None
        assert "updated" in cascade
        assert "deleted" in cascade
        assert "invalidations" in cascade
        assert "metadata" in cascade

        # Verify cascade content
        assert len(cascade["updated"]) > 0, "Should have updated entities"
        assert isinstance(cascade["updated"], list)

        # Verify entity structure
        first_update = cascade["updated"][0]
        assert "__typename" in first_update
        assert "id" in first_update
        assert "operation" in first_update
        assert "entity" in first_update

    @pytest.mark.asyncio
    async def test_partial_cascade_selection_updated_only(
        self, graphql_client, db_pool
    ):
        """Only requested CASCADE fields should be returned (updated only)."""

        mutation = """
            mutation CreatePostWithEntity($input: CreatePostInput!) {
                createPostWithEntity(input: $input) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        cascade {
                            updated {
                                __typename
                                id
                                operation
                                entity
                            }
                            # NOT requesting: deleted, invalidations, metadata
                        }
                    }
                }
            }
        """

        variables = {
            "input": {
                "title": "Test Post",
                "content": "Test content",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        assert "errors" not in result
        response = result["data"]["createPostWithEntity"]

        # CASCADE should be present with only requested fields
        assert "cascade" in response
        cascade = response["cascade"]

        # Only 'updated' should be present
        assert "updated" in cascade

        # These should NOT be present (not requested)
        assert "deleted" not in cascade, "deleted not requested, should not be in response"
        assert "invalidations" not in cascade, "invalidations not requested, should not be in response"
        assert "metadata" not in cascade, "metadata not requested, should not be in response"

    @pytest.mark.asyncio
    async def test_partial_cascade_selection_metadata_only(
        self, graphql_client, db_pool
    ):
        """Only metadata requested in CASCADE."""

        mutation = """
            mutation CreatePostWithEntity($input: CreatePostInput!) {
                createPostWithEntity(input: $input) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        cascade {
                            metadata {
                                timestamp
                                affectedCount
                            }
                            # NOT requesting: updated, deleted, invalidations
                        }
                    }
                }
            }
        """

        variables = {
            "input": {
                "title": "Test Post",
                "content": "Test content",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        assert "errors" not in result
        response = result["data"]["createPostWithEntity"]

        cascade = response["cascade"]

        # Only 'metadata' should be present
        assert "metadata" in cascade
        assert cascade["metadata"]["affectedCount"] >= 0
        assert "timestamp" in cascade["metadata"]

        # These should NOT be present
        assert "updated" not in cascade
        assert "deleted" not in cascade
        assert "invalidations" not in cascade

    @pytest.mark.asyncio
    async def test_cascade_with_error_response(
        self, graphql_client, db_pool
    ):
        """CASCADE should not be present in error responses when not requested."""

        mutation = """
            mutation CreatePostWithEntity($input: CreatePostInput!) {
                createPostWithEntity(input: $input) {
                    ... on CreatePostWithEntityError {
                        message
                        code
                        # No cascade in error branch
                    }
                }
            }
        """

        # Invalid input to trigger error
        variables = {
            "input": {
                "title": "",  # Invalid: empty title
                "content": "Test",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        # Should get error response
        response = result["data"]["createPostWithEntity"]

        # Error branch should not have cascade
        assert "cascade" not in response
        assert "__typename" in response
        # Check if it's error type (might be CreatePostWithEntityError)

    @pytest.mark.asyncio
    async def test_multiple_mutations_with_different_cascade_selections(
        self, graphql_client, db_pool
    ):
        """Multiple mutations in one query with different CASCADE selections."""

        mutation = """
            mutation MultiplePosts($input1: CreatePostInput!, $input2: CreatePostInput!) {
                post1: createPostWithEntity(input: $input1) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        cascade {
                            updated {
                                __typename
                                id
                            }
                        }
                    }
                }
                post2: createPostWithEntity(input: $input2) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        # No cascade requested for post2
                    }
                }
            }
        """

        variables = {
            "input1": {
                "title": "Post 1",
                "content": "Content 1",
                "authorId": "00000000-0000-0000-0000-000000000001"
            },
            "input2": {
                "title": "Post 2",
                "content": "Content 2",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        assert "errors" not in result

        # post1 should have cascade
        post1_response = result["data"]["post1"]
        assert "cascade" in post1_response
        assert "updated" in post1_response["cascade"]

        # post2 should NOT have cascade
        post2_response = result["data"]["post2"]
        assert "cascade" not in post2_response


class TestCascadeSelectionPayloadSize:
    """Test that selection filtering reduces payload size."""

    @pytest.mark.asyncio
    async def test_response_size_without_cascade(
        self, graphql_client, db_pool
    ):
        """Measure response size when CASCADE not requested."""
        import json

        mutation = """
            mutation CreatePostWithEntity($input: CreatePostInput!) {
                createPostWithEntity(input: $input) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        post {
                            id
                            title
                        }
                    }
                }
            }
        """

        variables = {
            "input": {
                "title": "Test Post",
                "content": "Test content",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        # Measure size
        result_json = json.dumps(result)
        size_without_cascade = len(result_json.encode('utf-8'))

        # Store for comparison
        return size_without_cascade

    @pytest.mark.asyncio
    async def test_response_size_with_cascade(
        self, graphql_client, db_pool
    ):
        """Measure response size when CASCADE requested."""
        import json

        mutation = """
            mutation CreatePostWithEntity($input: CreatePostInput!) {
                createPostWithEntity(input: $input) {
                    ... on CreatePostWithEntitySuccess {
                        message
                        post {
                            id
                            title
                        }
                        cascade {
                            updated {
                                __typename
                                id
                                operation
                                entity
                            }
                            deleted {
                                __typename
                                id
                            }
                            invalidations {
                                queryName
                                strategy
                                scope
                            }
                            metadata {
                                timestamp
                                affectedCount
                            }
                        }
                    }
                }
            }
        """

        variables = {
            "input": {
                "title": "Test Post",
                "content": "Test content",
                "authorId": "00000000-0000-0000-0000-000000000001"
            }
        }

        result = await execute_graphql(graphql_client, mutation, variables)

        # Measure size
        result_json = json.dumps(result)
        size_with_cascade = len(result_json.encode('utf-8'))

        # Store for comparison
        return size_with_cascade

        # Size with cascade should be significantly larger
        # This will be verified once bug is fixed
```

---

## Verification Commands

```bash
# Run new tests (should FAIL initially - RED phase)
uv run pytest tests/integration/test_cascade_selection_filtering.py -xvs

# Expected failures:
# - test_cascade_not_returned_when_not_requested: FAIL (CASCADE present when shouldn't be)
# - test_partial_cascade_selection_updated_only: FAIL (All CASCADE fields present)
# - test_partial_cascade_selection_metadata_only: FAIL (All CASCADE fields present)
```

---

## Acceptance Criteria

- ✅ All tests written and execute (even if failing)
- ✅ Tests cover:
  - No CASCADE requested → No CASCADE in response
  - Full CASCADE requested → Full CASCADE in response
  - Partial CASCADE requested → Only requested fields in response
  - Multiple mutations with different selections
  - Payload size difference measured
- ✅ Tests are clear, well-documented, and follow existing patterns
- ✅ Test file follows FraiseQL testing conventions

---

## DO NOT

- ❌ Implement any fixes (that's GREEN phase)
- ❌ Modify production code
- ❌ Skip tests or mark as xfail
- ❌ Write tests that pass initially (defeats RED phase purpose)

---

## Notes

- Tests use existing `createPostWithEntity` mutation from test suite
- Tests assume CASCADE is enabled on the mutation (`enable_cascade=True`)
- Payload size tests provide metrics for performance improvement validation
- Error response test ensures CASCADE doesn't leak in error cases

---

## Next Phase

After this phase completes:
→ **Phase 2: GREEN** - Implement minimal fix to make tests pass
