"""Metadata parser for @fraiseql annotations in PostgreSQL comments.

This module parses YAML-formatted metadata from database object comments
to extract FraiseQL configuration for auto-discovery.
"""

from dataclasses import dataclass
from typing import Optional

import yaml


@dataclass
class TypeAnnotation:
    """Parsed @fraiseql:type annotation."""

    trinity: bool = False
    use_projection: bool = False
    description: Optional[str] = None
    expose_fields: Optional[list[str]] = None
    filter_config: Optional[dict] = None


@dataclass
class MutationAnnotation:
    """Parsed @fraiseql:mutation annotation."""

    input_schema: dict[str, dict]
    success_type: str
    failure_type: str
    description: Optional[str] = None
    permissions: Optional[list[str]] = None


class MetadataParser:
    """Parse @fraiseql annotations from PostgreSQL comments."""

    ANNOTATION_MARKER = "@fraiseql:"

    def parse_type_annotation(self, comment: Optional[str]) -> Optional[TypeAnnotation]:
        """Parse @fraiseql:type annotation from view comment.

        Format:
            @fraiseql:type
            trinity: true
            description: User account
            expose_fields:
              - id
              - name
              - email

        Returns:
            TypeAnnotation if valid, None otherwise
        """
        if not comment or self.ANNOTATION_MARKER not in comment:
            return None

        try:
            # Extract YAML content after marker
            marker = "@fraiseql:type"
            if marker not in comment:
                return None

            yaml_start = comment.index(marker) + len(marker)
            yaml_content = comment[yaml_start:].strip()

            # Handle multi-line YAML
            # Stop at next @fraiseql: marker or end of comment
            if self.ANNOTATION_MARKER in yaml_content:
                next_marker = yaml_content.index(self.ANNOTATION_MARKER)
                yaml_content = yaml_content[:next_marker]

            # Parse YAML
            data = yaml.safe_load(yaml_content) or {}

            return TypeAnnotation(
                trinity=data.get("trinity", False),
                use_projection=data.get("use_projection", False),
                description=data.get("description"),
                expose_fields=data.get("expose_fields"),
                filter_config=data.get("filters"),
            )

        except (yaml.YAMLError, ValueError) as e:
            # Log warning but don't fail
            import logging

            logger = logging.getLogger(__name__)
            logger.warning(f"Failed to parse @fraiseql:type: {e}")
            return None

    def parse_mutation_annotation(self, comment: Optional[str]) -> Optional[MutationAnnotation]:
        """Parse @fraiseql:mutation annotation."""
        if not comment or self.ANNOTATION_MARKER not in comment:
            return None

        try:
            # Extract YAML content after marker
            marker = "@fraiseql:mutation"
            if marker not in comment:
                return None

            yaml_start = comment.index(marker) + len(marker)
            yaml_content = comment[yaml_start:].strip()

            # Handle multi-line YAML
            if self.ANNOTATION_MARKER in yaml_content:
                next_marker = yaml_content.index(self.ANNOTATION_MARKER)
                yaml_content = yaml_content[:next_marker]

            # Parse YAML
            data = yaml.safe_load(yaml_content) or {}

            # Validate required fields
            if (
                "input_schema" not in data
                or "success_type" not in data
                or "failure_type" not in data
            ):
                return None

            return MutationAnnotation(
                input_schema=data["input_schema"],
                success_type=data["success_type"],
                failure_type=data["failure_type"],
                description=data.get("description"),
                permissions=data.get("permissions"),
            )

        except (yaml.YAMLError, ValueError, KeyError) as e:
            import logging

            logger = logging.getLogger(__name__)
            logger.warning(f"Failed to parse @fraiseql:mutation: {e}")
            return None
