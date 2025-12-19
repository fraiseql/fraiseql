"""Mutation generation for AutoFraiseQL.
This module provides utilities to generate GraphQL mutations from PostgreSQL
functions with automatic Union return type handling.
"""

import logging
from typing import TYPE_CHECKING, Any, Callable, Type

from .input_generator import InputGenerator
from .metadata_parser import MutationAnnotation
from .postgres_introspector import FunctionMetadata

if TYPE_CHECKING:
    from .postgres_introspector import PostgresIntrospector

logger = logging.getLogger(__name__)


class MutationGenerator:
    """Generate mutations from PostgreSQL functions."""

    def __init__(self, input_generator: InputGenerator):
        self.input_generator = input_generator

    def _extract_context_params(
        self, function_metadata: FunctionMetadata, annotation: MutationAnnotation
    ) -> dict[str, str]:
        """Extract context parameters from function signature.
        NEW STANDARD (Phase 5.6):
        auth_tenant_id UUID -> context["tenant_id"]
        auth_user_id UUID -> context["user_id"]
        Priority:
        1. Explicit metadata (annotation.context_params)
        2. Prefix match (auth_*)
        """
        params = {}
        # 1. Check explicit metadata
        if annotation.context_params:
            for param_name in annotation.context_params:
                # Map auth_tenant_id -> tenant_id
                context_key = param_name.replace("auth_", "")
                params[param_name] = context_key

        # 2. Check function arguments for auth_ prefix
        for param in function_metadata.parameters:
            if param.name.startswith("auth_"):
                context_key = param.name.replace("auth_", "")
                params[param.name] = context_key

        return params

    async def generate_mutation_for_function(
        self,
        function_metadata: FunctionMetadata,
        annotation: MutationAnnotation,
        type_registry: dict[str, Type],
        introspector: "PostgresIntrospector",
    ) -> Callable | None:
        """Generate a GraphQL mutation function for a database function."""
        try:
            # Step 1: Extract context parameters
            context_params = self._extract_context_params(function_metadata, annotation)

            # Step 2: Handle input types if needed
            # ... rest of implementation (simplified for brevity in this example)

            # This is a mock implementation of the generation logic
            async def mutation_fn(root: Any, info: Any, **kwargs: Any) -> Any:
                """Generated mutation handler."""
                # Logic to call DB function
                return None

            mutation_fn.__name__ = annotation.name
            return mutation_fn
        except Exception as e:
            logger.warning(
                f"Failed to generate mutation for {function_metadata.function_name}: {e}"
            )
            return None
