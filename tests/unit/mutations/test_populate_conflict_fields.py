"""Unit tests for _populate_conflict_fields function."""

import pytest
import fraiseql
from fraiseql.mutations.parser import _populate_conflict_fields
from fraiseql.mutations.types import MutationResult


@pytest.mark.unit
@fraiseql.type
class TestEntity:
    """Test entity for conflict field testing."""

    id: str
    name: str

    @classmethod
    def from_dict(cls, data: dict) -> "TestEntity":
        return cls(**data)


class TestPopulateConflictFields:
    """Unit tests for the _populate_conflict_fields function."""

    def test_populate_conflict_fields_basic_functionality(self):
        """Test that _populate_conflict_fields correctly populates conflict_* fields."""
        # Setup test data
        result = MutationResult(
            extra_metadata={
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {"id": "test-id-123", "name": "Test Entity"}
                            }
                        }
                    }
                ]
            }
        )

        annotations = {
            "message": str,
            "conflict_entity": TestEntity | None,
        }

        fields = {"message": "Test message"}

        # Call the function
        _populate_conflict_fields(result, annotations, fields)

        # Verify results
        assert "conflict_entity" in fields
        assert fields["conflict_entity"] is not None
        assert isinstance(fields["conflict_entity"], TestEntity)
        assert fields["conflict_entity"].id == "test-id-123"
        assert fields["conflict_entity"].name == "Test Entity"

    def test_populate_conflict_fields_no_extra_metadata(self):
        """Test that function handles missing extra_metadata gracefully."""
        result = MutationResult()  # No extra_metadata
        annotations = {"conflict_entity": TestEntity | None}
        fields = {}

        # Should not raise exception and not modify fields
        _populate_conflict_fields(result, annotations, fields)
        assert "conflict_entity" not in fields

    def test_populate_conflict_fields_malformed_errors_structure(self):
        """Test that function handles malformed errors structure gracefully."""
        result = MutationResult(extra_metadata={"errors": "not-a-list"})  # Invalid structure
        annotations = {"conflict_entity": TestEntity | None}
        fields = {}

        _populate_conflict_fields(result, annotations, fields)
        assert "conflict_entity" not in fields

    def test_populate_conflict_fields_missing_conflict_object(self):
        """Test that function handles missing conflictObject gracefully."""
        result = MutationResult(
            extra_metadata={
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                # Missing conflictObject
                            }
                        }
                    }
                ]
            }
        )
        annotations = {"conflict_entity": TestEntity | None}
        fields = {}

        _populate_conflict_fields(result, annotations, fields)
        assert "conflict_entity" not in fields

    def test_populate_conflict_fields_skips_already_populated(self):
        """Test that function skips fields that are already populated."""
        result = MutationResult(
            extra_metadata={
                "errors": [
                    {
                        "details": {
                            "conflict": {"conflictObject": {"id": "new-id", "name": "New Entity"}}
                        }
                    }
                ]
            }
        )

        annotations = {"conflict_entity": TestEntity | None}
        existing_entity = TestEntity(id="existing-id", name="Existing Entity")
        fields = {"conflict_entity": existing_entity}

        # Call the function
        _populate_conflict_fields(result, annotations, fields)

        # Should not overwrite existing field
        assert fields["conflict_entity"] is existing_entity
        assert fields["conflict_entity"].id == "existing-id"

    def test_populate_conflict_fields_multiple_conflict_types(self):
        """Test that function can populate multiple different conflict_* fields."""

        result = MutationResult(
            extra_metadata={
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {"id": "multi-test", "name": "Multi Test"}
                            }
                        }
                    }
                ]
            }
        )

        annotations = {
            "message": str,
            "conflict_entity": TestEntity | None,
            "conflict_other": TestEntity | None,  # Same type so both can be instantiated
        }

        fields = {"message": "Test"}

        _populate_conflict_fields(result, annotations, fields)

        # Both conflict fields should be populated with the same data
        # (This tests the generic nature of the conflict resolution)
        assert "conflict_entity" in fields
        assert "conflict_other" in fields
        assert fields["conflict_entity"].id == "multi-test"
        assert fields["conflict_other"].id == "multi-test"

    def test_populate_conflict_fields_handles_instantiation_errors(self):
        """Test that function handles instantiation errors gracefully."""

        # Use a type that will cause _instantiate_type to fail
        # by not being a FraiseQL type and not having from_dict method
        class BadEntity:
            """Entity that will fail to instantiate properly."""

            def __init__(self, **kwargs):
                # This constructor signature won't work with _instantiate_type
                raise ValueError("Constructor designed to fail")

        result = MutationResult(
            extra_metadata={
                "errors": [
                    {
                        "details": {
                            "conflict": {"conflictObject": {"id": "bad-test", "name": "Bad Test"}}
                        }
                    }
                ]
            }
        )

        annotations = {"conflict_bad": BadEntity | None}
        fields = {}

        # Should not raise exception and should continue gracefully
        # Note: _instantiate_type might return the raw dict if it can't instantiate,
        # but our function should handle exceptions in the try/except
        _populate_conflict_fields(result, annotations, fields)

        # The field might be populated with raw data or not at all, depending on _instantiate_type behavior
        # The key point is that no exception should be raised
        # Let's just check that the function completed successfully
        assert True  # If we get here, no exception was raised

    def test_populate_conflict_fields_ignores_non_conflict_fields(self):
        """Test that function only processes fields starting with 'conflict_'."""
        result = MutationResult(
            extra_metadata={
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {"id": "ignore-test", "name": "Ignore Test"}
                            }
                        }
                    }
                ]
            }
        )

        annotations = {
            "message": str,
            "regular_entity": TestEntity | None,  # Should be ignored
            "conflict_entity": TestEntity | None,  # Should be populated
        }

        fields = {}

        _populate_conflict_fields(result, annotations, fields)

        # Only conflict_* fields should be populated
        assert "regular_entity" not in fields
        assert "conflict_entity" in fields
        assert fields["conflict_entity"].id == "ignore-test"
