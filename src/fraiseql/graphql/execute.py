"""Custom GraphQL execution with passthrough support."""

import asyncio
import json
import logging
from typing import Any, Dict, Optional

from graphql import (
    ExecutionResult,
    GraphQLSchema,
    parse,
)

from fraiseql.core.raw_json_executor import RawJSONResult
from fraiseql.graphql.passthrough_type import PassthroughResult

logger = logging.getLogger(__name__)


async def execute_with_passthrough_check(
    schema: GraphQLSchema,
    source: str,
    root_value: Any = None,
    context_value: Any = None,
    variable_values: Optional[Dict[str, Any]] = None,
    operation_name: Optional[str] = None,
    enable_introspection: bool = True,
) -> ExecutionResult:
    """Execute GraphQL with mutation-aware passthrough detection and introspection control.

    This function automatically disables JSON passthrough for mutations and subscriptions
    to ensure error auto-population and consistent behavior. Queries continue to use
    passthrough when enabled for optimal performance.

    Mutation Behavior:
        - Mutations NEVER use JSON passthrough, regardless of context settings
        - This ensures error auto-population (ALWAYS_DATA_CONFIG) works correctly
        - Guarantees consistent error handling across all mutations

    Args:
        schema: GraphQL schema to execute against
        source: GraphQL query string
        root_value: Root value for execution
        context_value: Context passed to resolvers (may be modified to disable passthrough)
        variable_values: Query variables
        operation_name: Operation name for multi-operation documents
        enable_introspection: Whether to allow introspection queries (default: True)

    Returns:
        ExecutionResult containing the query result or validation errors

    Note:
        The context_value may be modified to disable JSON passthrough for mutations.
    """
    logger.info("execute_with_passthrough_check called")
    # Import our custom execution context

    from fraiseql.graphql.passthrough_context import PassthroughExecutionContext

    # Parse the query
    try:
        document = parse(source)
    except Exception as e:
        return ExecutionResult(data=None, errors=[e])

    # Check if this is a mutation - mutations should NEVER use passthrough
    # to ensure error auto-population and consistent behavior
    from graphql import OperationType

    operation_type = None
    for definition in document.definitions:
        if hasattr(definition, "operation"):
            operation_type = definition.operation
            break

    # Disable passthrough for mutations and subscriptions regardless of context settings
    if (
        operation_type in (OperationType.MUTATION, OperationType.SUBSCRIPTION)
        and context_value
        and context_value.get("json_passthrough")
    ):
        logger.info(
            "Disabling JSON passthrough for mutation - ensuring error auto-population works"
        )
        context_value["json_passthrough"] = False
        if "execution_mode" in context_value:
            context_value["execution_mode"] = "standard"
        # Also disable it in db context if present
        if "db" in context_value and hasattr(context_value["db"], "context"):
            context_value["db"].context["json_passthrough"] = False
            if hasattr(context_value["db"], "mode"):
                context_value["db"].mode = "standard"

    # Check for passthrough mode hint
    use_passthrough = False
    if source.strip().startswith("# @mode: passthrough"):
        use_passthrough = True
        logger.debug("Query has @mode: passthrough hint")
    elif source.strip().startswith("# @mode: turbo"):
        use_passthrough = True
        logger.debug("Query has @mode: turbo hint (using passthrough)")

    # Set passthrough flag in context
    if use_passthrough and context_value:
        context_value["json_passthrough"] = True
        context_value["execution_mode"] = "passthrough"

    # Use custom execution with our PassthroughExecutionContext
    # This allows us to intercept before type validation
    from graphql.execution import execute
    from graphql.validation import validate

    # Always validate the document against the schema
    validation_rules = []

    # Add introspection validation rule if disabled
    if not enable_introspection:
        from graphql import NoSchemaIntrospectionCustomRule

        validation_rules.append(NoSchemaIntrospectionCustomRule)
        logger.info("Introspection disabled - adding NoSchemaIntrospectionCustomRule")

    # Validate the document against the schema
    validation_errors = validate(schema, document, validation_rules or None)
    if validation_errors:
        if not enable_introspection and validation_rules:
            logger.warning(
                "Introspection query blocked: %s", [err.message for err in validation_errors]
            )
        else:
            logger.warning(
                "Schema validation failed: %s", [err.message for err in validation_errors]
            )
        return ExecutionResult(data=None, errors=validation_errors)

    result = execute(
        schema,
        document,
        root_value,
        context_value,
        variable_values,
        operation_name,
        execution_context_class=PassthroughExecutionContext,
    )

    # Handle async result if needed
    if asyncio.iscoroutine(result):
        result = await result

    # Check if result contains RawJSONResult at any level
    if result.data:
        # First check if the entire data is RawJSONResult
        if isinstance(result.data, RawJSONResult):
            logger.debug("Entire result.data is RawJSONResult")
            return result

        # Otherwise check nested fields
        raw_json = extract_raw_json_result(result.data)
        if raw_json:
            logger.debug("Found RawJSONResult in execution result")
            # Return a new ExecutionResult with the raw JSON
            return ExecutionResult(data=raw_json, errors=result.errors)

    # Clean @fraise_type objects before returning to prevent JSON serialization issues
    cleaned_result = _serialize_fraise_types_in_result(result)
    return cleaned_result


def extract_raw_json_result(data: Any) -> Optional[RawJSONResult]:
    """Extract RawJSONResult from the data structure.

    This function recursively searches for RawJSONResult or PassthroughResult
    and converts them appropriately.
    """
    if isinstance(data, RawJSONResult):
        return data

    if isinstance(data, PassthroughResult):
        # Convert PassthroughResult to RawJSONResult
        return data.to_raw_json()

    if isinstance(data, dict):
        # Check each field
        for field_name, value in data.items():
            # Check if this field is RawJSONResult
            if isinstance(value, RawJSONResult):
                # For RawJSONResult, we need to wrap it in the GraphQL response structure
                # The RawJSONResult contains the field data, not the full response
                raw_data = json.loads(value.json_string)
                graphql_response = {"data": {field_name: raw_data}}
                return RawJSONResult(json.dumps(graphql_response))

            # Check if this field has PassthroughResult
            if isinstance(value, PassthroughResult):
                # Build complete GraphQL response with this field
                graphql_response = {"data": {field_name: value.data}}
                return RawJSONResult(json.dumps(graphql_response))

            # Recursively check
            result = extract_raw_json_result(value)
            if result:
                return result

    if isinstance(data, list):
        # Check if list contains RawJSONResult
        if data and isinstance(data[0], RawJSONResult):
            # This means the entire list field returned raw JSON
            return data[0]

        # Check if list contains PassthroughResult
        if data and isinstance(data[0], PassthroughResult):
            # Get the actual data from the first item
            actual_data = data[0].data
            field_name = data[0].field_name
            # Build GraphQL response
            graphql_response = {"data": {field_name: actual_data}}
            return RawJSONResult(json.dumps(graphql_response))

        # Otherwise check each item
        for item in data:
            result = extract_raw_json_result(item)
            if result:
                return result

    return None


def _serialize_fraise_types_in_result(result: ExecutionResult) -> ExecutionResult:
    """Convert @fraise_type objects to dicts for JSON serialization.

    This function processes the GraphQL ExecutionResult to convert any @fraise_type
    objects (those decorated with @fraiseql.type) into plain dictionaries that can
    be safely serialized to JSON. This prevents "Object of type X is not JSON
    serializable" errors when the GraphQL library attempts to serialize the response.

    Args:
        result: The ExecutionResult from GraphQL execution

    Returns:
        A new ExecutionResult with all @fraise_type objects converted to dicts
    """
    import logging

    logger = logging.getLogger(__name__)
    logger.info(f"_serialize_fraise_types_in_result called: has_data={result.data is not None}")

    if result.data:
        logger.info(f"Cleaning data of type: {type(result.data)}")
        cleaned_data = _clean_fraise_types(result.data)
        logger.info(f"Cleaned data type: {type(cleaned_data)}")
        return ExecutionResult(
            data=cleaned_data, errors=result.errors, extensions=result.extensions
        )
    return result


def _clean_fraise_types(obj: Any, _seen: set | None = None) -> Any:
    """Recursively convert @fraise_type objects to dictionaries.

    This function walks through a data structure and converts any objects that
    have the __fraiseql_definition__ attribute (indicating they are @fraise_type
    objects) into plain dictionaries using the same logic as FraiseQLJSONEncoder.

    Args:
        obj: The object to clean (can be dict, list, @fraise_type object, or primitive)
        _seen: Internal parameter to track seen objects and prevent infinite recursion

    Returns:
        The cleaned object with all @fraise_type objects converted to dicts
    """
    if _seen is None:
        _seen = set()

    # Debug logging at the start
    import logging

    logger = logging.getLogger(__name__)
    if hasattr(obj, "__class__"):
        logger.info(f"_clean_fraise_types called on: {obj.__class__.__name__}")

    # Handle FraiseQL types first (objects with __fraiseql_definition__)
    if hasattr(obj, "__fraiseql_definition__"):
        # Convert @fraise_type object to dictionary with recursive cleaning
        obj_dict = {}

        # Add __typename field for GraphQL union type resolution
        # This allows the GraphQL union resolver to identify the correct type
        if hasattr(obj, "__class__") and hasattr(obj.__class__, "__name__"):
            obj_dict["__typename"] = obj.__class__.__name__

            # CRITICAL FIX: Force errors array population for frontend compatibility
            # If this is an Error type with null errors field, auto-populate it
            class_name = obj.__class__.__name__

            # Debug logging
            import logging

            logger = logging.getLogger(__name__)
            logger.info(f"Processing object: {class_name}")
            logger.info(f"Has errors attr: {hasattr(obj, 'errors')}")
            if hasattr(obj, "errors"):
                logger.info(f"Errors value: {obj.errors}")
            logger.info(f"Has status attr: {hasattr(obj, 'status')}")
            logger.info(f"Has message attr: {hasattr(obj, 'message')}")

            if (
                class_name.endswith("Error")
                and hasattr(obj, "errors")
                and obj.errors is None
                and hasattr(obj, "status")
                and hasattr(obj, "message")
            ):
                # Create error structure from the status and message
                status = getattr(obj, "status", "")
                message = getattr(obj, "message", "Unknown error")

                # Extract error code and identifier from status
                if ":" in status:
                    error_code = 422  # Unprocessable Entity for noop: statuses
                    identifier = status.split(":", 1)[1] if ":" in status else "unknown_error"
                else:
                    error_code = 500  # Internal Server Error for other statuses
                    identifier = "general_error"

                # Create error object
                error_obj = {
                    "code": error_code,
                    "identifier": identifier,
                    "message": message,
                    "details": {},
                }

                obj_dict["errors"] = [error_obj]
                # Also force-set on the original object to ensure consistency
                obj.errors = [error_obj]

        for attr_name in dir(obj):
            # Skip private attributes, methods, and special FraiseQL attributes
            if (
                not attr_name.startswith("_")
                and not attr_name.startswith("__gql_")
                and not attr_name.startswith("__fraiseql_")
                and not callable(getattr(obj, attr_name, None))
            ):
                value = getattr(obj, attr_name, None)
                if value is not None:
                    # Recursively clean the value
                    obj_dict[attr_name] = _clean_fraise_types(value, _seen)
        return obj_dict

    # Handle Python Enums (convert to their value)
    if hasattr(obj, "__class__") and hasattr(obj.__class__, "__bases__"):
        import enum

        if isinstance(obj, enum.Enum):
            return obj.value

    # Handle circular references for non-@fraise_type objects
    obj_id = id(obj)
    is_complex = isinstance(obj, (dict, list)) or (
        hasattr(obj, "__dict__") and not isinstance(obj, (str, int, float, bool, type(None)))
    )

    if is_complex and obj_id in _seen:
        return obj  # Return as-is to break circular reference

    # Add complex objects to seen set
    if is_complex:
        _seen.add(obj_id)

    try:
        # Handle lists - recursively clean each item
        if isinstance(obj, list):
            return [_clean_fraise_types(item, _seen) for item in obj]

        # Handle dicts - recursively clean each value
        if isinstance(obj, dict):
            return {k: _clean_fraise_types(v, _seen) for k, v in obj.items()}

        # Handle objects with __dict__ that might contain @fraise_type objects
        if hasattr(obj, "__dict__") and not isinstance(obj, (str, int, float, bool, type(None))):
            # For objects that aren't primitives, check their attributes
            cleaned_obj = obj
            for attr_name in dir(obj):
                if not attr_name.startswith("_") and hasattr(obj, attr_name):
                    attr_value = getattr(obj, attr_name, None)
                    if attr_value is not None and not callable(attr_value):
                        cleaned_value = _clean_fraise_types(attr_value, _seen)
                        # Only modify if the value actually changed
                        if cleaned_value is not attr_value:
                            # Create a copy to avoid modifying the original
                            if cleaned_obj is obj:
                                import copy

                                cleaned_obj = copy.copy(obj)
                            setattr(cleaned_obj, attr_name, cleaned_value)
            return cleaned_obj

        # Return primitives and other objects as-is
        return obj

    finally:
        # Remove from seen set when done processing
        if is_complex:
            _seen.discard(obj_id)


class PassthroughResolver:
    """Wrapper for resolvers that can return raw JSON."""

    def __init__(self, original_resolver, field_name: str):
        self.original_resolver = original_resolver
        self.field_name = field_name

    async def __call__(self, source, info, **kwargs):
        """Execute resolver and handle raw JSON results."""
        # Check if passthrough is enabled - respect configuration
        use_passthrough = (
            info.context.get("json_passthrough", False)
            or info.context.get("execution_mode") == "passthrough"
            or (
                info.context.get("mode") in ("production", "staging")
                and info.context.get("json_passthrough_in_production", False)
            )
        )

        # Execute the original resolver
        result = self.original_resolver(source, info, **kwargs)

        # Handle async
        if asyncio.iscoroutine(result):
            result = await result

        # If it's already RawJSONResult, return as-is
        if isinstance(result, RawJSONResult):
            logger.debug(f"Resolver {self.field_name} returned RawJSONResult")
            return result

        # In passthrough mode, check for raw dicts
        if use_passthrough:
            if isinstance(result, dict) and "__typename" in result:
                # This looks like raw JSON from DB
                logger.debug(f"Converting dict to RawJSONResult for {self.field_name}")
                # Wrap just this field's data
                field_json = json.dumps(result)
                return RawJSONResult(field_json)

            if (
                isinstance(result, list)
                and result
                and all(isinstance(item, dict) and "__typename" in item for item in result)
            ):
                logger.debug(f"Converting list to RawJSONResult for {self.field_name}")
                # Wrap the list
                field_json = json.dumps(result)
                return RawJSONResult(field_json)

        # Return normal result
        return result


def wrap_resolver_for_passthrough(resolver, field_name: str):
    """Wrap a resolver to support passthrough mode.

    This allows resolvers to return raw JSON that bypasses
    GraphQL type validation.
    """
    if resolver is None:
        return None

    # Check if already wrapped
    if isinstance(resolver, PassthroughResolver):
        return resolver

    # Create wrapper
    wrapper = PassthroughResolver(resolver, field_name)

    # Preserve any attributes from original
    if hasattr(resolver, "__name__"):
        wrapper.__name__ = resolver.__name__
    if hasattr(resolver, "__wrapped__"):
        wrapper.__wrapped__ = resolver.__wrapped__

    return wrapper
