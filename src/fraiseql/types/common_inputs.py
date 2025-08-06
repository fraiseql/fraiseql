"""Module for defining common GraphQL input types used across the application.

This module includes input types for handling various operations such as deletion.
"""

import uuid

from fraiseql.types.fraise_input import fraise_input


@fraise_input
class DeletionInput:
    """Input type for handling deletion operations in GraphQL.

    Attributes:
        id (uuid.UUID): The unique identifier of the entity to be deleted.
        hard_delete (bool): Flag indicating whether to perform a hard delete.
            If set to True, the entity will be permanently deleted.
            Defaults to False, implying a soft delete.
    """

    id: uuid.UUID
    hard_delete: bool = False
