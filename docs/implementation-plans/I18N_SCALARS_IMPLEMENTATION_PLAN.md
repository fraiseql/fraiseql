# Implementation Plan: i18n Scalar Types (LanguageCode, LocaleCode, Timezone)

**Issue**: [#127 - Add i18n Scalar Types: LanguageCode, LocaleCode, Timezone](https://github.com/fraiseql/fraiseql/issues/127)

**Status**: Ready for Implementation
**Complexity**: Simple
**Estimated Time**: 2-3 hours
**Priority**: Medium

---

## Executive Summary

Add three internationalization (i18n) scalar types to FraiseQL's existing scalar library:

1. **`LanguageCode`** - ISO 639-1 two-letter language codes (en, fr, de, etc.)
2. **`LocaleCode`** - BCP 47 locale format (en-US, fr-FR, etc.)
3. **`Timezone`** - IANA timezone identifiers (America/New_York, Europe/Paris, etc.)

These scalars follow the **exact same pattern** as existing network scalars (Hostname, IpAddress, MacAddress, Port, CIDR) and will integrate seamlessly into FraiseQL's type system.

---

## Architecture Overview

### File Structure
```
src/fraiseql/types/scalars/
├── language_code.py          # NEW: ISO 639-1 validation
├── locale_code.py            # NEW: BCP 47 validation
├── timezone.py               # NEW: IANA timezone validation
└── __init__.py               # UPDATE: Export new scalars

src/fraiseql/types/__init__.py # UPDATE: Public API exports

tests/unit/core/type_system/
├── test_language_code_scalar.py  # NEW: LanguageCode tests
├── test_locale_code_scalar.py    # NEW: LocaleCode tests
└── test_timezone_scalar.py       # NEW: Timezone tests
```

### Pattern Consistency

All three scalars follow the **exact pattern** established by existing scalars:

```python
# Pattern template (based on hostname.py, port.py, etc.)
"""[Scalar name] scalar type for [validation purpose]."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# Validation regex
_VALIDATION_REGEX = re.compile(r"^[pattern]$")

def serialize_[scalar](value: Any) -> str | None:
    """Serialize [scalar] to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _VALIDATION_REGEX.match(value_str):
        raise GraphQLError(f"Invalid [scalar]: {value}. [Requirements]")

    return value_str

def parse_[scalar]_value(value: Any) -> str:
    """Parse [scalar] from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"[Scalar] must be a string, got {type(value).__name__}")

    if not _VALIDATION_REGEX.match(value):
        raise GraphQLError(f"Invalid [scalar]: {value}. [Requirements]")

    return value

def parse_[scalar]_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse [scalar] from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("[Scalar] must be a string")

    return parse_[scalar]_value(ast.value)

[Scalar]Scalar = GraphQLScalarType(
    name="[ScalarName]",
    description="[Description]",
    serialize=serialize_[scalar],
    parse_value=parse_[scalar]_value,
    parse_literal=parse_[scalar]_literal,
)

class [Scalar]Field(str, ScalarMarker):
    """[Documentation]"""

    __slots__ = ()

    def __new__(cls, value: str) -> "[Scalar]Field":
        """Create a new [Scalar]Field instance with validation."""
        if not _VALIDATION_REGEX.match(value):
            raise ValueError(f"Invalid [scalar]: {value}. [Requirements]")
        return super().__new__(cls, value)
```

---

## Detailed Implementation Specification

### 1. LanguageCodeScalar (`src/fraiseql/types/scalars/language_code.py`)

**Purpose**: Validate ISO 639-1 two-letter language codes

**Validation Rules**:
- Exactly 2 lowercase letters (a-z)
- Case-insensitive input, normalized to lowercase
- Examples: `en`, `fr`, `de`, `es`, `ja`, `zh`, `ar`, `ru`

**Regex Pattern**: `^[a-z]{2}$`

**Implementation**:

```python
"""Language code scalar type for ISO 639-1 validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISO 639-1: Two-letter language codes (en, fr, de, es, ja, etc.)
_LANGUAGE_CODE_REGEX = re.compile(r"^[a-z]{2}$")


def serialize_language_code(value: Any) -> str | None:
    """Serialize language code to string."""
    if value is None:
        return None

    value_str = str(value).lower()

    if not _LANGUAGE_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid language code: {value}. Must be ISO 639-1 two-letter code (e.g., 'en', 'fr', 'de')"
        )

    return value_str


def parse_language_code_value(value: Any) -> str:
    """Parse language code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Language code must be a string, got {type(value).__name__}")

    value_lower = value.lower()

    if not _LANGUAGE_CODE_REGEX.match(value_lower):
        raise GraphQLError(
            f"Invalid language code: {value}. Must be ISO 639-1 two-letter code (e.g., 'en', 'fr', 'de')"
        )

    return value_lower


def parse_language_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse language code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Language code must be a string")

    return parse_language_code_value(ast.value)


LanguageCodeScalar = GraphQLScalarType(
    name="LanguageCode",
    description=(
        "ISO 639-1 two-letter language code. "
        "Valid codes: en, fr, de, es, ja, zh, etc. "
        "See: https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes"
    ),
    serialize=serialize_language_code,
    parse_value=parse_language_code_value,
    parse_literal=parse_language_code_literal,
)


class LanguageCodeField(str, ScalarMarker):
    """ISO 639-1 two-letter language code.

    This scalar validates that the language code follows ISO 639-1 standard:
    - Exactly 2 lowercase letters
    - Valid codes: en, fr, de, es, ja, zh, ar, etc.
    - Case-insensitive (normalized to lowercase)

    Example:
        >>> from fraiseql.types import LanguageCode
        >>>
        >>> @fraiseql.input
        ... class UserPreferences:
        ...     language: LanguageCode
        ...     fallback_language: LanguageCode | None
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LanguageCodeField":
        """Create a new LanguageCodeField instance with validation."""
        value_lower = value.lower()
        if not _LANGUAGE_CODE_REGEX.match(value_lower):
            raise ValueError(
                f"Invalid language code: {value}. Must be ISO 639-1 two-letter code (e.g., 'en', 'fr', 'de')"
            )
        return super().__new__(cls, value_lower)
```

---

### 2. LocaleCodeScalar (`src/fraiseql/types/scalars/locale_code.py`)

**Purpose**: Validate BCP 47 locale codes (language-REGION format)

**Validation Rules**:
- Format: `language` OR `language-REGION`
- Language: 2 lowercase letters (ISO 639-1)
- Region: 2 uppercase letters (ISO 3166-1 alpha-2)
- Case-sensitive (must match exact format)
- Examples: `en-US`, `fr-FR`, `de-DE`, `en`, `fr`

**Regex Pattern**: `^[a-z]{2}(-[A-Z]{2})?$`

**Implementation**:

```python
"""Locale code scalar type for BCP 47 validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# BCP 47: language-REGION format (en-US, fr-FR, de-DE, etc.)
# Also supports language-only (en, fr) for flexibility
_LOCALE_CODE_REGEX = re.compile(r"^[a-z]{2}(-[A-Z]{2})?$")


def serialize_locale_code(value: Any) -> str | None:
    """Serialize locale code to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _LOCALE_CODE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid locale code: {value}. Must be BCP 47 format (e.g., 'en-US', 'fr-FR', 'de-DE')"
        )

    return value_str


def parse_locale_code_value(value: Any) -> str:
    """Parse locale code from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Locale code must be a string, got {type(value).__name__}")

    if not _LOCALE_CODE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid locale code: {value}. Must be BCP 47 format (e.g., 'en-US', 'fr-FR', 'de-DE')"
        )

    return value


def parse_locale_code_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse locale code from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Locale code must be a string")

    return parse_locale_code_value(ast.value)


LocaleCodeScalar = GraphQLScalarType(
    name="LocaleCode",
    description=(
        "BCP 47 locale code (language-REGION format). "
        "Format: lowercase language + hyphen + uppercase region. "
        "Examples: en-US, fr-FR, de-DE, es-ES, ja-JP, zh-CN. "
        "See: https://tools.ietf.org/html/bcp47"
    ),
    serialize=serialize_locale_code,
    parse_value=parse_locale_code_value,
    parse_literal=parse_locale_code_literal,
)


class LocaleCodeField(str, ScalarMarker):
    """BCP 47 locale code for regional/cultural formatting.

    This scalar validates locale codes following BCP 47 standard:
    - Format: language-REGION (e.g., en-US, fr-FR)
    - Language: 2 lowercase letters (ISO 639-1)
    - Region: 2 uppercase letters (ISO 3166-1 alpha-2)
    - Language-only also accepted (e.g., en, fr)

    Example:
        >>> from fraiseql.types import LocaleCode
        >>>
        >>> @fraiseql.type
        ... class User:
        ...     locale: LocaleCode  # for date/number formatting
        ...     language: LanguageCode  # for content translation
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "LocaleCodeField":
        """Create a new LocaleCodeField instance with validation."""
        if not _LOCALE_CODE_REGEX.match(value):
            raise ValueError(
                f"Invalid locale code: {value}. Must be BCP 47 format (e.g., 'en-US', 'fr-FR', 'de-DE')"
            )
        return super().__new__(cls, value)
```

---

### 3. TimezoneScalar (`src/fraiseql/types/scalars/timezone.py`)

**Purpose**: Validate IANA timezone database identifiers

**Validation Rules**:
- Format: `Region/City` or `Region/City/Locality`
- Region/City/Locality must start with uppercase letter
- Can contain letters (a-z, A-Z) and underscores
- Case-sensitive (exact capitalization required)
- Examples: `America/New_York`, `Europe/Paris`, `Asia/Tokyo`, `America/Argentina/Buenos_Aires`

**Regex Pattern**: `^[A-Z][a-zA-Z_]+(/[A-Z][a-zA-Z_]+){1,2}$`

**Implementation**:

```python
"""Timezone scalar type for IANA timezone identifier validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# IANA timezone database format: Region/City or Region/City/Locality
# Examples: America/New_York, Europe/Paris, Asia/Tokyo, Pacific/Auckland
_TIMEZONE_REGEX = re.compile(r"^[A-Z][a-zA-Z_]+(/[A-Z][a-zA-Z_]+){1,2}$")


def serialize_timezone(value: Any) -> str | None:
    """Serialize timezone to string."""
    if value is None:
        return None

    value_str = str(value)

    if not _TIMEZONE_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid timezone: {value}. Must be IANA timezone identifier (e.g., 'America/New_York', 'Europe/Paris')"
        )

    return value_str


def parse_timezone_value(value: Any) -> str:
    """Parse timezone from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"Timezone must be a string, got {type(value).__name__}")

    if not _TIMEZONE_REGEX.match(value):
        raise GraphQLError(
            f"Invalid timezone: {value}. Must be IANA timezone identifier (e.g., 'America/New_York', 'Europe/Paris')"
        )

    return value


def parse_timezone_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse timezone from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("Timezone must be a string")

    return parse_timezone_value(ast.value)


TimezoneScalar = GraphQLScalarType(
    name="Timezone",
    description=(
        "IANA timezone database identifier. "
        "Format: Region/City or Region/City/Locality. "
        "Examples: America/New_York, Europe/Paris, Asia/Tokyo, Pacific/Auckland. "
        "See: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones"
    ),
    serialize=serialize_timezone,
    parse_value=parse_timezone_value,
    parse_literal=parse_timezone_literal,
)


class TimezoneField(str, ScalarMarker):
    """IANA timezone identifier for timezone-aware applications.

    This scalar validates timezone identifiers from the IANA timezone database:
    - Format: Region/City (e.g., America/New_York, Europe/Paris)
    - Case-sensitive (standard capitalization required)
    - Handles daylight saving time transitions correctly
    - Better than UTC offsets (which don't handle DST)

    Example:
        >>> from fraiseql.types import Timezone
        >>>
        >>> @fraiseql.type
        ... class User:
        ...     timezone: Timezone
        ...
        >>> @fraiseql.input
        ... class ScheduleEvent:
        ...     start_time: DateTime
        ...     timezone: Timezone  # for display in user's local time
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "TimezoneField":
        """Create a new TimezoneField instance with validation."""
        if not _TIMEZONE_REGEX.match(value):
            raise ValueError(
                f"Invalid timezone: {value}. Must be IANA timezone identifier (e.g., 'America/New_York', 'Europe/Paris')"
            )
        return super().__new__(cls, value)
```

---

### 4. Update Scalar Exports (`src/fraiseql/types/scalars/__init__.py`)

**Changes**:
- Import the three new scalar modules
- Add to `__all__` export list
- Update module docstring

**Implementation**:

```python
# At the top with other imports (alphabetically ordered)
from .language_code import LanguageCodeScalar
from .locale_code import LocaleCodeScalar
from .timezone import TimezoneScalar

# In __all__ (alphabetically ordered)
__all__ = [
    "CIDRScalar",
    "CoordinateScalar",
    "DateRangeScalar",
    "DateScalar",
    "DateTimeScalar",
    "HostnameScalar",
    "IpAddressScalar",
    "JSONScalar",
    "LanguageCodeScalar",      # NEW
    "LocaleCodeScalar",        # NEW
    "LTreeScalar",
    "MacAddressScalar",
    "PortScalar",
    "SubnetMaskScalar",
    "TimezoneScalar",          # NEW
    "UUIDScalar",
]
```

**Update module docstring**:

```python
"""Custom GraphQL scalar types for FraiseQL.

This module exposes reusable scalar implementations that extend GraphQL's
capabilities to support domain-specific values such as IP addresses, UUIDs,
date ranges, JSON objects, and more.

Each export is a `GraphQLScalarType` used directly in schema definitions.

Exports:
- CIDRScalar: CIDR notation for IP network ranges.
- CoordinateScalar: Geographic coordinates (latitude, longitude).
- DateRangeScalar: PostgreSQL daterange values.
- DateScalar: ISO 8601 calendar date.
- DateTimeScalar: ISO 8601 datetime with timezone awareness.
- HostnameScalar: DNS hostnames (RFC 1123 compliant).
- IpAddressScalar: IPv4 and IPv6 addresses as strings.
- JSONScalar: Arbitrary JSON-serializable values.
- LanguageCodeScalar: ISO 639-1 two-letter language codes.
- LocaleCodeScalar: BCP 47 locale codes (language-REGION format).
- LTreeScalar: PostgreSQL ltree path type.
- MacAddressScalar: Hardware MAC addresses.
- PortScalar: Network port number (1-65535).
- SubnetMaskScalar: CIDR-style subnet masks.
- TimezoneScalar: IANA timezone database identifiers.
- UUIDScalar: RFC 4122 UUID values.
"""
```

---

### 5. Update Public API (`src/fraiseql/types/__init__.py`)

**Changes**:
- Import the three new Field classes with public aliases
- Add to `__all__` export list

**Implementation**:

```python
# Add with other scalar imports (alphabetically ordered)
from .scalars.language_code import LanguageCodeField as LanguageCode
from .scalars.locale_code import LocaleCodeField as LocaleCode
from .scalars.timezone import TimezoneField as Timezone

# Add to __all__ (alphabetically ordered)
__all__ = [
    "CIDR",
    "JSON",
    "UUID",
    "Connection",
    "Coordinate",
    "Date",
    "DateRange",
    "DateRangeValidatable",
    "DateRangeValidationMixin",
    "DateTime",
    "Edge",
    "EmailAddress",
    "Error",
    "Hostname",
    "IpAddress",
    "LanguageCode",             # NEW
    "LocaleCode",               # NEW
    "LTree",
    "MacAddress",
    "PageInfo",
    "PaginatedResponse",
    "Port",
    "Timezone",                 # NEW
    "convert_scalar_to_graphql",
    "create_connection",
    "date_range_validator",
    "fraise_input",
    "fraise_type",
    "get_date_range_validation_errors",
    "input",
    "type",
    "validate_date_range",
]
```

---

## Test Implementation

Create three comprehensive test files following the **exact pattern** of `test_hostname_scalar.py`.

### 6. LanguageCode Tests (`tests/unit/core/type_system/test_language_code_scalar.py`)

```python
"""Tests for LanguageCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.language_code import (
    LanguageCodeField,
    parse_language_code_literal,
    parse_language_code_value,
    serialize_language_code,
)


@pytest.mark.unit
class TestLanguageCodeSerialization:
    """Test language code serialization."""

    def test_serialize_valid_language_codes(self):
        """Test serializing valid ISO 639-1 language codes."""
        assert serialize_language_code("en") == "en"
        assert serialize_language_code("fr") == "fr"
        assert serialize_language_code("de") == "de"
        assert serialize_language_code("es") == "es"
        assert serialize_language_code("ja") == "ja"
        assert serialize_language_code("zh") == "zh"
        assert serialize_language_code("ar") == "ar"
        assert serialize_language_code("ru") == "ru"
        assert serialize_language_code("pt") == "pt"
        assert serialize_language_code("it") == "it"

    def test_serialize_case_insensitive(self):
        """Test language code serialization is case-insensitive (normalized to lowercase)."""
        assert serialize_language_code("EN") == "en"
        assert serialize_language_code("Fr") == "fr"
        assert serialize_language_code("DE") == "de"
        assert serialize_language_code("eS") == "es"

    def test_serialize_none(self):
        """Test serializing None returns None."""
        assert serialize_language_code(None) is None

    def test_serialize_invalid_language_code(self):
        """Test serializing invalid language codes raises error."""
        # Too long
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("eng")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("english")

        # Too short
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("e")

        # Contains numbers
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("e1")

        # Contains special characters
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("en-US")  # Use LocaleCode instead

        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("en_US")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("en-")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid language code"):
            serialize_language_code("")


class TestLanguageCodeParsing:
    """Test language code parsing from variables."""

    def test_parse_valid_language_code(self):
        """Test parsing valid language codes."""
        assert parse_language_code_value("en") == "en"
        assert parse_language_code_value("FR") == "fr"
        assert parse_language_code_value("De") == "de"

    def test_parse_invalid_language_code(self):
        """Test parsing invalid language codes raises error."""
        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_value("eng")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_value("e")

        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_value("en-US")

    def test_parse_invalid_type(self):
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_value(123)

        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_value(None)

        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_value(["en"])


class TestLanguageCodeField:
    """Test LanguageCodeField class."""

    def test_create_valid_language_code_field(self):
        """Test creating LanguageCodeField with valid values."""
        lang = LanguageCodeField("en")
        assert lang == "en"
        assert isinstance(lang, str)

        # Case normalization
        lang = LanguageCodeField("FR")
        assert lang == "fr"

    def test_create_invalid_language_code_field(self):
        """Test creating LanguageCodeField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid language code"):
            LanguageCodeField("eng")

        with pytest.raises(ValueError, match="Invalid language code"):
            LanguageCodeField("e")

        with pytest.raises(ValueError, match="Invalid language code"):
            LanguageCodeField("en-US")


class TestLanguageCodeLiteralParsing:
    """Test parsing language code from GraphQL literals."""

    def test_parse_valid_literal(self):
        """Test parsing valid language code literals."""
        assert parse_language_code_literal(StringValueNode(value="en")) == "en"
        assert parse_language_code_literal(StringValueNode(value="FR")) == "fr"
        assert parse_language_code_literal(StringValueNode(value="De")) == "de"

    def test_parse_invalid_literal_format(self):
        """Test parsing invalid language code format literals."""
        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_literal(StringValueNode(value="eng"))

        with pytest.raises(GraphQLError, match="Invalid language code"):
            parse_language_code_literal(StringValueNode(value="en-US"))

    def test_parse_non_string_literal(self):
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Language code must be a string"):
            parse_language_code_literal(IntValueNode(value="123"))
```

---

### 7. LocaleCode Tests (`tests/unit/core/type_system/test_locale_code_scalar.py`)

```python
"""Tests for LocaleCode scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.locale_code import (
    LocaleCodeField,
    parse_locale_code_literal,
    parse_locale_code_value,
    serialize_locale_code,
)


@pytest.mark.unit
class TestLocaleCodeSerialization:
    """Test locale code serialization."""

    def test_serialize_valid_locale_codes(self):
        """Test serializing valid BCP 47 locale codes."""
        assert serialize_locale_code("en-US") == "en-US"
        assert serialize_locale_code("fr-FR") == "fr-FR"
        assert serialize_locale_code("de-DE") == "de-DE"
        assert serialize_locale_code("es-ES") == "es-ES"
        assert serialize_locale_code("ja-JP") == "ja-JP"
        assert serialize_locale_code("zh-CN") == "zh-CN"
        assert serialize_locale_code("pt-BR") == "pt-BR"
        assert serialize_locale_code("en-GB") == "en-GB"

    def test_serialize_language_only(self):
        """Test serializing language-only codes."""
        assert serialize_locale_code("en") == "en"
        assert serialize_locale_code("fr") == "fr"
        assert serialize_locale_code("de") == "de"

    def test_serialize_none(self):
        """Test serializing None returns None."""
        assert serialize_locale_code(None) is None

    def test_serialize_invalid_locale_code(self):
        """Test serializing invalid locale codes raises error."""
        # Wrong case (must be lowercase-UPPERCASE)
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("EN-us")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("EN-US")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en-us")

        # Underscore instead of hyphen
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en_US")

        # Region too long
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en-USA")

        # Language too long
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("eng-US")

        # Invalid characters
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("en-U1")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            serialize_locale_code("")


class TestLocaleCodeParsing:
    """Test locale code parsing from variables."""

    def test_parse_valid_locale_code(self):
        """Test parsing valid locale codes."""
        assert parse_locale_code_value("en-US") == "en-US"
        assert parse_locale_code_value("fr-FR") == "fr-FR"
        assert parse_locale_code_value("en") == "en"

    def test_parse_invalid_locale_code(self):
        """Test parsing invalid locale codes raises error."""
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_value("EN-us")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_value("en_US")

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_value("en-USA")

    def test_parse_invalid_type(self):
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_value(123)

        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_value(None)

        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_value(["en-US"])


class TestLocaleCodeField:
    """Test LocaleCodeField class."""

    def test_create_valid_locale_code_field(self):
        """Test creating LocaleCodeField with valid values."""
        locale = LocaleCodeField("en-US")
        assert locale == "en-US"
        assert isinstance(locale, str)

        locale = LocaleCodeField("fr")
        assert locale == "fr"

    def test_create_invalid_locale_code_field(self):
        """Test creating LocaleCodeField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid locale code"):
            LocaleCodeField("EN-us")

        with pytest.raises(ValueError, match="Invalid locale code"):
            LocaleCodeField("en_US")

        with pytest.raises(ValueError, match="Invalid locale code"):
            LocaleCodeField("en-USA")


class TestLocaleCodeLiteralParsing:
    """Test parsing locale code from GraphQL literals."""

    def test_parse_valid_literal(self):
        """Test parsing valid locale code literals."""
        assert parse_locale_code_literal(StringValueNode(value="en-US")) == "en-US"
        assert parse_locale_code_literal(StringValueNode(value="fr-FR")) == "fr-FR"
        assert parse_locale_code_literal(StringValueNode(value="en")) == "en"

    def test_parse_invalid_literal_format(self):
        """Test parsing invalid locale code format literals."""
        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_literal(StringValueNode(value="EN-us"))

        with pytest.raises(GraphQLError, match="Invalid locale code"):
            parse_locale_code_literal(StringValueNode(value="en_US"))

    def test_parse_non_string_literal(self):
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Locale code must be a string"):
            parse_locale_code_literal(IntValueNode(value="123"))
```

---

### 8. Timezone Tests (`tests/unit/core/type_system/test_timezone_scalar.py`)

```python
"""Tests for Timezone scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.timezone import (
    TimezoneField,
    parse_timezone_literal,
    parse_timezone_value,
    serialize_timezone,
)


@pytest.mark.unit
class TestTimezoneSerialization:
    """Test timezone serialization."""

    def test_serialize_valid_timezones(self):
        """Test serializing valid IANA timezone identifiers."""
        assert serialize_timezone("America/New_York") == "America/New_York"
        assert serialize_timezone("Europe/Paris") == "Europe/Paris"
        assert serialize_timezone("Asia/Tokyo") == "Asia/Tokyo"
        assert serialize_timezone("Pacific/Auckland") == "Pacific/Auckland"
        assert serialize_timezone("America/Los_Angeles") == "America/Los_Angeles"
        assert serialize_timezone("Europe/London") == "Europe/London"
        assert serialize_timezone("Australia/Sydney") == "Australia/Sydney"

    def test_serialize_three_part_timezones(self):
        """Test serializing timezones with three parts (Region/City/Locality)."""
        assert serialize_timezone("America/Argentina/Buenos_Aires") == "America/Argentina/Buenos_Aires"
        assert serialize_timezone("America/Indiana/Indianapolis") == "America/Indiana/Indianapolis"
        assert serialize_timezone("America/Kentucky/Louisville") == "America/Kentucky/Louisville"

    def test_serialize_none(self):
        """Test serializing None returns None."""
        assert serialize_timezone(None) is None

    def test_serialize_invalid_timezone(self):
        """Test serializing invalid timezones raises error."""
        # Abbreviations not supported
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("EST")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("PST")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("UTC")

        # Offsets not supported
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("UTC+5")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("GMT-8")

        # Wrong capitalization
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("america/new_york")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("AMERICA/NEW_YORK")

        # Missing slash
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("NewYork")

        # Too many parts
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("America/USA/New_York/Manhattan")

        # Empty
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            serialize_timezone("")


class TestTimezoneParsing:
    """Test timezone parsing from variables."""

    def test_parse_valid_timezone(self):
        """Test parsing valid timezones."""
        assert parse_timezone_value("America/New_York") == "America/New_York"
        assert parse_timezone_value("Europe/Paris") == "Europe/Paris"
        assert parse_timezone_value("America/Argentina/Buenos_Aires") == "America/Argentina/Buenos_Aires"

    def test_parse_invalid_timezone(self):
        """Test parsing invalid timezones raises error."""
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_value("EST")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_value("UTC+5")

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_value("america/new_york")

    def test_parse_invalid_type(self):
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_value(123)

        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_value(None)

        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_value(["America/New_York"])


class TestTimezoneField:
    """Test TimezoneField class."""

    def test_create_valid_timezone_field(self):
        """Test creating TimezoneField with valid values."""
        tz = TimezoneField("America/New_York")
        assert tz == "America/New_York"
        assert isinstance(tz, str)

        tz = TimezoneField("Europe/Paris")
        assert tz == "Europe/Paris"

        tz = TimezoneField("America/Argentina/Buenos_Aires")
        assert tz == "America/Argentina/Buenos_Aires"

    def test_create_invalid_timezone_field(self):
        """Test creating TimezoneField with invalid values raises error."""
        with pytest.raises(ValueError, match="Invalid timezone"):
            TimezoneField("EST")

        with pytest.raises(ValueError, match="Invalid timezone"):
            TimezoneField("UTC+5")

        with pytest.raises(ValueError, match="Invalid timezone"):
            TimezoneField("america/new_york")


class TestTimezoneLiteralParsing:
    """Test parsing timezone from GraphQL literals."""

    def test_parse_valid_literal(self):
        """Test parsing valid timezone literals."""
        assert parse_timezone_literal(StringValueNode(value="America/New_York")) == "America/New_York"
        assert parse_timezone_literal(StringValueNode(value="Europe/Paris")) == "Europe/Paris"
        assert parse_timezone_literal(StringValueNode(value="America/Argentina/Buenos_Aires")) == "America/Argentina/Buenos_Aires"

    def test_parse_invalid_literal_format(self):
        """Test parsing invalid timezone format literals."""
        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_literal(StringValueNode(value="EST"))

        with pytest.raises(GraphQLError, match="Invalid timezone"):
            parse_timezone_literal(StringValueNode(value="america/new_york"))

    def test_parse_non_string_literal(self):
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="Timezone must be a string"):
            parse_timezone_literal(IntValueNode(value="123"))
```

---

## Testing Strategy

### Test Execution Plan

1. **Run individual scalar tests**:
   ```bash
   uv run pytest tests/unit/core/type_system/test_language_code_scalar.py -v
   uv run pytest tests/unit/core/type_system/test_locale_code_scalar.py -v
   uv run pytest tests/unit/core/type_system/test_timezone_scalar.py -v
   ```

2. **Run all scalar tests**:
   ```bash
   uv run pytest tests/unit/core/type_system/ -k "scalar" -v
   ```

3. **Run full test suite**:
   ```bash
   uv run pytest --tb=short
   ```

4. **Code quality checks**:
   ```bash
   uv run ruff check
   uv run mypy
   ```

### Test Coverage

Each scalar test suite includes:
- ✅ **Serialization tests**: Valid values, case handling, None handling, invalid values
- ✅ **Parsing tests**: Valid values, invalid values, type validation
- ✅ **Field marker tests**: Constructor validation, type checking
- ✅ **Literal parsing tests**: AST node handling, error cases

**Expected Coverage**: 100% for all three new scalar modules

---

## Validation & Quality Assurance

### Pre-commit Checklist

- [ ] All three scalar files created with correct patterns
- [ ] `scalars/__init__.py` updated with imports and exports
- [ ] `types/__init__.py` updated with public API exports
- [ ] All three test files created with comprehensive coverage
- [ ] All tests pass: `uv run pytest`
- [ ] Linting passes: `uv run ruff check`
- [ ] Type checking passes: `uv run mypy`
- [ ] No regressions in existing tests
- [ ] Code follows existing scalar patterns exactly

### Integration Verification

After implementation, verify integration by creating a test file:

```python
# test_i18n_integration.py
from fraiseql.types import LanguageCode, LocaleCode, Timezone, type, input

@type
class User:
    id: int
    name: str
    preferred_language: LanguageCode
    locale: LocaleCode
    timezone: Timezone

@input
class UpdateUserPreferencesInput:
    preferred_language: LanguageCode | None
    locale: LocaleCode | None
    timezone: Timezone | None

# Should work without errors
user = User(
    id=1,
    name="John Doe",
    preferred_language=LanguageCode("en"),
    locale=LocaleCode("en-US"),
    timezone=Timezone("America/New_York")
)

print("✅ i18n scalars integration successful!")
```

---

## Implementation Workflow

### Step-by-Step Execution

**Phase 1: Scalar Implementations (30 min)**

1. Create `src/fraiseql/types/scalars/language_code.py`
2. Create `src/fraiseql/types/scalars/locale_code.py`
3. Create `src/fraiseql/types/scalars/timezone.py`

**Phase 2: Module Updates (10 min)**

4. Update `src/fraiseql/types/scalars/__init__.py`
5. Update `src/fraiseql/types/__init__.py`

**Phase 3: Test Implementation (60 min)**

6. Create `tests/unit/core/type_system/test_language_code_scalar.py`
7. Create `tests/unit/core/type_system/test_locale_code_scalar.py`
8. Create `tests/unit/core/type_system/test_timezone_scalar.py`

**Phase 4: Validation (20 min)**

9. Run tests: `uv run pytest`
10. Run linting: `uv run ruff check`
11. Run type checking: `uv run mypy`
12. Fix any issues
13. Verify integration

**Total Estimated Time**: 2 hours

---

## Success Criteria

### Functional Requirements

✅ **LanguageCodeScalar**:
- Validates ISO 639-1 two-letter codes
- Case-insensitive (normalizes to lowercase)
- Rejects invalid formats

✅ **LocaleCodeScalar**:
- Validates BCP 47 format (language-REGION)
- Case-sensitive (enforces lowercase-UPPERCASE)
- Accepts language-only codes

✅ **TimezoneScalar**:
- Validates IANA timezone identifiers
- Case-sensitive (exact capitalization)
- Supports 2-part and 3-part formats

### Quality Requirements

✅ **Code Quality**:
- Follows existing scalar patterns exactly
- 100% test coverage
- Passes all linting and type checks
- No regressions in existing tests

✅ **Integration**:
- Exports available from `fraiseql.types`
- Works with `@fraiseql.type` and `@fraiseql.input`
- GraphQL schema generation working
- PostgreSQL integration compatible

### Documentation Requirements

✅ **Code Documentation**:
- Comprehensive docstrings
- Usage examples in class docstrings
- Clear error messages
- Reference links to standards

---

## PostgreSQL Integration Example

### Database Schema

```sql
CREATE TABLE app.users (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL,

    -- i18n fields with validation
    preferred_language TEXT
        CHECK (preferred_language ~ '^[a-z]{2}$'),

    locale TEXT
        CHECK (locale ~ '^[a-z]{2}(-[A-Z]{2})?$'),

    timezone TEXT
        CHECK (timezone ~ '^[A-Z][a-zA-Z_]+(/[A-Z][a-zA-Z_]+){1,2}$'),

    created_at TIMESTAMPTZ DEFAULT now()
);

COMMENT ON COLUMN app.users.preferred_language IS
    'ISO 639-1 two-letter language code (e.g., en, fr, de)';

COMMENT ON COLUMN app.users.locale IS
    'BCP 47 locale code for formatting (e.g., en-US, fr-FR)';

COMMENT ON COLUMN app.users.timezone IS
    'IANA timezone identifier (e.g., America/New_York, Europe/Paris)';
```

### FraiseQL Type Definition

```python
from fraiseql.types import type, LanguageCode, LocaleCode, Timezone, EmailAddress

@type(sql_source="app.users")
class User:
    id: UUID
    name: str
    email: EmailAddress
    preferred_language: LanguageCode
    locale: LocaleCode
    timezone: Timezone
    created_at: DateTime
```

### GraphQL Schema Output

```graphql
scalar LanguageCode
scalar LocaleCode
scalar Timezone

type User {
  id: UUID!
  name: String!
  email: EmailAddress!
  preferredLanguage: LanguageCode!
  locale: LocaleCode!
  timezone: Timezone!
  createdAt: DateTime!
}
```

---

## Real-World Use Cases

### 1. Multi-tenant SaaS Platform
```python
@type(sql_source="v_tenant_user", jsonb_column="preferences")
class TenantUser:
    id: int
    email: EmailAddress
    language: LanguageCode      # UI language
    locale: LocaleCode          # Date/number formatting
    timezone: Timezone          # Display all timestamps in user's timezone
```

### 2. Content Management System
```python
@type(sql_source="v_article", jsonb_column="metadata")
class Article:
    id: int
    title: str
    content: str
    language: LanguageCode      # Content language (for SEO, filtering)
    published_at: DateTime
```

### 3. Event Scheduling Platform
```python
@input
class CreateEventInput:
    title: str
    description: str
    start_time: DateTime
    timezone: Timezone          # Event timezone (handles DST correctly)
```

### 4. E-commerce Platform
```python
@type(sql_source="v_customer", jsonb_column="preferences")
class Customer:
    id: int
    email: EmailAddress
    locale: LocaleCode          # For price/date formatting (e.g., $1,234.56 vs 1.234,56 €)
    timezone: Timezone          # For order confirmation emails
```

---

## Benefits

### 1. Type Safety
- Prevent invalid language/locale/timezone values at API boundary
- Catch errors before database insert
- Better developer experience with IDE autocomplete

### 2. Validation
- Early validation at GraphQL layer
- Consistent error messages
- Prevents storage of malformed data

### 3. Self-Documenting
- GraphQL schema shows valid formats
- Clear descriptions and examples
- Reference links to standards

### 4. PostgreSQL Optimization
- CHECK constraints enable query optimization
- Indexed text columns with guaranteed format
- Database-level validation as backup

### 5. Universal Utility
- Every global application needs these types
- Common pattern across all FraiseQL projects
- Reusable in any multi-language/multi-region application

---

## References

- **ISO 639-1 Language Codes**: https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes
- **BCP 47 Locale Format**: https://tools.ietf.org/html/bcp47
- **IANA Timezone Database**: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
- **FraiseQL Scalar Patterns**: `/home/lionel/code/fraiseql/src/fraiseql/types/scalars/`
- **Existing Test Patterns**: `/home/lionel/code/fraiseql/tests/unit/core/type_system/test_hostname_scalar.py`

---

## Notes for Implementation

### Critical Pattern Requirements

1. **Follow existing patterns EXACTLY**: Use `hostname.py` as the reference template
2. **Test structure**: Follow `test_hostname_scalar.py` structure exactly
3. **Module docstrings**: Update both `scalars/__init__.py` and `types/__init__.py` docstrings
4. **Alphabetical ordering**: All imports and exports must be alphabetically ordered
5. **Error messages**: Must be clear, actionable, and include examples

### Common Pitfalls to Avoid

❌ **Don't**:
- Deviate from established scalar patterns
- Skip test coverage for edge cases
- Forget to update module docstrings
- Use inconsistent error message formats

✅ **Do**:
- Copy-paste existing scalar structure and modify
- Test all validation edge cases
- Run full test suite before considering complete
- Verify exports are accessible from `fraiseql.types`

---

## Final Validation Checklist

Before marking this issue as complete:

- [ ] All three scalar files created: `language_code.py`, `locale_code.py`, `timezone.py`
- [ ] All three test files created with 100% coverage
- [ ] `scalars/__init__.py` updated with imports, exports, docstring
- [ ] `types/__init__.py` updated with imports, exports
- [ ] All tests pass: `uv run pytest`
- [ ] No regressions: `uv run pytest tests/unit/core/type_system/ -k scalar`
- [ ] Linting passes: `uv run ruff check`
- [ ] Type checking passes: `uv run mypy`
- [ ] Integration test successful
- [ ] GraphQL schema generation working
- [ ] Documentation complete

---

**Implementation ready for execution!**

This plan provides complete, copy-paste-ready implementations following FraiseQL's established patterns. An agent executing this plan should:

1. Create files in exact order listed
2. Copy code blocks exactly as written
3. Run tests after each phase
4. Fix any issues before proceeding to next phase
5. Verify all checkboxes before completion

**Estimated completion time**: 2-3 hours
**Risk level**: Low (simple, well-defined patterns)
**Dependencies**: None (self-contained feature addition)
