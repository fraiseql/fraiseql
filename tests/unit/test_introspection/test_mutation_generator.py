"""Unit tests for MutationGenerator."""

import pytest

from fraiseql.introspection.input_generator import InputGenerator
from fraiseql.introspection.metadata_parser import MutationAnnotation
from fraiseql.introspection.mutation_generator import MutationGenerator
from fraiseql.introspection.postgres_introspector import FunctionMetadata, ParameterInfo
from fraiseql.introspection.type_mapper import TypeMapper


@pytest.fixture
def mock_introspector() -> None:
    """Mock PostgresIntrospector for testing."""

    class MockIntrospector:
        async def discover_composite_type(self, *args, **kwargs) -> None:
            return None

    return MockIntrospector()


class TestMutationGenerator:
    """Test MutationGenerator functionality."""

    @pytest.fixture
    def type_mapper(self) -> TypeMapper:
        """Create a TypeMapper instance."""
        return TypeMapper()

    @pytest.fixture
    def input_generator(self, type_mapper: TypeMapper) -> InputGenerator:
        """Create an InputGenerator instance."""
        return InputGenerator(type_mapper)

    @pytest.fixture
    def mutation_generator(self, input_generator: InputGenerator) -> MutationGenerator:
        """Create a MutationGenerator instance."""
        return MutationGenerator(input_generator)

    @pytest.mark.asyncio
    async def test_generate_mutation_for_function_success(
        self, mutation_generator: MutationGenerator, mock_introspector
    ):
        """Test successful mutation generation."""
        # Given: Function metadata and annotation
        function_metadata = FunctionMetadata(
            schema_name="public",
            function_name="fn_create_user",
            parameters=[
                ParameterInfo(name="p_name", pg_type="text", mode="IN", default_value=None),
                ParameterInfo(name="p_email", pg_type="text", mode="IN", default_value=None),
            ],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="createUser",
            success_type="User",
            error_type="ValidationError",
            description="Create a user",
        )

        # Type registry with required types
        type_registry = {
            "User": type("User", (), {}),
            "ValidationError": type("ValidationError", (), {}),
        }

        # When: Generate mutation
        mutation = await mutation_generator.generate_mutation_for_function(
            function_metadata, annotation, type_registry, mock_introspector
        )

        # Then: Mutation is generated
        assert mutation is not None
        assert hasattr(mutation, "__name__")
        assert mutation.__name__ == "CreateUser"  # Class name

    @pytest.mark.asyncio
    async def test_generate_mutation_for_function_missing_types(
        self, mutation_generator: MutationGenerator, mock_introspector
    ):
        """Test mutation generation fails when types are missing."""
        # Given: Function with missing types in registry
        function_metadata = FunctionMetadata(
            schema_name="public",
            function_name="fn_create_user",
            parameters=[],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="createUser", success_type="User", error_type="ValidationError"
        )

        # Empty type registry
        type_registry = {}

        # When: Generate mutation
        mutation = await mutation_generator.generate_mutation_for_function(
            function_metadata, annotation, type_registry, mock_introspector
        )

        # Then: Returns None due to missing types
        assert mutation is None

    @pytest.mark.asyncio
    async def test_generate_mutation_for_function_partial_missing_types(
        self, mutation_generator: MutationGenerator, mock_introspector
    ):
        """Test mutation generation fails when success type is missing."""
        # Given: Function with only failure type in registry
        function_metadata = FunctionMetadata(
            schema_name="public",
            function_name="fn_create_user",
            parameters=[],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="createUser", success_type="User", error_type="ValidationError"
        )

        type_registry = {
            "ValidationError": type("ValidationError", (), {}),
        }

        # When: Generate mutation
        mutation = await mutation_generator.generate_mutation_for_function(
            function_metadata, annotation, type_registry, mock_introspector
        )

        # Then: Returns None due to missing success type
        assert mutation is None

    def test_create_mutation_class(self, mutation_generator: MutationGenerator) -> None:
        """Test creating mutation class with proper annotations."""
        # Given: Input class and types
        input_cls = type("CreateUserInput", (), {"__annotations__": {"name": str, "email": str}})

        success_type = type("User", (), {})
        error_type = type("ValidationError", (), {})

        function_metadata = FunctionMetadata(
            schema_name="public",
            function_name="fn_create_user",
            parameters=[],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="createUser",
            success_type="User",
            error_type="ValidationError",
            description="Create a user",
        )

        # When: Create mutation class
        mutation_cls = mutation_generator._create_mutation_class(
            function_metadata, annotation, input_cls, success_type, error_type
        )

        # Then: Class has correct structure
        assert mutation_cls.__name__ == "CreateUser"
        assert hasattr(mutation_cls, "__annotations__")
        annotations = mutation_cls.__annotations__
        assert annotations["input"] == input_cls
        assert annotations["success"] == success_type
        assert annotations["error"] == error_type
        assert mutation_cls.__doc__ == "Create a user"

    def test_function_to_mutation_class_name_conversion(
        self, mutation_generator: MutationGenerator
    ):
        """Test function name to mutation class name conversion."""
        test_cases = [
            ("fn_create_user", "CreateUser"),
            ("fn_update_post", "UpdatePost"),
            ("fn_delete_item", "DeleteItem"),
            ("tv_create_machine", "CreateMachine"),
        ]

        for function_name, expected_class_name in test_cases:
            actual = mutation_generator._function_to_mutation_class_name(function_name)
            assert actual == expected_class_name

    def test_extract_context_params_auth_prefix(self) -> None:
        """Test context parameter extraction with auth_ prefix (new standard)."""
        # Given: MutationGenerator
        type_mapper = TypeMapper()
        input_generator = InputGenerator(type_mapper)
        mutation_generator = MutationGenerator(input_generator)

        # Given: Function with auth_ prefix context params
        function = FunctionMetadata(
            schema_name="app",
            function_name="qualify_lead",
            parameters=[
                ParameterInfo("p_contact_id", "uuid", "IN", None),
                ParameterInfo("auth_tenant_id", "uuid", "IN", None),
                ParameterInfo("auth_user_id", "uuid", "IN", None),
            ],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        # Given: Annotation without explicit context_params (will auto-detect)
        annotation = MutationAnnotation(
            name="qualifyLead",
            description=None,
            success_type="Contact",
            error_type="ContactError",
            context_params=None,
        )

        # When: Extract context params
        context_params = mutation_generator._extract_context_params(function, annotation)

        # Then: Correct mapping
        assert context_params == {"tenant_id": "auth_tenant_id", "user_id": "auth_user_id"}

    def test_extract_context_params_explicit_metadata(self) -> None:
        """Test context parameter extraction with explicit metadata."""
        # Given: MutationGenerator
        type_mapper = TypeMapper()
        input_generator = InputGenerator(type_mapper)
        mutation_generator = MutationGenerator(input_generator)

        # Given: Function with context params
        function = FunctionMetadata(
            schema_name="crm",
            function_name="qualify_lead",
            parameters=[
                ParameterInfo("p_contact_id", "uuid", "IN", None),
                ParameterInfo("auth_tenant_id", "text", "IN", None),
                ParameterInfo("auth_user_id", "uuid", "IN", None),
            ],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        # Given: Annotation WITH explicit context_params (SpecQL provides this)
        annotation = MutationAnnotation(
            name="qualifyLead",
            description=None,
            success_type="Contact",
            error_type="ContactError",
            context_params=["auth_tenant_id", "auth_user_id"],  # Explicit!
        )

        # When: Extract context params
        context_params = mutation_generator._extract_context_params(function, annotation)

        # Then: Uses explicit metadata (priority 1)
        assert context_params == {"tenant_id": "auth_tenant_id", "user_id": "auth_user_id"}

    def test_extract_context_params_no_context(self) -> None:
        """Test context parameter extraction with no context params."""
        # Given: Function without context parameters
        function = FunctionMetadata(
            schema_name="public",
            function_name="get_status",
            parameters=[
                ParameterInfo("p_status_id", "uuid", "IN", None),
            ],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="getStatus",
            description=None,
            success_type="Status",
            error_type="StatusError",
            context_params=None,
        )

        # When: Extract context params
        type_mapper = TypeMapper()
        input_generator = InputGenerator(type_mapper)
        mutation_generator = MutationGenerator(input_generator)
        context_params = mutation_generator._extract_context_params(function, annotation)

        # Then: Empty dict (no context params)
        assert context_params == {}

    def test_extract_context_params_generic_auth_prefix(self) -> None:
        """Test generic auth_ prefix support (e.g., auth_organization_id)."""
        # Given: Function with non-standard auth param
        function = FunctionMetadata(
            schema_name="app",
            function_name="create_item",
            parameters=[
                ParameterInfo("p_name", "text", "IN", None),
                ParameterInfo("auth_organization_id", "uuid", "IN", None),  # Non-standard
            ],
            return_type="jsonb",
            comment=None,
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="createItem",
            description=None,
            success_type="Item",
            error_type="ItemError",
            context_params=None,
        )

        # When: Extract context params
        type_mapper = TypeMapper()
        input_generator = InputGenerator(type_mapper)
        mutation_generator = MutationGenerator(input_generator)
        context_params = mutation_generator._extract_context_params(function, annotation)

        # Then: Generic auth_ handling (auth_organization_id → organization_id)
        assert context_params == {"organization_id": "auth_organization_id"}

    def test_function_comment_used_as_mutation_description(
        self, mutation_generator: MutationGenerator
    ):
        """Test that PostgreSQL function comments become GraphQL mutation descriptions."""
        # Given: Input class and types
        input_cls = type("CreateUserInput", (), {"__annotations__": {"name": str, "email": str}})

        success_type = type("User", (), {})
        error_type = type("ValidationError", (), {})

        function_metadata = FunctionMetadata(
            schema_name="app",
            function_name="fn_create_user",
            parameters=[],
            return_type="jsonb",
            comment="Creates a new user account with email verification",  # PostgreSQL comment
            language="plpgsql",
        )

        annotation = MutationAnnotation(
            name="createUser",
            success_type="User",
            error_type="ValidationError",
            description=None,  # No explicit description
        )

        # When: Create mutation class
        mutation_cls = mutation_generator._create_mutation_class(
            function_metadata, annotation, input_cls, success_type, error_type
        )

        # Then: Class uses function comment as description
        assert mutation_cls.__doc__ == "Creates a new user account with email verification"
