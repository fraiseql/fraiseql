"""Clean mutation decorator with guaranteed error management."""

from typing import Type, TypeVar, Dict, Any, Callable
from .result_processor import MutationResultProcessor

T = TypeVar('T')


def clean_mutation(
    function: str,
    schema: str = "app", 
    context_params: Dict[str, str] = None
):
    """Clean mutation decorator with predictable error management."""
    
    def decorator(cls: Type[T]) -> Type[T]:
        # Validate class structure
        _validate_mutation_class(cls)
        
        # Create resolver with clean error management
        resolver = _create_clean_resolver(cls, function, schema, context_params)
        
        # Store resolver on class for testing
        cls._get_resolver = lambda: resolver
        
        # Register with GraphQL (simplified for now)
        _register_graphql_resolver(cls, resolver)
        
        return cls
    
    return decorator


def _validate_mutation_class(cls: Type[T]) -> None:
    """Validate that the mutation class has required structure."""
    required_attrs = ['Input', 'Success', 'Error']
    missing_attrs = []
    
    for attr in required_attrs:
        if not hasattr(cls, attr):
            missing_attrs.append(attr)
    
    if missing_attrs:
        raise ValueError(
            f"Mutation class {cls.__name__} missing required attributes: {missing_attrs}. "
            f"Required: {required_attrs}"
        )


def _create_clean_resolver(cls, function, schema, context_params):
    """Create resolver with clean error management."""
    processor = MutationResultProcessor()
    
    async def resolver(info, **kwargs):
        # Extract context parameters
        context_args = _extract_context_args(info, context_params or {})
        
        # Call database function
        db_result = await info.context["db"].execute_function(
            f"{schema}.{function}",
            **context_args,
            **kwargs
        )
        
        # Process result with clean error management
        if _is_error_status(db_result.status):
            return processor.process_error(db_result, cls.Error)
        else:
            return processor.process_success(db_result, cls.Success)
    
    return resolver


def _extract_context_args(info, context_params: Dict[str, str]) -> Dict[str, Any]:
    """Extract context parameters from GraphQL info."""
    context_args = {}
    
    for param_name, context_key in context_params.items():
        if context_key in info.context:
            context_args[param_name] = info.context[context_key]
    
    return context_args


def _is_error_status(status: str) -> bool:
    """Determine if a status indicates an error condition."""
    if not status:
        return False
    
    error_prefixes = ['noop:', 'blocked:', 'failed:', 'error:']
    return any(status.startswith(prefix) for prefix in error_prefixes) or status == 'error'


def _register_graphql_resolver(cls, resolver):
    """Register the resolver with GraphQL schema."""
    # For now, this is a placeholder
    # In the real implementation, this would integrate with FraiseQL's schema registration
    pass