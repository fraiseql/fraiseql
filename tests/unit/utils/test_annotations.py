"""Tests for fraiseql.utils.annotations module.

These tests ensure that annotation utility functions correctly handle
various type annotation patterns used in GraphQL schema generation.
"""

from typing import Union

from fraiseql.utils.annotations import (
    get_non_optional_type,
    is_optional_type,
    unwrap_annotated,
)


class TestUnwrapAnnotated:
    """Tests for unwrap_annotated function."""

    def test_unwrap_simple_annotated(self) -> None:
        """Test unwrapping Annotated with metadata."""
        from typing import Annotated

        typ = Annotated[str, "metadata"]
        base, annotations = unwrap_annotated(typ)
        assert base is str
        assert annotations == ["metadata"]

    def test_unwrap_non_annotated(self) -> None:
        """Test unwrapping non-Annotated types returns as-is."""
        base, annotations = unwrap_annotated(str)
        assert base is str
        assert annotations == []


class TestIsOptionalType:
    """Tests for is_optional_type function."""

    def test_optional_with_union(self) -> None:
        """Test detection of Union[T, None] as optional."""
        assert is_optional_type(Union[str, None])

    def test_optional_with_pipe_syntax(self) -> None:
        """Test detection of T | None as optional."""
        assert is_optional_type(str | None)

    def test_non_optional_type(self) -> None:
        """Test that non-optional types are correctly identified."""
        assert not is_optional_type(str)
        assert not is_optional_type(int)

    def test_union_without_none(self) -> None:
        """Test that unions without None are not considered optional."""
        assert not is_optional_type(Union[str, int])


class TestGetNonOptionalType:
    """Tests for get_non_optional_type function."""

    def test_extract_from_simple_optional(self) -> None:
        """Test extracting type from Union[T, None]."""
        assert get_non_optional_type(Union[str, None]) is str

    def test_extract_from_pipe_optional(self) -> None:
        """Test extracting type from T | None."""
        assert get_non_optional_type(str | None) is str

    def test_non_optional_returns_as_is(self) -> None:
        """Test that non-optional types are returned unchanged."""
        assert get_non_optional_type(str) is str

    def test_multiple_types_with_none(self) -> None:
        """Test handling of Union[T1, T2, None].

        This is a regression test for the bug where dict | list | None
        would be incorrectly reconstructed using the | syntax, causing
        GraphQL schema building errors.

        The function should return Union[dict, list] instead of dict | list,
        because the Union type annotation is properly handled by GraphQL
        schema introspection, while the | syntax (UnionType) may not be.
        """
        result = get_non_optional_type(Union[dict, list, None])

        # The result should be a Union type, not a UnionType from | syntax
        from typing import get_args, get_origin

        assert get_origin(result) is Union
        args = get_args(result)
        assert set(args) == {dict, list}

    def test_dict_list_pipe_union_with_none(self) -> None:
        """Test dict | list | None is properly handled.

        This specific pattern was causing GraphQL schema build failures
        in printoptim_backend when fraiseql upgraded to modern type hints.
        """
        result = get_non_optional_type(dict | list | None)

        # Verify it returns a proper Union, not UnionType
        from typing import get_args, get_origin

        assert get_origin(result) is Union
        args = get_args(result)
        assert set(args) == {dict, list}

    def test_union_with_only_none(self) -> None:
        """Test that Union[None] is not considered optional.

        Since None is the only type, is_optional_type returns False,
        and get_non_optional_type returns the type unchanged.
        """
        # Union[None] is not considered "optional" by is_optional_type
        # because there's no actual non-None type
        assert not is_optional_type(Union[None])
        # So get_non_optional_type returns it as-is
        assert get_non_optional_type(Union[None]) is type(None)
