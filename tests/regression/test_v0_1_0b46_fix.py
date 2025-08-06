"""Test for v0.1.0b46 fix - find_one returns dict instead of RawJSONResult."""

from fraiseql.db import FraiseQLRepository


def test_raw_json_methods_still_available():
    """Test that raw JSON methods are still available for special use cases."""
    context = {"mode": "production"}
    db = FraiseQLRepository(None, context)

    # Check methods exist
    assert hasattr(db, "find_raw_json")
    assert hasattr(db, "find_one_raw_json")

    # Check they're documented as special-case methods
    assert "special" in db.find_raw_json.__doc__
    assert "special" in db.find_one_raw_json.__doc__


def test_find_methods_return_type_annotations():
    """Test that find methods have correct return type annotations."""
    from typing import get_type_hints

    # Get type hints for methods
    find_hints = get_type_hints(FraiseQLRepository.find)
    find_one_hints = get_type_hints(FraiseQLRepository.find_one)

    # Check return types - should be dict/list, not RawJSONResult
    assert "return" in find_hints
    assert "return" in find_one_hints

    # The return type should be list[dict[str, Any]]
    return_type_str = str(find_hints["return"])
    assert "list" in return_type_str
    assert "dict" in return_type_str

    # The return type should be Optional[dict[str, Any]]
    return_type_str = str(find_one_hints["return"])
    assert "dict" in return_type_str or "Dict" in return_type_str
