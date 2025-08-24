import pytest

from fraiseql.utils.casing import to_camel_case


@pytest.mark.unit
class TestCasing:
    """Test suite for casing utility functions."""

    def test_to_camel_case_basic(self) -> None:
        """Test basic snake_case to camelCase conversion."""
        assert to_camel_case("user_name") == "userName"
        assert to_camel_case("first_name") == "firstName"
        assert to_camel_case("last_updated_at") == "lastUpdatedAt"

    def test_to_camel_case_single_word(self) -> None:
        """Test single word remains unchanged."""
        assert to_camel_case("username") == "username"
        assert to_camel_case("id") == "id"
        assert to_camel_case("name") == "name"

    def test_to_camel_case_already_camel(self) -> None:
        """Test already camelCase strings remain unchanged."""
        assert to_camel_case("userName") == "userName"
        assert to_camel_case("firstName") == "firstName"

    def test_to_camel_case_empty_string(self) -> None:
        """Test empty string returns empty string."""
        assert to_camel_case("") == ""

    def test_to_camel_case_with_numbers(self) -> None:
        """Test conversion with numbers."""
        assert to_camel_case("user_id_123") == "userId123"
        assert to_camel_case("page_2_content") == "page2Content"

    def test_to_camel_case_consecutive_underscores(self) -> None:
        """Test handling of consecutive underscores."""
        assert to_camel_case("user__name") == "userName"
        assert to_camel_case("first___name") == "firstName"

    def test_to_camel_case_leading_underscore(self) -> None:
        """Test handling of leading underscore."""
        # Common convention: leading underscore indicates private
        assert to_camel_case("_private_field") == "PrivateField"
        assert to_camel_case("__double_private") == "DoublePrivate"

    def test_to_camel_case_trailing_underscore(self) -> None:
        """Test handling of trailing underscore."""
        assert to_camel_case("reserved_word_") == "reservedWord"
        assert to_camel_case("field_name__") == "fieldName"

    def test_to_camel_case_all_caps(self) -> None:
        """Test conversion of all caps snake_case."""
        assert to_camel_case("USER_ID") == "USERId"
        assert to_camel_case("API_KEY") == "APIKey"
        assert to_camel_case("HTTP_STATUS_CODE") == "HTTPStatusCode"

    def test_to_camel_case_mixed_case(self) -> None:
        """Test conversion of mixed case input."""
        assert to_camel_case("User_Name") == "UserName"
        assert to_camel_case("FIRST_name") == "FIRSTName"
