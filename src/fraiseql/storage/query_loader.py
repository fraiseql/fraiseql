"""Query loader for APQ from .graphql files.

This module provides functionality to load GraphQL queries from .graphql
and .gql files for use with the APQ persisted queries system.
"""

import logging
import re
from pathlib import Path

logger = logging.getLogger(__name__)

# Pattern to match GraphQL operation definitions
# Matches: query Name { ... }, mutation Name { ... }, subscription Name { ... }
# Also handles anonymous operations and fragments
OPERATION_PATTERN = re.compile(
    r"(query|mutation|subscription|fragment)\s+\w*\s*(\([^)]*\))?\s*\{",
    re.IGNORECASE,
)


def load_queries_from_directory(directory: str | Path) -> list[str]:
    """Load all GraphQL queries from .graphql and .gql files in a directory.

    This function recursively scans the directory for .graphql and .gql files,
    reads their contents, and extracts individual operations.

    Args:
        directory: Path to directory containing .graphql/.gql files

    Returns:
        List of GraphQL query strings

    Raises:
        FileNotFoundError: If the directory does not exist
    """
    directory_path = Path(directory)

    if not directory_path.exists():
        raise FileNotFoundError(f"Directory not found: {directory}")

    queries: list[str] = []

    # Find all .graphql and .gql files recursively
    graphql_files = list(directory_path.rglob("*.graphql"))
    gql_files = list(directory_path.rglob("*.gql"))
    all_files = graphql_files + gql_files

    logger.debug(f"Found {len(all_files)} GraphQL files in {directory}")

    for file_path in all_files:
        try:
            content = file_path.read_text(encoding="utf-8")
            file_queries = _extract_operations(content)
            queries.extend(file_queries)
            logger.debug(f"Loaded {len(file_queries)} operations from {file_path}")
        except Exception as e:
            logger.warning(f"Failed to load queries from {file_path}: {e}")

    return queries


def _extract_operations(content: str) -> list[str]:
    """Extract individual GraphQL operations from file content.

    Handles files with multiple operations by splitting on operation keywords.

    Args:
        content: Raw GraphQL file content

    Returns:
        List of individual GraphQL operation strings
    """
    content = content.strip()
    if not content:
        return []

    # Find all operation start positions
    operations: list[str] = []
    matches = list(OPERATION_PATTERN.finditer(content))

    if not matches:
        # No operations found - might be a single anonymous query
        # or malformed content. Return as-is if non-empty.
        if content:
            return [content]
        return []

    # Extract each operation
    for i, match in enumerate(matches):
        start = match.start()
        # End is either start of next operation or end of content
        end = matches[i + 1].start() if i + 1 < len(matches) else len(content)

        operation = content[start:end].strip()
        if operation:
            operations.append(operation)

    return operations


def load_queries_from_file(file_path: str | Path) -> list[str]:
    """Load GraphQL queries from a single file.

    Args:
        file_path: Path to .graphql or .gql file

    Returns:
        List of GraphQL query strings from the file

    Raises:
        FileNotFoundError: If the file does not exist
    """
    path = Path(file_path)

    if not path.exists():
        raise FileNotFoundError(f"File not found: {file_path}")

    content = path.read_text(encoding="utf-8")
    return _extract_operations(content)
