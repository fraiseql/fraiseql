"""Metadata parser for @fraiseql annotations in PostgreSQL comments.
This module parses YAML-formatted metadata from database object comments
to extract FraiseQL configuration for auto-discovery.
"""
import logging
from dataclasses import dataclass
import yaml

logger = logging.getLogger(__name__)

@dataclass
class TypeAnnotation:
    """Parsed @fraiseql:type annotation."""
    trinity: bool = False
    use_projection: bool = False
    description: str | None = None
    expose_fields: list[str] | None = None
    filter_config: dict | None = None

@dataclass
class MutationAnnotation:
    """Parsed @fraiseql:mutation annotation."""
    name: str
    success_type: str
    error_type: str
    description: str | None = None
    input_type: str | None = None
    context_params: list[str] | None = None  # NEW: Explicit context params

class MetadataParser:
    """Parse @fraiseql annotations from PostgreSQL comments."""

    def parse_type_annotation(self, comment: str | None) -> TypeAnnotation | None:
        """Parse @fraiseql:type annotation from a comment string."""
        if not comment:
            return None

        # Look for @fraiseql:type header
        if "@fraiseql:type" not in comment:
            return None

        try:
            # Extract YAML content after the header
            parts = comment.split("@fraiseql:type")
            yaml_content = parts[1].strip() if len(parts) > 1 else ""
            
            if not yaml_content:
                return TypeAnnotation()

            data = yaml.safe_load(yaml_content)
            if not isinstance(data, dict):
                return TypeAnnotation()

            return TypeAnnotation(
                trinity=data.get("trinity", False),
                use_projection=data.get("use_projection", False),
                description=data.get("description"),
                expose_fields=data.get("expose_fields"),
                filter_config=data.get("filter_config"),
            )
        except Exception as e:
            logger.warning(f"Failed to parse @fraiseql:type annotation: {e}")
            return None

    def parse_mutation_annotation(self, comment: str | None) -> MutationAnnotation | None:
        """Parse @fraiseql:mutation annotation from a comment string."""
        if not comment:
            return None

        # Look for @fraiseql:mutation header
        if "@fraiseql:mutation" not in comment:
            return None

        try:
            # Extract YAML content after the header
            parts = comment.split("@fraiseql:mutation")
            yaml_content = parts[1].strip() if len(parts) > 1 else ""
            
            if not yaml_content:
                return None

            data = yaml.safe_load(yaml_content)
            if not isinstance(data, dict):
                return None

            # Validate required fields
            if "name" not in data or "success_type" not in data or "error_type" not in data:
                logger.warning("Missing required fields in @fraiseql:mutation annotation")
                return None

            return MutationAnnotation(
                name=data["name"],
                success_type=data["success_type"],
                error_type=data["error_type"],
                description=data.get("description"),
                input_type=data.get("input_type"),
                context_params=data.get("context_params"),
            )
        except Exception as e:
            logger.warning(f"Failed to parse @fraiseql:mutation annotation: {e}")
            return None
