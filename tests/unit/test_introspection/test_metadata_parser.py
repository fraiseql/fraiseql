"""Unit tests for metadata parser."""

from fraiseql.introspection.metadata_parser import (
    MetadataParser,
)


class TestMetadataParser:
    """Test metadata parsing functionality."""

    def test_parse_type_annotation_basic(self) -> None:
        """Test basic type annotation parsing."""
        parser = MetadataParser()
        comment = "@fraiseql:type\ntrinity: true\ndescription: User account"

        result = parser.parse_type_annotation(comment)

        assert result is not None
        assert result.trinity is True
        assert result.description == "User account"
        assert result.expose_fields is None

    def test_parse_type_annotation_with_fields(self) -> None:
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

    def test_parse_type_annotation_invalid_yaml(self) -> None:
        """Test error handling for invalid YAML."""
        parser = MetadataParser()
        comment = "@fraiseql:type\ninvalid: yaml: [unclosed"

        result = parser.parse_type_annotation(comment)
        assert result is None

    def test_parse_type_annotation_no_marker(self) -> None:
        """Test handling of comments without markers."""
        parser = MetadataParser()
        comment = "Just a regular comment"

        result = parser.parse_type_annotation(comment)
        assert result is None

    def test_parse_mutation_annotation_basic(self) -> None:
        """Test basic mutation annotation parsing."""
        parser = MetadataParser()
        comment = """@fraiseql:mutation
name: createUser
success_type: User
error_type: ValidationError
description: Create a new user"""

        result = parser.parse_mutation_annotation(comment)

        assert result is not None
        assert result.name == "createUser"
        assert result.success_type == "User"
        assert result.error_type == "ValidationError"
        assert result.description == "Create a new user"

    def test_parse_mutation_annotation_missing_required(self) -> None:
        """Test mutation annotation with missing required fields."""
        parser = MetadataParser()
        comment = "@fraiseql:mutation\ndescription: Missing required fields"

        result = parser.parse_mutation_annotation(comment)
        assert result is None

    def test_parse_field_annotation_basic(self) -> None:
        """Test parsing basic field annotation (created by SpecQL)."""
        # Given: Parser
        parser = MetadataParser()

        # Given: Field comment (as SpecQL creates it)
        comment = "@fraiseql:field name=email,type=String!,required=true"

        # When: Parse annotation
        metadata = parser.parse_field_annotation(comment)

        # Then: Metadata is parsed correctly
        assert metadata is not None
        assert metadata.name == "email"
        assert metadata.graphql_type == "String!"
        assert metadata.required is True
        assert metadata.is_enum is False

    def test_parse_field_annotation_with_enum(self) -> None:
        """Test parsing field annotation with enum flag."""
        # Given: Parser
        parser = MetadataParser()

        # Given: Field comment with enum (SpecQL creates this for enum fields)
        comment = "@fraiseql:field name=status,type=ContactStatus,required=true,enum=true"

        # When: Parse
        metadata = parser.parse_field_annotation(comment)

        # Then: Enum flag is set
        assert metadata is not None
        assert metadata.name == "status"
        assert metadata.is_enum is True

    def test_parse_field_annotation_optional(self) -> None:
        """Test parsing optional field (required=false)."""
        # Given: Parser
        parser = MetadataParser()

        # Given: Optional field (SpecQL marks nullable fields this way)
        comment = "@fraiseql:field name=companyId,type=UUID,required=false"

        # When: Parse
        metadata = parser.parse_field_annotation(comment)

        # Then: Required is False
        assert metadata is not None
        assert metadata.required is False

    def test_parse_field_annotation_no_annotation(self) -> None:
        """Test parsing comment without @fraiseql:field."""
        # Given: Parser
        parser = MetadataParser()

        # Given: Regular comment (not from SpecQL)
        comment = "This is just a regular comment"

        # When: Parse
        metadata = parser.parse_field_annotation(comment)

        # Then: Returns None
        assert metadata is None

    def test_parse_mutation_annotation_with_context_params(self) -> None:
        """Test parsing mutation annotation with context_params."""
        # Given: Mutation comment with context_params
        comment = """
        @fraiseql:mutation
        name: qualifyLead
        success_type: Contact
        error_type: ContactError
        context_params: [auth_tenant_id, auth_user_id]
        """

        # When: Parse annotation
        parser = MetadataParser()
        annotation = parser.parse_mutation_annotation(comment)

        # Then: context_params is parsed
        assert annotation is not None
        assert annotation.name == "qualifyLead"
        assert annotation.context_params == ["auth_tenant_id", "auth_user_id"]

    def test_parse_mutation_annotation_without_context_params(self) -> None:
        """Test parsing mutation annotation without context_params (backward compat)."""
        # Given: Mutation comment without context_params
        comment = """
        @fraiseql:mutation
        name: getStatus
        success_type: Status
        error_type: StatusError
        """

        # When: Parse annotation
        parser = MetadataParser()
        annotation = parser.parse_mutation_annotation(comment)

        # Then: context_params is None (will use auto-detection)
        assert annotation is not None
        assert annotation.name == "getStatus"
        assert annotation.context_params is None
