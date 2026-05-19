//! Scalar type registry - unified source of truth for scalar type recognition.
//!
//! This module consolidates all scalar type definitions (builtin and rich) into a
//! single location to eliminate duplication and provide a consistent API for checking
//! whether a given type name is a known scalar.

/// Builtin GraphQL scalar types that are always available.
///
/// These are the core scalar types defined by the GraphQL specification and
/// commonly used types provided by FraiseQL.
pub const BUILTIN_SCALARS: &[&str] = &[
    "ID",
    "String",
    "Int",
    "Float",
    "Boolean",
    "DateTime",
    "Date",
    "Time",
    "JSON",
    "UUID",
    "Decimal",
    "BigInt",
    "Timestamp",
    "Void",
];

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
/// This provides a unified way to determine if a type string refers to a
/// scalar type, eliminating the need to maintain multiple hardcoded lists
/// throughout the codebase.
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
    BUILTIN_SCALARS.contains(&name) || RICH_SCALARS.contains(&name)
}
