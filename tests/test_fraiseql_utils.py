"""Tests for FraiseQL utilities module - denormalized column naming."""

import pytest
from fraiseql.fraiseql_utils import (
    snake_case,
    generate_denormalized_column_name,
    parse_denormalized_column_name,
)


class TestSnakeCase:
    """Tests for camelCase to snake_case conversion."""

    def test_simple_camel_case(self):
        """Test basic camelCase conversion."""
        assert snake_case("ltreePath") == "ltree_path"

    def test_already_snake_case(self):
        """Test already snake_case stays unchanged."""
        assert snake_case("ltree_path") == "ltree_path"

    def test_single_word(self):
        """Test single word unchanged."""
        assert snake_case("field") == "field"

    def test_uppercase_acronym(self):
        """Test handling of uppercase sequences."""
        assert snake_case("HTTPServer") == "http_server"

    def test_consecutive_capitals(self):
        """Test consecutive capital letters."""
        assert snake_case("XMLParser") == "xml_parser"

    def test_numbers(self):
        """Test numbers in field names."""
        assert snake_case("field2Name") == "field2_name"

    def test_leading_capital(self):
        """Test leading capital letter."""
        assert snake_case("FieldName") == "field_name"

    def test_empty_string(self):
        """Test empty string."""
        assert snake_case("") == ""

    def test_all_uppercase(self):
        """Test all uppercase."""
        assert snake_case("CONSTANT") == "constant"


class TestGenerateDenormalizedColumnName:
    """Tests for denormalized column name generation."""

    def test_simple_nested_path(self):
        """Test simple two-level nested path."""
        result = generate_denormalized_column_name("location.ltreePath")
        assert result == "location__ltree_path"

    def test_three_level_nested_path(self):
        """Test three-level nested path."""
        result = generate_denormalized_column_name("company.department.name")
        assert result == "company__department__name"

    def test_four_level_nested_path(self):
        """Test four-level nested path."""
        result = generate_denormalized_column_name("company.dept.division.section")
        assert result == "company__dept__division__section"

    def test_single_level_path(self):
        """Test single-level path (just field name)."""
        result = generate_denormalized_column_name("postalCode")
        assert result == "postal_code"

    def test_address_postal_code_example(self):
        """Test address.postalCode example from plan."""
        result = generate_denormalized_column_name("address.postalCode")
        assert result == "address__postal_code"

    def test_column_name_length_under_limit(self):
        """Test column name under 63 byte limit."""
        result = generate_denormalized_column_name("location.ltreePath")
        assert len(result.encode()) <= 63
        assert "__" in result
        assert "_" in result

    def test_long_path_exceeds_limit(self):
        """Test that long paths are handled with hash suffix."""
        # Create a path that exceeds 63 bytes when converted
        long_path = "very.deeply.nested.structure.with.many.levels.and_a_very_long_field_name"
        result = generate_denormalized_column_name(long_path)
        assert len(result.encode()) <= 63
        # Should have hash suffix (6 hex chars)
        assert "_" in result
        parts = result.split("_")
        # Last part should be 6 hex characters (hash suffix)
        assert len(parts[-1]) == 6
        assert all(c in "0123456789abcdef" for c in parts[-1])

    def test_hash_suffix_determinism(self):
        """Test that hash suffix is deterministic for same input."""
        long_path = "very.deeply.nested.structure.with.many.levels.field"
        result1 = generate_denormalized_column_name(long_path)
        result2 = generate_denormalized_column_name(long_path)
        assert result1 == result2

    def test_hash_suffix_different_for_different_inputs(self):
        """Test that different inputs get different hash suffixes."""
        path1 = "very.deeply.nested.structure.with.many.levels.field1"
        path2 = "very.deeply.nested.structure.with.many.levels.field2"
        result1 = generate_denormalized_column_name(path1)
        result2 = generate_denormalized_column_name(path2)
        assert result1 != result2

    def test_unicode_field_names(self):
        """Test handling of unicode characters in field names."""
        result = generate_denormalized_column_name("location.données")
        assert isinstance(result, str)
        assert len(result.encode()) <= 63

    def test_numbers_in_field_names(self):
        """Test numbers in field names."""
        result = generate_denormalized_column_name("level1.level2.field3")
        assert result == "level1__level2__field3"

    def test_mixed_naming_styles(self):
        """Test mixed naming styles (camelCase, snake_case, etc)."""
        result = generate_denormalized_column_name("my_Entity.nestedField.another_Name")
        assert result == "my_entity__nested_field__another_name"

    def test_empty_path(self):
        """Test empty path handling."""
        result = generate_denormalized_column_name("")
        assert isinstance(result, str)

    def test_single_dot(self):
        """Test path with just dots."""
        result = generate_denormalized_column_name(".")
        assert isinstance(result, str)
        assert len(result.encode()) <= 63

    def test_allocation_location_ltree_path(self):
        """Test allocation.location.ltreePath example from plan."""
        result = generate_denormalized_column_name("allocation.location.ltreePath")
        assert result == "allocation__location__ltree_path"

    def test_user_address_postal_code(self):
        """Test user.address.postalCode example from plan."""
        result = generate_denormalized_column_name("user.address.postalCode")
        assert result == "user__address__postal_code"

    def test_result_is_valid_postgres_column_name(self):
        """Test that result is a valid PostgreSQL column name."""
        result = generate_denormalized_column_name("location.ltreePath")
        # Valid PostgreSQL column names are alphanumeric + underscore
        assert all(c.isalnum() or c == "_" for c in result)
        # Cannot start with digit
        assert result[0].isalpha() or result[0] == "_"


class TestParseDenormalizedColumnName:
    """Tests for reverse parsing of denormalized column names."""

    def test_parse_simple_column(self):
        """Test parsing simple denormalized column."""
        # This function should return None for simple columns
        # (ones without hash suffix)
        result = parse_denormalized_column_name("location__ltree_path")
        assert result is None  # Not a hash-suffixed column

    def test_parse_hash_suffixed_column(self):
        """Test parsing hash-suffixed column."""
        # Create a long path that will have hash suffix
        long_path = "very.deeply.nested.structure.with.many.levels.and_a_field"
        column = generate_denormalized_column_name(long_path)
        # Should be able to recognize it has hash suffix
        result = parse_denormalized_column_name(column)
        # Result could be None (no mapping) or the original path if we store it
        # For now, just verify it doesn't crash
        assert result is None or isinstance(result, str)

    def test_parse_regular_column_returns_none(self):
        """Test that regular columns return None."""
        result = parse_denormalized_column_name("id")
        assert result is None

    def test_parse_regular_column_with_underscore_returns_none(self):
        """Test regular column with underscore."""
        result = parse_denormalized_column_name("user_id")
        assert result is None


class TestEdgeCases:
    """Test edge cases and error handling."""

    def test_none_input_to_generate(self):
        """Test None input (should raise or handle gracefully)."""
        with pytest.raises((TypeError, AttributeError)):
            generate_denormalized_column_name(None)

    def test_special_characters_in_path(self):
        """Test special characters in path."""
        # Should handle or raise appropriately
        try:
            result = generate_denormalized_column_name("location$$.ltreePath")
            # If it succeeds, should still be valid
            assert len(result.encode()) <= 63
        except (ValueError, AttributeError):
            # Acceptable to raise for invalid input
            pass

    def test_whitespace_in_path(self):
        """Test whitespace in path."""
        # Whitespace should be handled
        try:
            result = generate_denormalized_column_name("location .ltreePath")
            assert isinstance(result, str)
        except ValueError:
            # Acceptable to raise for invalid input
            pass

    def test_multiple_consecutive_dots(self):
        """Test multiple consecutive dots in path."""
        result = generate_denormalized_column_name("location..ltreePath")
        assert isinstance(result, str)
        assert len(result.encode()) <= 63

    def test_leading_trailing_dots(self):
        """Test leading/trailing dots in path."""
        result = generate_denormalized_column_name(".location.ltreePath.")
        assert isinstance(result, str)
        assert len(result.encode()) <= 63


class TestIntegration:
    """Integration tests for the utility functions."""

    def test_multiple_paths_consistent_naming(self):
        """Test that similar paths get consistent naming."""
        paths = [
            "location.ltreePath",
            "address.zipCode",
            "company.department.name",
        ]
        results = [generate_denormalized_column_name(p) for p in paths]
        # All should be valid column names
        for result in results:
            assert len(result.encode()) <= 63
            assert all(c.isalnum() or c == "_" for c in result)

    def test_column_naming_schema_follows_convention(self):
        """Test that naming follows double-underscore convention."""
        result = generate_denormalized_column_name("location.ltreePath")
        # Should follow pattern: entity__sub_entity__field_name
        assert "__" in result

    def test_snake_case_applied_to_all_parts(self):
        """Test that snake_case is applied to all path parts."""
        result = generate_denormalized_column_name("myEntity.nestedField.anotherField")
        # All parts should be snake_case
        parts = result.split("__")
        for part in parts:
            # Should be lowercase with underscores
            assert part == part.lower()
