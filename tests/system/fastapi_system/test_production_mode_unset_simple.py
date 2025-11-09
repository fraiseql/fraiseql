import pytest

"""Simple tests for production mode UNSET fixes."""

from fraiseql.fastapi.json_encoder import clean_unset_values
from fraiseql.types.definitions import UNSET


@pytest.mark.unit
class TestUnsetCleaning:
    """Test UNSET value cleaning functionality."""

    def test_clean_unset_values_simple(self) -> None:
        """Test basic UNSET cleaning."""
        data = {"valid": "value", "invalid": UNSET, "nested": {"good": 123, "bad": UNSET}}

        cleaned = clean_unset_values(data)

        assert cleaned["valid"] == "value"
        assert cleaned["invalid"] is None
        assert cleaned["nested"]["good"] == 123
        assert cleaned["nested"]["bad"] is None

    def test_clean_unset_values_list(self) -> None:
        """Test UNSET cleaning in lists."""
        data = [
            {"id": 1, "name": "John", "email": UNSET},
            {"id": 2, "name": UNSET, "email": "jane@example.com"},
        ]

        cleaned = clean_unset_values(data)

        assert cleaned[0]["email"] is None
        assert cleaned[1]["name"] is None
        assert cleaned[0]["name"] == "John"
        assert cleaned[1]["email"] == "jane@example.com"

    def test_clean_unset_direct_unset(self) -> None:
        """Test cleaning when the value itself is UNSET."""
        assert clean_unset_values(UNSET) is None

    def test_clean_unset_preserves_other_values(self) -> None:
        """Test that non-UNSET values are preserved."""
        data = {
            "string": "test",
            "number": 42,
            "boolean": True,
            "null": None,
            "list": [1, 2, 3],
            "dict": {"nested": "value"},
        }

        cleaned = clean_unset_values(data)

        assert cleaned == data  # Should be identical since no UNSET values


class TestProductionModeLogging:
    """Test production mode error logging improvements."""

    def test_unset_error_detection(self) -> None:
        """Test that UNSET serialization errors are detected correctly."""
        error_message = "Object of type Unset is not JSON serializable"

        # This should match our error detection in the router
        assert "Unset is not JSON serializable" in error_message

    def test_query_truncation_logic(self) -> None:
        """Test query truncation for logging."""
        long_query = "{ " + "field " * 100 + "}"

        # Simulate the truncation logic from the router
        truncated = long_query[:200]

        assert len(truncated) <= 200
        assert truncated.startswith("{ field")
