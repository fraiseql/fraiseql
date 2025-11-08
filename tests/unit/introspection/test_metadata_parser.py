"""Unit tests for metadata parser."""

import pytest
from fraiseql.introspection.metadata_parser import (
    MetadataParser,
    TypeAnnotation,
    MutationAnnotation,
)


class TestMetadataParser:
    """Test metadata parsing functionality."""

    def test_parse_type_annotation_basic(self):
        """Test basic type annotation parsing."""
        parser = MetadataParser()
        comment = "@fraiseql:type\ntrinity: true\ndescription: User account"

        result = parser.parse_type_annotation(comment)

        assert result is not None
        assert result.trinity is True
        assert result.description == "User account"
        assert result.expose_fields is None

    def test_parse_type_annotation_with_fields(self):
        """Test type annotation with expose_fields."""
        parser = MetadataParser()
        comment = """@fraiseql:type
expose_fields:
  - id
  - name
  - email"""

        result = parser.parse_type_annotation(comment)

        assert result is not None
        assert result.expose_fields == ["id", "name", "email"]

    def test_parse_type_annotation_invalid_yaml(self):
        """Test error handling for invalid YAML."""
        parser = MetadataParser()
        comment = "@fraiseql:type\ninvalid: yaml: [unclosed"

        result = parser.parse_type_annotation(comment)
        assert result is None

    def test_parse_type_annotation_no_marker(self):
        """Test handling of comments without markers."""
        parser = MetadataParser()
        comment = "Just a regular comment"

        result = parser.parse_type_annotation(comment)
        assert result is None

    def test_parse_mutation_annotation_basic(self):
        """Test basic mutation annotation parsing."""
        parser = MetadataParser()
        comment = """@fraiseql:mutation
input_schema:
  name: {type: string, required: true}
  email: {type: string, required: true}
success_type: User
failure_type: ValidationError
description: Create a new user"""

        result = parser.parse_mutation_annotation(comment)

        assert result is not None
        assert result.success_type == "User"
        assert result.failure_type == "ValidationError"
        assert result.description == "Create a new user"
        assert "name" in result.input_schema

    def test_parse_mutation_annotation_missing_required(self):
        """Test mutation annotation with missing required fields."""
        parser = MetadataParser()
        comment = "@fraiseql:mutation\ndescription: Missing required fields"

        result = parser.parse_mutation_annotation(comment)
        assert result is None
