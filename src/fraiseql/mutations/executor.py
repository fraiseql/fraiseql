"""Executor for running FraiseQL SQL mutations.

This module provides utilities to execute SQL-based mutations defined via JSON inputs,
parse their result payloads, and return structured mutation results (success or error).
"""

import traceback
from dataclasses import is_dataclass
from typing import Any, Protocol, TypeVar, runtime_checkable

import psycopg

from fraiseql.db import FraiseQLRepository
from fraiseql.types.errors import Error

R = TypeVar("R")  # Result type
E = TypeVar("E")  # Error type


@runtime_checkable
class MutationResult(Protocol):
    """Protocol for mutation result classes."""

    status: str
    message: str


@runtime_checkable
class MutationError(Protocol):
    """Protocol for mutation error classes."""

    status: str
    message: str
    errors: list[Error]


@runtime_checkable
class MutationSuccess(Protocol):
    """Protocol for mutation success classes."""

    status: str
    message: str
    data: Any


def parse_mutation_result(
    result_row: dict[str, Any],
    result_cls: type[R],
    error_cls: type[E],
    custom_status_map: dict[str, tuple[str, int]],
) -> R | E:
    """Parses the raw dictionary result from a SQL mutation into a typed dataclass."""
    status = result_row.get("status", "unknown_status")
    message = result_row.get("message", "No message provided.")
    errors_data = result_row.get("errors")

    if status in custom_status_map:
        mapped_status, mapped_code = custom_status_map[status]
        if mapped_status == "success":
            # Build kwargs dynamically based on what the class accepts
            kwargs: dict[str, Any] = {
                "status": status,
                "message": message,
            }
            if hasattr(result_cls, "__annotations__") and "data" in result_cls.__annotations__:
                kwargs["data"] = result_row.get("data")
            return result_cls(**kwargs)  # type: ignore[call-arg]

        errors = []
        if isinstance(errors_data, list):
            errors.extend(
                [
                    Error(
                        message=err.get("message", "An error occurred."),
                        code=err.get("code", mapped_code),
                        identifier=err.get("identifier", mapped_status),
                    )
                    for err in errors_data
                ],
            )
        return error_cls(status=status, message=message, errors=errors)  # type: ignore[call-arg]

    errors = []
    if isinstance(errors_data, list):
        errors.extend(
            [
                Error(
                    message=err.get("message", "An error occurred."),
                    code=err.get("code", 500),
                    identifier=err.get("identifier", "unknown_error"),
                )
                for err in errors_data
            ],
        )
    return error_cls(
        status="failed:unknown_status",
        message=message,
        errors=errors,
    )  # type: ignore[call-arg]


async def run_fraiseql_mutation(
    input_payload: object,
    sql_function_name: str,
    result_cls: type[R],
    error_cls: type[E],
    status_map: dict[str, tuple[str, int]],
    fallback_error_identifier: str,
    repository: FraiseQLRepository,
    context: dict[str, Any],
    noop_message: str | None = None,
) -> R | E:
    """Runs a SQL mutation safely and parses its result for FraiseQL."""
    from fraiseql.mutations.sql_generator import (
        generate_insert_json_call,
    )  # Local import to avoid circular import

    if not is_dataclass(input_payload):
        msg = "Input payload for mutation must be a dataclass instance."
        raise TypeError(msg)

    try:
        mutation_query = generate_insert_json_call(
            input_object=input_payload,
            context=context,
            sql_function_name=sql_function_name,
        )

        input_json = mutation_query.params["input_json"]
        if not getattr(input_json, "obj", None):
            return error_cls(
                status="noop",
                message=noop_message or "No fields to update.",
                errors=[
                    Error(
                        message=noop_message or "No fields to update.",
                        code=422,
                        identifier="generic_noop",
                    ),
                ],
            )  # type: ignore[call-arg]

        result = await repository.run(mutation_query)

        if not result or not result[0]:
            return error_cls(
                status="failed:no_result",
                message="No result returned from mutation.",
                errors=[
                    Error(
                        message="No result returned from mutation.",
                        code=500,
                        identifier="no_mutation_result",
                    ),
                ],
            )  # type: ignore[call-arg]

        return parse_mutation_result(
            result_row=result[0],
            result_cls=result_cls,
            error_cls=error_cls,
            custom_status_map=status_map,
        )

    except psycopg.Error as e:
        stack_trace = traceback.format_exc()
        return error_cls(
            status="failed:exception",
            message="Unhandled database exception occurred.",
            errors=[
                Error(
                    message=f"{type(e).__name__}: {e}\n{stack_trace}",
                    code=500,
                    identifier=fallback_error_identifier,
                ),
            ],
        )  # type: ignore[call-arg]

    except (BaseException, Exception) as e:  # Safer than bare `except Exception`
        stack_trace = traceback.format_exc()
        return error_cls(
            status="failed:exception",
            message="An unexpected error occurred.",
            errors=[
                Error(
                    message=f"{type(e).__name__}: {e}\n{stack_trace}",
                    code=500,
                    identifier=fallback_error_identifier,
                ),
            ],
        )  # type: ignore[call-arg]
