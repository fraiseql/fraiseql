//! Scalar type registry - unified source of truth for scalar type recognition.
//!
//! This module consolidates all scalar type definitions (builtin and rich) into a
//! single location to eliminate duplication and provide a consistent API for checking
//! whether a given type name is a known scalar.

/// Rich scalar types with validation rules beyond basic GraphQL scalars.
///
/// These are scalar types with application-level validation rules.
/// They are stored as TEXT in PostgreSQL and validated at the application level.
pub const RICH_SCALARS: &[&str] = &[
    // Contact/Communication
    "Email",
    "PhoneNumber",
    "URL",
    "DomainName",
    "Hostname",
    // Location/Address
    "PostalCode",
    "Latitude",
    "Longitude",
    "Coordinates",
    "Timezone",
    "LocaleCode",
    "LanguageCode",
    "CountryCode",
    // Financial
    "IBAN",
    "CUSIP",
    "ISIN",
    "SEDOL",
    "LEI",
    "MIC",
    "CurrencyCode",
    "Money",
    "ExchangeCode",
    "ExchangeRate",
    "StockSymbol",
    // Identifiers
    "Slug",
    "SemanticVersion",
    "HashSHA256",
    "APIKey",
    "LicensePlate",
    "VIN",
    "TrackingNumber",
    "ContainerNumber",
    // Networking
    "IPAddress",
    "IPv4",
    "IPv6",
    "MACAddress",
    "CIDR",
    "Port",
    // Transportation
    "AirportCode",
    "PortCode",
    "FlightNumber",
    // Content
    "Markdown",
    "HTML",
    "MimeType",
    "Color",
    "Image",
    "File",
    // Database/PostgreSQL specific
    "LTree",
    // Ranges
    "DateRange",
    "Duration",
    "Percentage",
];

/// Check if a type name is a known scalar (builtin or rich).
///
/// The built-in half reads [`BUILTIN_SCALARS`](super::BUILTIN_SCALARS), the
/// table the compiler itself parses authored type names with, so this answers
/// the same question the compiler does. It used to carry a hand-written list of
/// its own, which claimed to be "the unified source of truth" while disagreeing
/// with the compiler in both directions: it spelled JSON `"JSON"` where the
/// authoring format writes `"Json"`, it did not know the four vector types, and
/// it called `BigInt`, `Timestamp` and `Void` built-ins, none of which the
/// compiler recognizes as a field type.
///
/// # Arguments
///
/// * `name` - The type name to check
///
/// # Returns
///
/// `true` if the name is a known scalar (builtin or rich), `false` otherwise.
///
/// # Examples
///
/// ```
/// # use fraiseql_core::schema::is_known_scalar;
/// assert!(is_known_scalar("String"));
/// assert!(is_known_scalar("Email"));
/// assert!(is_known_scalar("UUID"));
/// assert!(!is_known_scalar("User"));
/// assert!(!is_known_scalar("CustomType"));
/// ```
#[inline]
#[must_use]
pub fn is_known_scalar(name: &str) -> bool {
    super::field_type::BUILTIN_SCALARS.iter().any(|(builtin, _)| *builtin == name)
        || RICH_SCALARS.contains(&name)
}
