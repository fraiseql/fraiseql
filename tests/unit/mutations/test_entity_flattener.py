"""Unit tests for entity flattening logic."""

import pytest
from fraiseql.mutations.entity_flattener import (
    should_flatten_entity,
    get_success_type_fields,
    flatten_entity_wrapper,
)


class MinimalSuccess:
    """Success type with only message field."""

    message: str


class CreatePostSuccess:
    """Success type with explicit fields."""

    post: dict  # Simplified for testing
    message: str
    cascade: dict


class NoAnnotations:
    """Success type with no annotations."""

    pass


def test_should_flatten_entity_no_annotations():
    """Type with no annotations should not flatten."""
    assert not should_flatten_entity(NoAnnotations)


def test_should_flatten_entity_minimal():
    """Type with only message should not flatten."""
    assert not should_flatten_entity(MinimalSuccess)


def test_should_flatten_entity_explicit_fields():
    """Type with explicit fields should flatten."""
    assert should_flatten_entity(CreatePostSuccess)


def test_get_success_type_fields():
    """Should return correct field names."""
    fields = get_success_type_fields(CreatePostSuccess)
    assert fields == {"post", "message", "cascade"}


def test_flatten_entity_wrapper_basic():
    """Should flatten entity fields to top level."""
    mutation_result = {
        "status": "created",
        "message": "Post created",
        "entity": {
            "post": {"id": "123", "title": "Test"},
            "extra": "data",
        },
        "cascade": {"updated": [], "deleted": []},
        "entity_type": "Article",  # Doesn't match 'post' field, so should flatten
        "entity_id": "123",
    }

    flattened = flatten_entity_wrapper(mutation_result, CreatePostSuccess)

    # Entity wrapper should be removed
    assert "entity" not in flattened

    # Fields should be at top level
    assert flattened["post"] == {"id": "123", "title": "Test"}
    assert flattened["message"] == "Post created"

    # Cascade should come from top-level, not entity
    assert flattened["cascade"] == {"updated": [], "deleted": []}

    # Other fields preserved
    assert flattened["entity_type"] == "Article"
    assert flattened["entity_id"] == "123"


def test_flatten_entity_wrapper_minimal_success():
    """Should keep entity wrapper for minimal success type."""
    mutation_result = {
        "status": "success",
        "message": "Done",
        "entity": {"data": "value"},
    }

    flattened = flatten_entity_wrapper(mutation_result, MinimalSuccess)

    # Entity wrapper should be kept
    assert "entity" in flattened
    assert flattened["entity"] == {"data": "value"}


def test_flatten_entity_wrapper_no_entity_field():
    """Should return unchanged if no entity field (v1 format)."""
    mutation_result = {
        "status": "success",
        "message": "Done",
        "object_data": {"id": "123"},
    }

    flattened = flatten_entity_wrapper(mutation_result, CreatePostSuccess)

    # Should be unchanged
    assert flattened == mutation_result


def test_flatten_entity_wrapper_cascade_priority():
    """Top-level cascade should take priority over entity.cascade."""
    mutation_result = {
        "status": "created",
        "message": "Done",
        "entity": {
            "post": {"id": "123"},
            "cascade": {"wrong": "value"},  # Should be ignored
        },
        "cascade": {"correct": "value"},  # Should be used
    }

    flattened = flatten_entity_wrapper(mutation_result, CreatePostSuccess)

    # Top-level cascade should be used
    assert flattened["cascade"] == {"correct": "value"}
    assert "wrong" not in str(flattened["cascade"])
