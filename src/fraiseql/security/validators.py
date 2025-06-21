"""Security validators for FraiseQL input validation.

Provides validation and sanitization for user inputs before SQL generation
to add an additional layer of protection against injection attacks.
"""

import logging
import re
from dataclasses import dataclass
from typing import Any, ClassVar, TypeVar

logger = logging.getLogger(__name__)

T = TypeVar("T")


@dataclass
class ValidationResult:
    """Result of input validation."""

    is_valid: bool
    errors: list[str]
    sanitized_value: Any
    warnings: list[str] = None

    def __post_init__(self):
        """Initialize warnings list if not provided."""
        if self.warnings is None:
            self.warnings = []


class InputValidator:
    """Validates and sanitizes input before SQL generation.

    This provides defense-in-depth by checking for suspicious patterns
    that might indicate injection attempts, even though the SQL layer
    uses parameterized queries.
    """

    # Patterns that might indicate injection attempts
    SUSPICIOUS_SQL_PATTERNS: ClassVar[list[tuple[str, str]]] = [
        # SQL comments
        (r"(--|#|/\*|\*/)", "SQL comment syntax detected"),
        # Common SQL injection keywords (case-insensitive)
        (
            r"\b(union\s+select|select\s+\*\s+from|drop\s+table|delete\s+from|"
            r"insert\s+into|update\s+.+\s+set)\b",
            "Suspicious SQL keyword pattern",
        ),
        # SQL functions that could be dangerous
        (
            r"\b(exec|execute|eval|system|cmd|xp_cmdshell)\b",
            "Potentially dangerous SQL function",
        ),
        # Stacked queries
        (r";\s*(select|insert|update|delete|drop|create|alter)", "Stacked query attempt"),
    ]

    # XSS patterns (for fields that might be displayed)
    XSS_PATTERNS: ClassVar[list[tuple[str, str]]] = [
        (r"<script[^>]*>.*?</script>", "Script tag detected"),
        (r"javascript:", "JavaScript URL detected"),
        (r"on\w+\s*=", "Event handler attribute detected"),
        (r"<iframe[^>]*>", "IFrame tag detected"),
    ]

    # Suspicious file path patterns
    PATH_TRAVERSAL_PATTERNS: ClassVar[list[tuple[str, str]]] = [
        (r"\.\./", "Path traversal attempt"),
        (r"\.\.\\", "Path traversal attempt (Windows)"),
        (r"/etc/passwd", "Suspicious system file access"),
        (r"c:\\windows", "Suspicious system path access"),
    ]

    # Maximum reasonable lengths for different field types
    MAX_LENGTHS: ClassVar[dict[str, int]] = {
        "name": 255,
        "email": 255,
        "description": 5000,
        "url": 2000,
        "default": 10000,
    }

    @classmethod
    def validate_field_value(
        cls,
        field_name: str,
        value: Any,
        field_type: str | None = None,
        allow_html: bool = False,
    ) -> ValidationResult:
        """Validate a single field value.

        Args:
            field_name: Name of the field being validated
            value: The value to validate
            field_type: Optional type hint for validation rules
            allow_html: Whether HTML content is allowed

        Returns:
            ValidationResult with validation status and sanitized value
        """
        errors = []
        warnings = []
        sanitized_value = value

        if value is None:
            return ValidationResult(is_valid=True, errors=[], sanitized_value=None)

        # String validation
        if isinstance(value, str):
            # Length validation
            max_length = cls.MAX_LENGTHS.get(field_name, cls.MAX_LENGTHS["default"])
            if len(value) > max_length:
                errors.append(f"Value too long for {field_name} (max {max_length} chars)")

            # Check for null bytes
            if "\x00" in value:
                errors.append(f"Null byte detected in {field_name}")
                sanitized_value = value.replace("\x00", "")

            # SQL injection patterns
            for pattern, message in cls.SUSPICIOUS_SQL_PATTERNS:
                if re.search(pattern, value, re.IGNORECASE):
                    warnings.append(f"{message} in {field_name}")

            # XSS patterns (only if HTML not allowed)
            if not allow_html:
                for pattern, message in cls.XSS_PATTERNS:
                    if re.search(pattern, value, re.IGNORECASE):
                        errors.append(f"{message} in {field_name}")

            # Path traversal (for file-related fields)
            if field_type == "path" or "path" in field_name.lower() or "file" in field_name.lower():
                for pattern, message in cls.PATH_TRAVERSAL_PATTERNS:
                    if re.search(pattern, value, re.IGNORECASE):
                        errors.append(f"{message} in {field_name}")

        # List validation
        elif isinstance(value, list):
            # Validate each item in the list
            for i, item in enumerate(value):
                item_result = cls.validate_field_value(
                    f"{field_name}[{i}]",
                    item,
                    field_type,
                    allow_html,
                )
                errors.extend(item_result.errors)
                warnings.extend(item_result.warnings)

        # Dict validation (for JSON fields)
        elif isinstance(value, dict):
            # Recursively validate dict values
            for key, val in value.items():
                # Validate the key itself
                key_result = cls.validate_field_value(
                    f"{field_name}.key",
                    key,
                    "key",
                    allow_html=False,
                )
                errors.extend(key_result.errors)

                # Validate the value
                val_result = cls.validate_field_value(
                    f"{field_name}.{key}",
                    val,
                    field_type,
                    allow_html,
                )
                errors.extend(val_result.errors)
                warnings.extend(val_result.warnings)

        # Numeric validation
        elif isinstance(value, int | float):
            # Check for suspicious numeric values
            if isinstance(value, float) and (value == float("inf") or value == float("-inf")):
                errors.append(f"Infinite value not allowed in {field_name}")
            elif isinstance(value, float) and str(value) == "nan":  # NaN check
                errors.append(f"NaN value not allowed in {field_name}")

        # Log warnings for monitoring
        if warnings:
            logger.warning(
                "Suspicious patterns detected in %s: %s",
                field_name,
                warnings,
                extra={"field": field_name, "warnings": warnings},
            )

        return ValidationResult(
            is_valid=len(errors) == 0,
            errors=errors,
            sanitized_value=sanitized_value,
            warnings=warnings,
        )

    @classmethod
    def validate_where_clause(cls, where_dict: dict) -> ValidationResult:
        """Validate an entire WHERE clause structure.

        Args:
            where_dict: Dictionary representation of WHERE conditions

        Returns:
            ValidationResult for the entire WHERE clause
        """
        all_errors = []
        all_warnings = []

        for field_name, conditions in where_dict.items():
            if isinstance(conditions, dict):
                for operator, value in conditions.items():
                    # Validate operator
                    if operator not in {
                        "eq",
                        "ne",
                        "gt",
                        "gte",
                        "lt",
                        "lte",
                        "in",
                        "nin",
                        "contains",
                        "starts_with",
                        "ends_with",
                        "is_null",
                    }:
                        all_errors.append(f"Invalid operator '{operator}' for field {field_name}")
                        continue

                    # Validate value based on operator
                    if operator in {"in", "nin"}:
                        if not isinstance(value, list):
                            all_errors.append(f"Operator '{operator}' requires a list value")
                    elif operator == "is_null" and not isinstance(value, bool):
                        all_errors.append("Operator 'is_null' requires a boolean value")

                    # Validate the actual value
                    result = cls.validate_field_value(field_name, value)
                    all_errors.extend(result.errors)
                    all_warnings.extend(result.warnings)

        return ValidationResult(
            is_valid=len(all_errors) == 0,
            errors=all_errors,
            sanitized_value=where_dict,  # WHERE structure remains unchanged
            warnings=all_warnings,
        )

    @classmethod
    def validate_mutation_input(
        cls,
        input_dict: dict,
        input_type: type | None = None,
    ) -> ValidationResult:
        """Validate mutation input data.

        Args:
            input_dict: Input data for mutation
            input_type: Optional type information for validation

        Returns:
            ValidationResult for the mutation input
        """
        all_errors = []
        all_warnings = []
        sanitized = {}

        for field_name, value in input_dict.items():
            # Get field type hint if available
            field_type = None
            if input_type and hasattr(input_type, "__annotations__"):
                field_type = input_type.__annotations__.get(field_name)

            # Special handling for email fields
            if field_name == "email" or (field_type and "email" in str(field_type).lower()):
                email_result = cls._validate_email(value)
                if not email_result.is_valid:
                    all_errors.extend(email_result.errors)
                sanitized[field_name] = email_result.sanitized_value
            else:
                # General validation
                result = cls.validate_field_value(
                    field_name,
                    value,
                    str(field_type) if field_type else None,
                )
                all_errors.extend(result.errors)
                all_warnings.extend(result.warnings)
                sanitized[field_name] = result.sanitized_value

        return ValidationResult(
            is_valid=len(all_errors) == 0,
            errors=all_errors,
            sanitized_value=sanitized,
            warnings=all_warnings,
        )

    @classmethod
    def _validate_email(cls, email: str) -> ValidationResult:
        """Validate email address format."""
        if not isinstance(email, str):
            return ValidationResult(
                is_valid=False,
                errors=["Email must be a string"],
                sanitized_value=email,
            )

        # Basic email regex (not comprehensive but catches common issues)
        email_pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"

        if not re.match(email_pattern, email):
            return ValidationResult(
                is_valid=False,
                errors=["Invalid email format"],
                sanitized_value=email,
            )

        # Additional checks
        if len(email) > 255:
            return ValidationResult(
                is_valid=False,
                errors=["Email address too long"],
                sanitized_value=email[:255],
            )

        # Check for suspicious patterns in email
        suspicious_email_patterns = [
            (r'[<>\'";]', "Suspicious characters in email"),
            (r"\.\.", "Consecutive dots in email"),
        ]

        warnings = []
        for pattern, message in suspicious_email_patterns:
            if re.search(pattern, email):
                warnings.append(message)

        return ValidationResult(
            is_valid=True,
            errors=[],
            sanitized_value=email.lower().strip(),
            warnings=warnings,
        )
