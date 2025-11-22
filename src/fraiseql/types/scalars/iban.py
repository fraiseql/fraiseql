"""IBAN scalar type for ISO 13616 bank account number validation."""

import re
from typing import Any

from graphql import GraphQLError, GraphQLScalarType
from graphql.language import StringValueNode

from fraiseql.types.definitions import ScalarMarker

# ISO 13616 IBAN: 2 letters + 2 digits + up to 30 alphanumeric
_IBAN_REGEX = re.compile(r"^[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}$")

# IBAN country-specific lengths (ISO 13616)
_IBAN_LENGTHS = {
    "AL": 28,
    "AD": 24,
    "AT": 20,
    "AZ": 28,
    "BH": 22,
    "BY": 28,
    "BE": 16,
    "BA": 20,
    "BR": 29,
    "BG": 22,
    "CR": 22,
    "HR": 21,
    "CY": 28,
    "CZ": 24,
    "DK": 18,
    "DO": 28,
    "EG": 29,
    "EE": 20,
    "FO": 18,
    "FI": 18,
    "FR": 27,
    "GE": 22,
    "DE": 22,
    "GI": 23,
    "GR": 27,
    "GL": 18,
    "GT": 28,
    "HU": 28,
    "IS": 26,
    "IQ": 23,
    "IE": 22,
    "IL": 23,
    "IT": 27,
    "JO": 30,
    "KZ": 20,
    "XK": 20,
    "KW": 30,
    "LV": 21,
    "LB": 28,
    "LY": 25,
    "LI": 21,
    "LT": 20,
    "LU": 20,
    "MK": 19,
    "MT": 31,
    "MR": 27,
    "MU": 30,
    "MD": 24,
    "MC": 27,
    "ME": 22,
    "NL": 18,
    "NO": 15,
    "PK": 24,
    "PS": 29,
    "PL": 28,
    "PT": 25,
    "QA": 29,
    "RO": 24,
    "LC": 32,
    "SM": 27,
    "ST": 25,
    "SA": 24,
    "RS": 22,
    "SC": 31,
    "SK": 24,
    "SI": 19,
    "ES": 24,
    "SE": 24,
    "CH": 21,
    "TL": 23,
    "TN": 24,
    "TR": 26,
    "UA": 29,
    "AE": 23,
    "GB": 22,
    "VA": 22,
    "VG": 24,
    "DZ": 24,
    "AO": 25,
    "BJ": 28,
    "BF": 28,
    "BI": 16,
    "CM": 27,
    "CV": 25,
    "CF": 27,
    "TD": 27,
    "KM": 27,
    "CG": 27,
    "DJ": 27,
    "GQ": 27,
    "GA": 27,
    "GW": 25,
    "HN": 28,
    "IR": 26,
    "CI": 28,
    "MG": 27,
    "ML": 28,
    "MA": 28,
    "MZ": 25,
    "NI": 32,
    "NE": 28,
    "SN": 28,
    "TG": 28,
}


def _validate_iban_check_digits(iban: str) -> bool:
    """Validate IBAN check digits using ISO 13616 mod-97 algorithm."""
    # Move first 4 characters to the end
    rearranged = iban[4:] + iban[:4]

    # Convert letters to numbers (A=10, B=11, ..., Z=35)
    numeric_string = ""
    for char in rearranged:
        if char.isdigit():
            numeric_string += char
        else:
            # A=10, B=11, ..., Z=35
            numeric_string += str(ord(char.upper()) - ord("A") + 10)

    # Check if the number is divisible by 97
    # Use modulo operation in chunks to handle large numbers
    try:
        number = int(numeric_string)
        return number % 97 == 1
    except ValueError:
        return False


def serialize_iban(value: Any) -> str | None:
    """Serialize IBAN to string."""
    if value is None:
        return None

    value_str = str(value).upper().replace(" ", "")

    if not _IBAN_REGEX.match(value_str):
        raise GraphQLError(
            f"Invalid IBAN: {value}. Must be ISO 13616 format: "
            "2 country letters + 2 check digits + 11-30 alphanumeric "
            "(e.g., 'GB82WEST12345698765432')"
        )

    # Check country-specific length
    country_code = value_str[:2]
    expected_length = _IBAN_LENGTHS.get(country_code)
    if expected_length and len(value_str) != expected_length:
        raise GraphQLError(
            f"Invalid IBAN: {value}. {country_code} IBANs must be {expected_length} characters long"
        )

    if not _validate_iban_check_digits(value_str):
        raise GraphQLError(
            f"Invalid IBAN: {value}. Check digits do not match (ISO 13616 mod-97 validation failed)"
        )

    return value_str


def parse_iban_value(value: Any) -> str:
    """Parse IBAN from variable value."""
    if not isinstance(value, str):
        raise GraphQLError(f"IBAN must be a string, got {type(value).__name__}")

    value_upper = value.upper().replace(" ", "")

    if not _IBAN_REGEX.match(value_upper):
        raise GraphQLError(
            f"Invalid IBAN: {value}. Must be ISO 13616 format: "
            "2 country letters + 2 check digits + 11-30 alphanumeric "
            "(e.g., 'GB82WEST12345698765432')"
        )

    # Check country-specific length
    country_code = value_upper[:2]
    expected_length = _IBAN_LENGTHS.get(country_code)
    if expected_length and len(value_upper) != expected_length:
        raise GraphQLError(
            f"Invalid IBAN: {value}. {country_code} IBANs must be {expected_length} characters long"
        )

    if not _validate_iban_check_digits(value_upper):
        raise GraphQLError(
            f"Invalid IBAN: {value}. Check digits do not match (ISO 13616 mod-97 validation failed)"
        )

    return value_upper


def parse_iban_literal(ast: Any, _variables: dict[str, Any] | None = None) -> str:
    """Parse IBAN from AST literal."""
    if not isinstance(ast, StringValueNode):
        raise GraphQLError("IBAN must be a string")

    return parse_iban_value(ast.value)


IBANScalar = GraphQLScalarType(
    name="IBAN",
    description=(
        "International Bank Account Number (ISO 13616). "
        "Format: 2 country letters + 2 check digits + account number. "
        "Length varies by country (15-32 characters). "
        "Examples: GB82WEST12345698765432, DE89370400440532013000. "
        "See: https://en.wikipedia.org/wiki/International_Bank_Account_Number"
    ),
    serialize=serialize_iban,
    parse_value=parse_iban_value,
    parse_literal=parse_iban_literal,
)


class IBANField(str, ScalarMarker):
    """International Bank Account Number (ISO 13616).

    This scalar validates IBANs according to ISO 13616:
    - 2 uppercase letters (country code)
    - 2 digits (check digits)
    - 11-30 alphanumeric characters (account number)
    - Total length varies by country
    - Mod-97 check digit validation

    The check digits ensure data integrity using the ISO 13616 mod-97 algorithm.

    Examples:
    - GB82WEST12345698765432 (UK)
    - DE89370400440532013000 (Germany)
    - FR7630006000011234567890189 (France)

    Example:
        >>> from fraiseql.types import IBAN
        >>>
        >>> @fraiseql.type
        ... class BankAccount:
        ...     iban: IBAN
        ...     account_holder: str
        ...     currency: CurrencyCode
    """

    __slots__ = ()

    def __new__(cls, value: str) -> "IBANField":
        """Create a new IBANField instance with validation."""
        value_upper = value.upper().replace(" ", "")

        if not _IBAN_REGEX.match(value_upper):
            raise ValueError(
                f"Invalid IBAN: {value}. Must be ISO 13616 format: "
                "2 country letters + 2 check digits + 11-30 alphanumeric "
                "(e.g., 'GB82WEST12345698765432')"
            )

        # Check country-specific length
        country_code = value_upper[:2]
        expected_length = _IBAN_LENGTHS.get(country_code)
        if expected_length and len(value_upper) != expected_length:
            raise ValueError(
                f"Invalid IBAN: {value}. {country_code} IBANs must be "
                f"{expected_length} characters long"
            )

        if not _validate_iban_check_digits(value_upper):
            raise ValueError(
                f"Invalid IBAN: {value}. Check digits do not match "
                "(ISO 13616 mod-97 validation failed)"
            )

        return super().__new__(cls, value_upper)
