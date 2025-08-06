"""Module for defining common GraphQL mutation result types and status mappings.

This module includes types for handling mutation results and a mapping of mutation
statuses to error codes.
"""

import uuid
from dataclasses import dataclass
from typing import Any

JSONType = dict[str, object]


@dataclass
class MutationResultRow:
    """A class to represent the result of a mutation operation in a database.

    This class encapsulates the details of a mutation operation, including the unique identifier
    of the mutated object, the fields that were updated, the status and message of the operation,
    and any additional metadata associated with the mutation.

    Attributes:
        id (UUID): The unique identifier of the mutated object.
        updated_fields (list[str]): A list of fields that were updated during the mutation.
        status (str): The status of the mutation operation.
        message (str): A message providing additional details about the mutation operation.
        object_data (dict[str, Any]): The data of the mutated object.
        extra_metadata (dict[str, Any]): Additional metadata associated with the mutation.
    """

    id: uuid.UUID
    status: str
    updated_fields: list[str]
    message: str
    object_data: dict[str, Any]
    extra_metadata: dict[str, Any]


MUTATION_STATUS_MAP = {
    # ✅ Success
    "ok": (None, 200),
    "updated": (None, 200),
    "deleted": (None, 200),
    # 🟡 No-op / neutral
    "noop": ("generic_noop", 422),
    "noop:already_exists": ("already_exists", 422),
    "noop:not_found": ("not_found", 404),
    # ❌ Business logic blockages
    "blocked:children": ("delete_blocked_child_units", 422),
    "blocked:allocations": ("delete_blocked_allocations", 422),
    "blocked:children_and_allocations": (
        "delete_blocked_current_allocations_and_children",
        422,
    ),
    # ❌ Validation
    "failed:validation": ("invalid_input", 422),
    # ❌ Technical
    "failed:exception": ("error_internal", 500),
}
