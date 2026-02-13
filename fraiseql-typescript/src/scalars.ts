/**
 * FraiseQL scalar type markers for schema authoring.
 *
 * These are branded type aliases used in TypeScript to generate the correct
 * GraphQL scalar types in schema.json. They have no runtime behavior - validation
 * and serialization happen in the Rust runtime after compilation.
 *
 * Architecture:
 *   TypeScript type annotation → schema.json type string → Rust FieldType → codegen/introspection
 *
 * @example
 * ```typescript
 * import { Type } from "fraiseql";
 * import { ID, DateTime, Email, URL } from "fraiseql/scalars";
 *
 * @Type()
 * class User {
 *   id: ID;                    // → "ID" in schema.json → FieldType::Id
 *   name: string;              // → "String"
 *   email: Email;              // → "Email" → FieldType::Scalar("Email")
 *   website: URL | null;       // → "URL" (nullable)
 *   createdAt: DateTime;       // → "DateTime" → FieldType::DateTime
 * }
 * ```
 *
 * FraiseQL Convention:
 *   - `id` fields should ALWAYS use `ID` type (UUID v4 at runtime)
 *   - Foreign keys (e.g., `authorId`) should also use `ID`
 *
 * Custom Scalars:
 *   You can define your own custom scalars using branded types:
 *
 *   ```typescript
 *   type MyCustomScalar = string & { readonly __brand: "MyCustomScalar" };
 *   ```
 *
 *   The scalar name will pass through to schema.json and be validated at runtime.
 *
 * @packageDocumentation
 */

// =============================================================================
// Branded Type Helper
// =============================================================================

/**
 * Creates a branded type for nominal typing.
 * This allows TypeScript to distinguish between different scalar types at compile time.
 */
type Brand<T, B extends string> = T & { readonly __brand: B };

// =============================================================================
// Core GraphQL Scalars
// =============================================================================

/**
 * GraphQL ID scalar - used for unique identifiers.
 *
 * FraiseQL enforces UUID v4 format for all ID fields at runtime.
 * This is the REQUIRED type for `id` fields and foreign key references.
 */
export type ID = Brand<string, "ID">;

// =============================================================================
// Date/Time Scalars
// =============================================================================

/** ISO 8601 DateTime scalar (e.g., "2025-01-10T12:00:00Z"). */
export type DateTime = Brand<string, "DateTime">;

/** ISO 8601 Date scalar (e.g., "2025-01-10"). */
export type Date = Brand<string, "Date">;

/** ISO 8601 Time scalar (e.g., "12:00:00"). */
export type Time = Brand<string, "Time">;

/** Date range scalar (e.g., "[2025-01-01,2025-12-31)"). */
export type DateRange = Brand<string, "DateRange">;

/** ISO 8601 Duration scalar (e.g., "P1Y2M3D"). */
export type Duration = Brand<string, "Duration">;

// =============================================================================
// Complex Core Scalars
// =============================================================================

/** Arbitrary JSON value scalar. Maps to PostgreSQL JSONB. */
export type Json = Brand<unknown, "Json">;

/** UUID scalar (explicit UUID type, distinct from ID). */
export type UUID = Brand<string, "UUID">;

/** Decimal/BigDecimal scalar for precise numeric values. */
export type Decimal = Brand<string, "Decimal">;

/** Vector scalar for pgvector embeddings. */
export type Vector = Brand<number[], "Vector">;

// =============================================================================
// Contact/Communication Scalars
// =============================================================================

/** Email address scalar with RFC 5322 validation. */
export type Email = Brand<string, "Email">;

/** Phone number scalar (E.164 format recommended). */
export type PhoneNumber = Brand<string, "PhoneNumber">;

/** URL scalar with RFC 3986 validation. */
export type URL = Brand<string, "URL">;

/** Domain name scalar. */
export type DomainName = Brand<string, "DomainName">;

/** Hostname scalar. */
export type Hostname = Brand<string, "Hostname">;

// =============================================================================
// Location/Address Scalars
// =============================================================================

/** Postal/ZIP code scalar. */
export type PostalCode = Brand<string, "PostalCode">;

/** Latitude coordinate (-90 to 90). */
export type Latitude = Brand<number, "Latitude">;

/** Longitude coordinate (-180 to 180). */
export type Longitude = Brand<number, "Longitude">;

/** Geographic coordinates scalar (lat,lng or GeoJSON). */
export type Coordinates = Brand<string, "Coordinates">;

/** IANA timezone identifier (e.g., "America/New_York"). */
export type Timezone = Brand<string, "Timezone">;

/** Locale code scalar (e.g., "en-US"). */
export type LocaleCode = Brand<string, "LocaleCode">;

/** ISO 639-1 language code (e.g., "en"). */
export type LanguageCode = Brand<string, "LanguageCode">;

/** ISO 3166-1 alpha-2 country code (e.g., "US"). */
export type CountryCode = Brand<string, "CountryCode">;

// =============================================================================
// Financial Scalars
// =============================================================================

/** International Bank Account Number. */
export type IBAN = Brand<string, "IBAN">;

/** CUSIP identifier for North American securities. */
export type CUSIP = Brand<string, "CUSIP">;

/** International Securities Identification Number. */
export type ISIN = Brand<string, "ISIN">;

/** Stock Exchange Daily Official List number. */
export type SEDOL = Brand<string, "SEDOL">;

/** Legal Entity Identifier. */
export type LEI = Brand<string, "LEI">;

/** Market Identifier Code. */
export type MIC = Brand<string, "MIC">;

/** ISO 4217 currency code (e.g., "USD"). */
export type CurrencyCode = Brand<string, "CurrencyCode">;

/** Monetary amount with currency (e.g., "USD 100.00"). */
export type Money = Brand<string, "Money">;

/** Stock exchange code. */
export type ExchangeCode = Brand<string, "ExchangeCode">;

/** Currency exchange rate. */
export type ExchangeRate = Brand<string, "ExchangeRate">;

/** Stock ticker symbol. */
export type StockSymbol = Brand<string, "StockSymbol">;

/** Percentage value (0-100 or 0-1 depending on context). */
export type Percentage = Brand<number, "Percentage">;

// =============================================================================
// Identifier Scalars
// =============================================================================

/** URL-safe slug (lowercase, hyphens, no spaces). */
export type Slug = Brand<string, "Slug">;

/** Semantic version string (e.g., "1.2.3"). */
export type SemanticVersion = Brand<string, "SemanticVersion">;

/** SHA-256 hash string (64 hex characters). */
export type HashSHA256 = Brand<string, "HashSHA256">;

/** API key string. */
export type APIKey = Brand<string, "APIKey">;

/** Vehicle license plate number. */
export type LicensePlate = Brand<string, "LicensePlate">;

/** Vehicle Identification Number. */
export type VIN = Brand<string, "VIN">;

/** Shipping tracking number. */
export type TrackingNumber = Brand<string, "TrackingNumber">;

/** Shipping container number (ISO 6346). */
export type ContainerNumber = Brand<string, "ContainerNumber">;

// =============================================================================
// Networking Scalars
// =============================================================================

/** IP address (IPv4 or IPv6). */
export type IPAddress = Brand<string, "IPAddress">;

/** IPv4 address. */
export type IPv4 = Brand<string, "IPv4">;

/** IPv6 address. */
export type IPv6 = Brand<string, "IPv6">;

/** MAC address. */
export type MACAddress = Brand<string, "MACAddress">;

/** CIDR notation for IP ranges. */
export type CIDR = Brand<string, "CIDR">;

/** Network port number (0-65535). */
export type Port = Brand<number, "Port">;

// =============================================================================
// Transportation Scalars
// =============================================================================

/** IATA airport code (e.g., "JFK"). */
export type AirportCode = Brand<string, "AirportCode">;

/** UN/LOCODE port code. */
export type PortCode = Brand<string, "PortCode">;

/** Flight number (e.g., "AA123"). */
export type FlightNumber = Brand<string, "FlightNumber">;

// =============================================================================
// Content Scalars
// =============================================================================

/** Markdown-formatted text. */
export type Markdown = Brand<string, "Markdown">;

/** HTML-formatted text. */
export type HTML = Brand<string, "HTML">;

/** MIME type (e.g., "application/json"). */
export type MimeType = Brand<string, "MimeType">;

/** Color value (hex, RGB, or named). */
export type Color = Brand<string, "Color">;

/** Image reference (URL or base64). */
export type Image = Brand<string, "Image">;

/** File reference (URL or path). */
export type File = Brand<string, "File">;

// =============================================================================
// Database/PostgreSQL Specific Scalars
// =============================================================================

/** PostgreSQL ltree path (e.g., "root.child.leaf"). */
export type LTree = Brand<string, "LTree">;

// =============================================================================
// Scalar Name Registry
// =============================================================================

/**
 * Set of all known scalar type names.
 * Used by the type system to recognize scalar types.
 */
export const SCALAR_NAMES = new Set([
  // Core
  "ID",
  "UUID",
  "Json",
  "Decimal",
  "Vector",
  // Date/Time
  "DateTime",
  "Date",
  "Time",
  "DateRange",
  "Duration",
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
  "Percentage",
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
  // Database
  "LTree",
]);

/**
 * Check if a type name is a known scalar type.
 */
export function isScalarType(typeName: string): boolean {
  return SCALAR_NAMES.has(typeName);
}
