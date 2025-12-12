"""PostgreSQL function-based mutation decorator."""

import logging
import re
from collections.abc import Callable
from typing import Any, TypeVar, get_type_hints

from graphql import GraphQLResolveInfo

from fraiseql.mutations.error_config import MutationErrorConfig
from fraiseql.types.definitions import UNSET
from fraiseql.utils.casing import to_snake_case

T = TypeVar("T")

logger = logging.getLogger(__name__)


def _extract_fields_from_selection_set(selection_set: Any, field_set: set[str]) -> None:
    """Helper to extract field names from selection set recursively."""
    for field_selection in selection_set.selections:
        if hasattr(field_selection, "name"):
            field_name = field_selection.name.value
            if field_name != "__typename":
                field_set.add(field_name)


def _extract_mutation_selected_fields(info: GraphQLResolveInfo, type_name: str) -> list[str] | None:
    """Extract fields selected on a mutation response type from GraphQL query.

    Supports both inline fragments (... on Type) and named fragments (...FragmentName).

    For mutations with union types, looks for fragments on the specific type.
    Returns None if no specific selection found (= return all fields for backward compat).

    Example query (inline fragment):
        mutation {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    status
                    machine { id }
                }
            }
        }

    Example query (named fragment):
        fragment MachineFields on CreateMachineSuccess {
            status
            machine { id }
        }
        mutation {
            createMachine(input: $input) {
                ...MachineFields
            }
        }

    Extracts: ["status", "machine"] for type_name="CreateMachineSuccess"
    """
    if not info or not info.field_nodes:
        return None

    selected_fields = set()

    # Mutations typically have one field_node (the mutation field)
    for field_node in info.field_nodes:
        if not field_node.selection_set:
            continue

        # Look through selections for fragments matching our type
        for selection in field_node.selection_set.selections:
            # InlineFragment with type condition (e.g., "... on CreateMachineSuccess")
            if hasattr(selection, "type_condition") and selection.type_condition:
                fragment_type = selection.type_condition.name.value

                if fragment_type == type_name and selection.selection_set:
                    # Extract fields from this inline fragment
                    _extract_fields_from_selection_set(selection.selection_set, selected_fields)

            # Named fragment spread (e.g., "...FragmentName")
            elif hasattr(selection, "name") and hasattr(info, "fragments"):
                fragment_name = selection.name.value
                fragment = info.fragments.get(fragment_name)

                if fragment and hasattr(fragment, "type_condition"):
                    fragment_type = fragment.type_condition.name.value

                    if fragment_type == type_name:
                        # Extract fields from this named fragment
                        _extract_fields_from_selection_set(fragment.selection_set, selected_fields)

    if not selected_fields:
        return None

    result = list(selected_fields)
    return result


class MutationDefinition:
    """Definition of a PostgreSQL-backed mutation."""

    def __init__(
        self,
        mutation_class: type,
        function_name: str | None = None,
        schema: str | None = None,
        context_params: dict[str, str] | None = None,
        error_config: MutationErrorConfig | None = None,
        enable_cascade: bool = False,
    ) -> None:
        self.mutation_class = mutation_class
        self.name = mutation_class.__name__

        # Store the provided schema for lazy resolution
        self._provided_schema = schema
        self._resolved_schema = None  # Will be resolved lazily
        self._provided_error_config = error_config  # Store provided value
        self._resolved_error_config = None  # Lazy resolution

        self.context_params = context_params or {}
        self.enable_cascade = enable_cascade

        # Get type hints
        hints = get_type_hints(mutation_class)
        self.input_type = hints.get("input")
        self.success_type = hints.get("success")
        self.error_type = hints.get("error")

        # Derive function name from class name if not provided
        if function_name:
            self.function_name = function_name
        else:
            # Convert CamelCase to snake_case
            # CreateUser -> create_user
            self.function_name = _camel_to_snake(self.name)

        # Derive entity field name and type from success type
        self.entity_field_name, self.entity_type = self._derive_entity_info()

    def _derive_entity_info(self) -> tuple[str | None, str | None]:
        """Derive entity field name and type from success type annotations.

        Returns:
            Tuple of (entity_field_name, entity_type)
        """
        if not self.success_type:
            return None, None

        # Get annotations from the success type
        success_hints = get_type_hints(self.success_type)

        # Look for common entity field names
        entity_fields = [
            "user",
            "post",
            "comment",
            "tag",
            "organization",
            "project",
            "task",
            "location",
            "category",
            "item",
            "entity",
        ]

        for field_name, field_type in success_hints.items():
            if field_name in entity_fields:
                # Get the type name (e.g., "User" from <class 'User'>)
                type_name = getattr(field_type, "__name__", None)
                if type_name:
                    return field_name, type_name

        # Fallback: try to find any field that looks like an entity
        for field_name, field_type in success_hints.items():
            # Skip common non-entity fields
            if field_name in ["id", "message", "success", "error", "errors", "code", "status"]:
                continue

            # If it's a type (class), assume it's the entity
            if hasattr(field_type, "__name__"):
                type_name = field_type.__name__
                return field_name, type_name

        return None, None

    @property
    def schema(self) -> str:
        """Get the schema, resolving it lazily if needed."""
        if self._resolved_schema is None:
            self._resolved_schema = self._resolve_schema(self._provided_schema)
        return self._resolved_schema

    @schema.setter
    def schema(self, value: str) -> None:
        """Allow setting the schema directly for testing."""
        self._resolved_schema = value

    def _resolve_schema(self, provided_schema: str | None) -> str:
        """Resolve the schema to use, considering defaults from config."""
        # If schema was explicitly provided, use it
        if provided_schema is not None:
            return provided_schema

        # Try to get default from registry config
        try:
            from fraiseql.gql.builders.registry import SchemaRegistry

            registry = SchemaRegistry.get_instance()

            if registry.config and hasattr(registry.config, "default_mutation_schema"):
                return registry.config.default_mutation_schema
        except ImportError:
            pass

        # Fall back to "public" as per feature requirements
        return "public"

    @property
    def error_config(self) -> MutationErrorConfig | None:
        """Get the error config, resolving it lazily if needed."""
        if self._resolved_error_config is None:
            self._resolved_error_config = self._resolve_error_config(self._provided_error_config)
        return self._resolved_error_config

    def _resolve_error_config(
        self, provided_error_config: MutationErrorConfig | None
    ) -> MutationErrorConfig | None:
        """Resolve the error config to use, considering defaults from config.

        Resolution order:
        1. Explicit error_config parameter on decorator (highest priority)
        2. default_error_config from FraiseQLConfig
        3. None (no error configuration)
        """
        # If error_config was explicitly provided, use it (even if None)
        if provided_error_config is not None:
            return provided_error_config

        # Try to get default from registry config
        try:
            from fraiseql.gql.builders.registry import SchemaRegistry

            registry = SchemaRegistry.get_instance()

            if registry.config and hasattr(registry.config, "default_error_config"):
                return registry.config.default_error_config
        except ImportError:
            pass

        # Fall back to None (no error configuration)
        return None

    def _get_cascade_selections(self, info: GraphQLResolveInfo) -> str | None:
        """Extract CASCADE selections from GraphQL query if enabled."""
        if not self.enable_cascade:
            return None

        from fraiseql.mutations.cascade_selections import extract_cascade_selections

        return extract_cascade_selections(info)

    def validate_types(self) -> None:
        """Validate Success and Error types conform to v1.8.0 requirements."""
        # Validate Success type
        if not self.success_type:
            raise ValueError(f"Mutation {self.name} must have a success type")

        success_type_name = getattr(self.success_type, "__name__", "Success")
        if not hasattr(self.success_type, "__annotations__"):
            raise ValueError(f"Success type {success_type_name} must have annotations")

        success_annotations = self.success_type.__annotations__

        # Success must have entity field
        entity_field = self._get_entity_field_name()
        if entity_field not in success_annotations:
            raise ValueError(
                f"Success type {success_type_name} must have '{entity_field}' field. "
                f"v1.8.0 requires Success types to always have non-null entity."
            )

        # Entity field must NOT be Optional
        entity_type = success_annotations[entity_field]
        if self._is_optional(entity_type):
            raise ValueError(
                f"Success type {success_type_name} has nullable entity field. "
                f"v1.8.0 requires entity to be non-null. "
                f"Change '{entity_field}: {entity_type}' to non-nullable type."
            )

        # Validate Error type
        if not self.error_type:
            raise ValueError(f"Mutation {self.name} must have an error type")

        error_type_name = getattr(self.error_type, "__name__", "Error")
        if not hasattr(self.error_type, "__annotations__"):
            raise ValueError(f"Error type {error_type_name} must have annotations")

        error_annotations = self.error_type.__annotations__

        # NOTE: code field validation REMOVED (v1.8.1)
        # The 'code' field is now auto-injected by @fraiseql.error decorator
        # No manual definition required - automatically added to all Error types

        # Error must have status field
        if "status" not in error_annotations:
            raise ValueError(f"Error type {error_type_name} must have 'status: str' field.")

        # Error must have message field
        if "message" not in error_annotations:
            raise ValueError(f"Error type {error_type_name} must have 'message: str' field.")

    def _get_entity_field_name(self) -> str:
        """Get entity field name from Success type.

        Looks for common patterns: entity, <lowercase_type>, etc.
        """
        if not self.success_type:
            raise ValueError("Success type not set")

        annotations = self.success_type.__annotations__

        # Common patterns
        if "entity" in annotations:
            return "entity"

        # Try lowercase type name (e.g., CreateMachineSuccess → machine)
        mutation_name = getattr(self.success_type, "__name__", "").replace("Success", "")
        entity_name_candidate = mutation_name.lower()
        if entity_name_candidate in annotations:
            return entity_name_candidate

        # Fallback: first non-standard field
        standard_fields = {"cascade", "message", "updated_fields", "code", "status"}
        for field in annotations:
            if field not in standard_fields:
                return field

        raise ValueError(
            f"Could not determine entity field name for {getattr(self.success_type, '__name__', 'Success')}. "  # noqa: E501
            f"Expected 'entity' or lowercase mutation name."
        )

    def _is_optional(self, type_hint: Any) -> bool:
        """Check if type hint is Optional (includes None)."""
        import typing

        # Check for X | None (Python 3.10+)
        if hasattr(typing, "get_args") and hasattr(typing, "get_origin"):
            origin = typing.get_origin(type_hint)
            if origin is typing.Union:
                args = typing.get_args(type_hint)
                return type(None) in args

        # Check for Optional[X] (older syntax)
        return getattr(type_hint, "__origin__", None) is typing.Union and type(None) in getattr(
            type_hint, "__args__", []
        )

    def create_resolver(self) -> Callable:
        """Create the GraphQL resolver function."""

        async def resolver(info: GraphQLResolveInfo, input: dict[str, Any]) -> Any:
            """Auto-generated resolver for PostgreSQL mutation."""
            # Get database connection
            db = info.context.get("db")
            if not db:
                msg = "No database connection in context"
                raise RuntimeError(msg)

            # Convert input to dict
            input_data = _to_dict(input)

            # Call prepare_input hook if defined on mutation class
            if hasattr(self.mutation_class, "prepare_input"):
                input_data = self.mutation_class.prepare_input(input_data)

            # Call PostgreSQL function via Rust executor
            from fraiseql.mutations.rust_executor import execute_mutation_rust

            full_function_name = f"{self.schema}.{self.function_name}"
            # GraphQL field name: CreatePost -> createPost (lowercase first letter)
            field_name = self.name[0].lower() + self.name[1:] if self.name else self.name
            success_type_name = getattr(self.success_type, "__name__", "Success")  # type: ignore[attr-defined]
            error_type_name = getattr(self.error_type, "__name__", "Error")

            # Extract context arguments
            context_args = []
            if self.context_params:
                for context_key in self.context_params:
                    context_value = info.context.get(context_key)
                    if context_value is None:
                        msg = (
                            f"Required context parameter '{context_key}' "
                            f"not found in GraphQL context"
                        )
                        raise RuntimeError(msg)

                    # Extract specific field if it's a UserContext object
                    if hasattr(context_value, "user_id") and context_key == "user":
                        context_args.append(context_value.user_id)
                    else:
                        context_args.append(context_value)

            # Get connection pool from repository
            # db is a FraiseQLRepository, we need to get the underlying pool
            pool = db.get_pool() if hasattr(db, "get_pool") else db._pool

            # Call Rust executor with a connection from the pool
            async with pool.connection() as conn:
                logger.debug(f"Executing mutation {self.name} with function {full_function_name}")
                logger.debug(f"Input data keys: {list(input_data.keys()) if input_data else None}")
                logger.debug(f"Success type: {success_type_name}, Error type: {error_type_name}")

                # Extract selected fields from GraphQL query for field filtering
                # Returns None if no specific selection found (backward compat: return all fields)
                success_type_fields = _extract_mutation_selected_fields(info, success_type_name)
                error_type_fields = _extract_mutation_selected_fields(info, error_type_name)
                logger.debug(f"Selected success fields from query: {success_type_fields}")
                logger.debug(f"Selected error fields from query: {error_type_fields}")

                # Extract CASCADE selections from GraphQL query
                cascade_selections_json = self._get_cascade_selections(info)

                try:
                    rust_response = await execute_mutation_rust(
                        conn=conn,
                        function_name=full_function_name,
                        input_data=input_data,
                        field_name=field_name,
                        success_type=success_type_name,
                        error_type=error_type_name,
                        entity_field_name=self.entity_field_name,
                        entity_type=self.entity_type,
                        context_args=context_args if context_args else None,
                        cascade_selections=cascade_selections_json,
                        success_type_class=self.success_type,
                        success_type_fields=success_type_fields,
                        error_type_fields=error_type_fields,
                    )
                    logger.debug(f"Mutation {self.name} executed successfully")
                except Exception as e:
                    logger.error(
                        f"Mutation {self.name} failed during execution",
                        extra={
                            "function": full_function_name,
                            "success_type": success_type_name,
                            "error_type": error_type_name,
                            "entity_field_name": self.entity_field_name,
                            "entity_type": self.entity_type,
                            "input_keys": list(input_data.keys()) if input_data else None,
                            "error": str(e),
                        },
                    )
                    raise

            # Check if we're in HTTP mode (set by FastAPI routers)
            # HTTP mode: return RustResponseBytes directly for performance
            # Non-HTTP mode: parse and return Python objects for direct GraphQL execution
            http_mode = info.context.get("_http_mode", False)

            if http_mode:
                # RUST-FIRST PATH: Return RustResponseBytes directly to HTTP
                # This bypasses Python JSON parsing entirely for maximum performance
                # PostgreSQL → Rust → HTTP bytes (zero Python string operations)
                return rust_response

            # NON-HTTP PATH: Convert to dict for GraphQL execute()
            # Used in tests and direct GraphQL execute() calls
            try:
                graphql_response = rust_response.to_json()
                mutation_result = graphql_response["data"][field_name]
                logger.debug(f"Parsed GraphQL response for field '{field_name}'")
            except Exception as e:
                logger.error(
                    f"Failed to parse GraphQL response for mutation {self.name}",
                    extra={
                        "field_name": field_name,
                        "error": str(e),
                        "response_type": type(rust_response).__name__,
                    },
                )
                raise

            # Return dict directly (no parsing into Python objects)
            # CASCADE is already at correct level from Rust
            # Tests will work with dict access: result["user"]["id"]
            return mutation_result

        # Set metadata for GraphQL introspection
        # Create unique resolver name to prevent collisions between similar mutation names
        # Add the PostgreSQL function name as disambiguation when available
        base_name = to_snake_case(self.name)
        if hasattr(self, "function_name") and self.function_name:
            # Use function name to ensure uniqueness (e.g., create_item vs create_item_component)
            resolver_name = self.function_name
        else:
            resolver_name = base_name

        resolver.__name__ = resolver_name
        resolver.__doc__ = self.mutation_class.__doc__ or f"Mutation for {self.name}"

        # Store mutation definition for schema building
        resolver.__fraiseql_mutation__ = self

        # Set proper annotations for the resolver
        # We use FraiseUnion wrapper for success/error result unions
        from typing import Annotated

        from fraiseql.mutations.decorators import FraiseUnion

        if self.success_type and self.error_type:
            # Check if success and error types are the same (single result type pattern)
            if self.success_type is self.error_type:
                # Single result type used for both success and error - no union needed
                return_type = self.success_type
            else:
                # Create union name from success type (e.g., CreateUserSuccess -> CreateUserResult)
                success_name = getattr(self.success_type, "__name__", "Success")
                base_name = success_name.removesuffix("Success")
                union_name = f"{base_name}Result"

                # Wrap in FraiseUnion to indicate this is a result union
                return_type = Annotated[
                    self.success_type | self.error_type,
                    FraiseUnion(union_name),
                ]
        else:
            return_type = self.success_type or self.error_type

        # Create a fresh annotations dict to avoid any shared reference issues
        resolver.__annotations__ = {"input": self.input_type, "return": return_type}

        return resolver


def mutation(
    _cls: type[T] | Callable[..., Any] | None = None,
    *,
    function: str | None = None,
    schema: str | None = None,
    context_params: dict[str, str] | None = None,
    error_config: MutationErrorConfig | None = None,
    enable_cascade: bool = False,
) -> type[T] | Callable[[type[T]], type[T]] | Callable[..., Any]:
    """Decorator to define GraphQL mutations with PostgreSQL function backing.

    This decorator supports both simple function-based mutations and sophisticated
    class-based mutations with structured success/error handling. Class-based mutations
    automatically call PostgreSQL functions and parse results into typed responses.

    Args:
        _cls: The mutation function or class to decorate (when used without parentheses)
        function: PostgreSQL function name (defaults to snake_case of class name)
        schema: PostgreSQL schema containing the function (defaults to "graphql")
        context_params: Maps GraphQL context keys to PostgreSQL function parameter names
        error_config: Optional configuration for error detection behavior
        enable_cascade: Enable GraphQL cascade functionality to include side effects in response

    Returns:
        Decorated mutation with automatic PostgreSQL function integration

    Examples:
        Simple function-based mutation::\

            @fraiseql.mutation
            async def create_user(info, input: CreateUserInput) -> User:
                db = info.context["db"]
                # Use SQL function for business logic
                result = await db.execute_function("fn_create_user", {
                    "name": input.name,
                    "email": input.email
                })
                return await db.find_one("v_user", "user", info, id=result["id"])

        Basic class-based mutation::\

            @fraiseql.mutation
            class CreateUser:
                input: CreateUserInput
                success: CreateUserSuccess
                error: CreateUserError

            # This automatically calls PostgreSQL function: graphql.create_user(input)
            # and parses the result into either CreateUserSuccess or CreateUserError

        Mutation with custom PostgreSQL function::\

            @fraiseql.mutation(function="register_new_user", schema="auth")
            class RegisterUser:
                input: RegistrationInput
                success: RegistrationSuccess
                error: RegistrationError

            # Calls: auth.register_new_user(input) instead of default name

        Mutation with context parameters::\

            @fraiseql.mutation(
                function="create_location",
                schema="app",
                context_params={
                    "tenant_id": "input_pk_organization",
                    "user": "input_created_by"
                }
            )
            class CreateLocation:
                input: CreateLocationInput
                success: CreateLocationSuccess
                error: CreateLocationError

            # Calls: app.create_location(tenant_id, user_id, input)
            # Where tenant_id comes from info.context["tenant_id"]
            # And user_id comes from info.context["user"].user_id

        Mutation with validation and error handling::\

            @fraise_input
            class UpdateUserInput:
                id: UUID
                name: str | None = None
                email: str | None = None

            @fraise_type
            class UpdateUserSuccess:
                user: User
                message: str

            @fraise_type
            class UpdateUserError:
                code: str
                message: str
                field: str | None = None

            @fraiseql.mutation
            async def update_user(info, input: UpdateUserInput) -> User:
                db = info.context["db"]
                user_context = info.context.get("user")

                # Authorization check
                if not user_context:
                    raise GraphQLError("Authentication required")

                # Validation
                if input.email and not is_valid_email(input.email):
                    raise GraphQLError("Invalid email format")

                # Update logic
                updates = {}
                if input.name:
                    updates["name"] = input.name
                if input.email:
                    updates["email"] = input.email

                if not updates:
                    raise GraphQLError("No fields to update")

                return await db.update_one("user_view", {"id": input.id}, updates)

        Multi-step mutation with transaction::\

            @fraiseql.mutation
            async def transfer_funds(
                info,
                input: TransferInput
            ) -> TransferResult:
                db = info.context["db"]

                async with db.transaction():
                    # Validate source account
                    source = await db.find_one(
                        "account_view",
                        {"id": input.source_account_id}
                    )
                    if not source or source.balance < input.amount:
                        raise GraphQLError("Insufficient funds")

                    # Validate destination account
                    dest = await db.find_one(
                        "account_view",
                        {"id": input.destination_account_id}
                    )
                    if not dest:
                        raise GraphQLError("Destination account not found")

                    # Perform transfer
                    await db.update_one(
                        "account_view",
                        {"id": source.id},
                        {"balance": source.balance - input.amount}
                    )
                    await db.update_one(
                        "account_view",
                        {"id": dest.id},
                        {"balance": dest.balance + input.amount}
                    )

                    # Log transaction
                    transfer = await db.create_one("transfer_view", {
                        "source_account_id": input.source_account_id,
                        "destination_account_id": input.destination_account_id,
                        "amount": input.amount,
                        "created_at": datetime.utcnow()
                    })

                    return TransferResult(
                        transfer=transfer,
                        new_source_balance=source.balance - input.amount,
                        new_dest_balance=dest.balance + input.amount
                    )

        Mutation with file upload handling::\

            @fraiseql.mutation
            async def upload_avatar(
                info,
                input: UploadAvatarInput  # Contains file: Upload field
            ) -> User:
                db = info.context["db"]
                storage = info.context["storage"]
                user_context = info.context["user"]

                if not user_context:
                    raise GraphQLError("Authentication required")

                # Process file upload
                file_content = await input.file.read()
                if len(file_content) > 5 * 1024 * 1024:  # 5MB limit
                    raise GraphQLError("File too large")

                # Store file
                file_url = await storage.store_user_avatar(
                    user_context.user_id,
                    file_content,
                    input.file.content_type
                )

                # Update user record
                return await db.update_one(
                    "user_view",
                    {"id": user_context.user_id},
                    {"avatar_url": file_url}
                )

        Mutation with input transformation using prepare_input hook::\

            @fraise_input
            class NetworkConfigInput:
                ip_address: str
                subnet_mask: str

            @fraiseql.mutation
            class CreateNetworkConfig:
                input: NetworkConfigInput
                success: NetworkConfigSuccess
                error: NetworkConfigError

                @staticmethod
                def prepare_input(input_data: dict) -> dict:
                    \"\"\"Transform IP + subnet mask to CIDR notation before database call.\"\"\"
                    ip = input_data.get("ip_address")
                    mask = input_data.get("subnet_mask")

                    if ip and mask:
                        # Convert subnet mask to CIDR prefix
                        cidr_prefix = {
                            "255.255.255.0": 24,
                            "255.255.0.0": 16,
                            "255.0.0.0": 8,
                        }.get(mask, 32)

                        return {
                            "ip_address": f"{ip}/{cidr_prefix}",
                            # subnet_mask field is removed
                        }
                    return input_data

            # Frontend sends: { ipAddress: "192.168.1.1", subnetMask: "255.255.255.0" }
            # Database receives: { ip_address: "192.168.1.1/24" }

        Mutation with empty string to null conversion::\

            @fraise_input
            class UpdateNoteInput:
                id: UUID
                notes: str | None = None

            @fraiseql.mutation
            class UpdateNote:
                input: UpdateNoteInput
                success: UpdateNoteSuccess
                error: UpdateNoteError

                @staticmethod
                def prepare_input(input_data: dict) -> dict:
                    \"\"\"Convert empty strings to None for nullable fields.\"\"\"
                    result = input_data.copy()

                    # Convert empty strings to None for optional string fields
                    if "notes" in result and result["notes"] == "":
                        result["notes"] = None

                    return result

            # Frontend sends: { id: "...", notes: "" }
            # Database receives: { id: "...", notes: null }

    PostgreSQL Function Requirements:
        For class-based mutations, the PostgreSQL function should:

        1. Accept input as JSONB parameter
        2. Return a result with 'success' boolean field
        3. Include either 'data' field (success) or 'error' field (failure)

        Example PostgreSQL function::\

            CREATE OR REPLACE FUNCTION graphql.create_user(input jsonb)
            RETURNS jsonb
            LANGUAGE plpgsql
            AS $$
            DECLARE
                user_id uuid;
                result jsonb;
            BEGIN
                -- Insert user
                INSERT INTO users (name, email, created_at)
                VALUES (
                    input->>'name',
                    input->>'email',
                    now()
                )
                RETURNING id INTO user_id;

                -- Return success response
                result := jsonb_build_object(
                    'success', true,
                    'data', jsonb_build_object(
                        'id', user_id,
                        'name', input->>'name',
                        'email', input->>'email',
                        'message', 'User created successfully'
                    )
                );

                RETURN result;
            EXCEPTION
                WHEN unique_violation THEN
                    -- Return error response
                    result := jsonb_build_object(
                        'success', false,
                        'error', jsonb_build_object(
                            'code', 'EMAIL_EXISTS',
                            'message', 'Email address already exists',
                            'field', 'email'
                        )
                    );
                    RETURN result;
            END;
            $$;

    Notes:
        - Function-based mutations provide full control over implementation
        - Class-based mutations automatically integrate with PostgreSQL functions
        - Use transactions for multi-step operations to ensure data consistency
        - PostgreSQL functions handle validation and business logic at the database level
        - Context parameters enable tenant isolation and user tracking
        - Success/error types provide structured response handling
        - All mutations are automatically registered with the GraphQL schema
        - The prepare_input hook allows transforming input data before database calls
        - prepare_input is called after GraphQL validation but before the PostgreSQL function
        - Use prepare_input for multi-field transformations, empty string normalization, etc.
    """

    def decorator(
        cls_or_fn: type[T] | Callable[..., Any],
    ) -> type[T] | Callable[..., Any]:
        # Import here to avoid circular imports
        from fraiseql.gql.schema_builder import SchemaRegistry

        registry = SchemaRegistry.get_instance()

        # Check if it's a function (simple mutation pattern)
        if callable(cls_or_fn) and not isinstance(cls_or_fn, type):
            # It's a function-based mutation
            fn = cls_or_fn

            # Store metadata for schema building
            fn.__fraiseql_mutation__ = True
            fn.__fraiseql_resolver__ = fn

            # Auto-register with schema
            registry.register_mutation(fn)

            return fn

        # Otherwise, it's a class-based mutation
        cls = cls_or_fn
        # Create mutation definition
        definition = MutationDefinition(
            cls, function, schema, context_params, error_config, enable_cascade
        )

        # Store definition on the class
        cls.__fraiseql_mutation__ = definition

        # Create and store resolver
        cls.__fraiseql_resolver__ = definition.create_resolver()

        # Auto-register with schema
        registry.register_mutation(cls)

        return cls

    if _cls is None:
        return decorator
    return decorator(_cls)


def _camel_to_snake(name: str) -> str:
    """Convert CamelCase to snake_case."""
    # Insert underscore before uppercase letters (except at start)
    s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", name)
    # Handle sequences of capitals
    return re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()


def _to_dict(obj: Any) -> dict[str, Any]:
    """Convert an object to a dictionary.

    UNSET values are excluded from the dictionary to enable partial updates.
    Only fields that were explicitly provided (including explicit None) are included.

    Empty strings are converted to None to support frontends that send "" when
    clearing text fields. This aligns with database NULL semantics and prevents
    empty string pollution in the database.
    """
    if hasattr(obj, "to_dict"):
        return obj.to_dict()
    if hasattr(obj, "__dict__"):
        # Convert UUIDs to strings for JSON serialization
        # Convert empty strings to None for database compatibility
        result = {}
        for k, v in obj.__dict__.items():
            if not k.startswith("_"):
                if v is UNSET:
                    # Skip UNSET fields entirely - they weren't provided
                    continue
                if hasattr(v, "hex"):  # UUID
                    result[k] = str(v)
                elif hasattr(v, "isoformat"):  # date, datetime, time
                    result[k] = v.isoformat()
                elif isinstance(v, str) and not v.strip():
                    # Convert empty strings to None for database NULL semantics
                    result[k] = None
                else:
                    result[k] = v
        return result
    if isinstance(obj, dict):
        return obj
    msg = f"Cannot convert {type(obj)} to dictionary"
    raise TypeError(msg)


def _filter_cascade_rust(cascade_data: dict, selections_json: str) -> dict:
    """Filter cascade data using Rust implementation.

    Args:
        cascade_data: Raw cascade data from PostgreSQL
        selections_json: JSON string of field selections from GraphQL query

    Returns:
        Filtered cascade data dict

    Raises:
        Exception: If Rust filtering fails (handled by caller)
    """
    import json

    from fraiseql import fraiseql_rs

    # Convert cascade data to JSON
    cascade_json = json.dumps(cascade_data, separators=(",", ":"))

    # Call Rust filter
    filtered_json = fraiseql_rs.filter_cascade_data(cascade_json, selections_json)

    # Parse back to dict
    return json.loads(filtered_json)
