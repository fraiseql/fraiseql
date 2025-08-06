"""Module for managing result type mappings in FraiseQL.

This module provides functionality to register and retrieve mappings between success and
error classes, enabling type-safe result handling in FraiseQL's GraphQL-to-SQL translation
pipeline. It supports robust error management for GraphQL resolvers and SQL query execution,
ensuring that success and error outcomes are explicitly paired in a type-safe manner.

Attributes.
----------
_registered_results : dict[type, type]
    A dictionary mapping success classes to their corresponding error classes, used to
    associate GraphQL resolver or SQL query outcomes with their error counterparts.
"""

# Maps success class to its associated error class
_registered_results: dict[type, type] = {}


def register_result(success_cls: type, error_cls: type) -> None:
    """Register a mapping between a success class and its corresponding error class.

    This function is used in FraiseQL to associate a success type (e.g., a GraphQL resolver
    result or SQL query output) with its corresponding error type, ensuring type-safe error
    handling during GraphQL-to-SQL translation.

    Parameters
    ----------
    success_cls : type
        The class representing a successful outcome, such as a GraphQL resolver result or
        SQL query response.
    error_cls : type
        The class representing the error outcome associated with the success class, used
        for handling errors in GraphQL resolvers or SQL execution.

    Examples.
    --------
    >>> class QuerySuccess:
    ...     pass
    >>> class QueryError:
    ...     pass
    >>> register_result(QuerySuccess, QueryError)
    >>> get_error_type_for(QuerySuccess)
    <class '__main__.QueryError'>
    """
    _registered_results[success_cls] = error_cls


def get_error_type_for(success_cls: type) -> type | None:
    """Retrieve the error class associated with a given success class.

    This function is used in FraiseQL to look up the error type for a given success type,
    facilitating type-safe error handling in GraphQL resolvers or SQL query execution.

    Parameters
    ----------
    success_cls : type
        The success class (e.g., a GraphQL resolver result or SQL query output) for which
        to retrieve the associated error class.

    Returns.
    -------
    type | None
        The error class associated with the success class, or None if no mapping exists.

    Examples.
    --------
    >>> class QuerySuccess:
    ...     pass
    >>> class QueryError:
    ...     pass
    >>> register_result(QuerySuccess, QueryError)
    >>> get_error_type_for(QuerySuccess)
    <class '__main__.QueryError'>
    >>> get_error_type_for(int)
    None
    """
    return _registered_results.get(success_cls)
