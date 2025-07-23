from typing import Set

class MutationErrorConfig:
    """Configuration for mutation error handling.
    
    This class defines how mutation results are parsed and categorized
    as success or error based on status messages and keywords.
    """

    success_keywords: Set[str]
    error_prefixes: Set[str]
    error_as_data_prefixes: Set[str]
    error_keywords: Set[str]

    def __init__(
        self,
        success_keywords: Set[str],
        error_prefixes: Set[str],
        error_as_data_prefixes: Set[str],
        error_keywords: Set[str],
    ) -> None:
        """Initialize mutation error configuration.
        
        Args:
            success_keywords: Keywords that indicate successful operations
            error_prefixes: Prefixes that indicate error responses
            error_as_data_prefixes: Prefixes for errors returned as data
            error_keywords: Keywords that indicate error conditions
        """

    def is_success(self, status: str) -> bool:
        """Check if a status indicates success."""

    def is_error(self, status: str) -> bool:
        """Check if a status indicates an error."""

    def should_return_as_data(self, status: str) -> bool:
        """Check if an error should be returned as data rather than thrown."""

# Pre-configured error configurations
ALWAYS_DATA_CONFIG: MutationErrorConfig
DEFAULT_ERROR_CONFIG: MutationErrorConfig
PRINTOPTIM_ERROR_CONFIG: MutationErrorConfig

__all__ = [
    "ALWAYS_DATA_CONFIG",
    "DEFAULT_ERROR_CONFIG",
    "PRINTOPTIM_ERROR_CONFIG",
    "MutationErrorConfig",
]
