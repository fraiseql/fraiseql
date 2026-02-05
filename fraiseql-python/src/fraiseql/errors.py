"""FraiseQL errors and exceptions."""


class FederationValidationError(ValueError):
    """Exception raised when federation schema validation fails.

    This is raised when decorators detect invalid federation metadata,
    such as non-existent key fields, circular dependencies, or incorrect
    directive usage.
    """

    pass
