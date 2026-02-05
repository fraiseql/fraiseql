// Package fraiseql provides scalar type markers for schema authoring.
//
// These are type aliases used in Go to generate the correct GraphQL scalar
// types in schema.json. They have no runtime behavior - validation and
// serialization happen in the Rust runtime after compilation.
//
// Architecture:
//
//	Go type annotation → schema.json type string → Rust FieldType → codegen/introspection
//
// Example:
//
//	import "github.com/fraiseql/fraiseql-go/fraiseql"
//
//	type User struct {
//	    ID        fraiseql.ID       `fraiseql:"id,type=ID"`           // → "ID" in schema.json
//	    Name      string            `fraiseql:"name"`                  // → "String"
//	    Email     fraiseql.Email    `fraiseql:"email,type=Email"`     // → "Email"
//	    Website   *fraiseql.URL     `fraiseql:"website,type=URL"`     // → "URL" (nullable)
//	    CreatedAt fraiseql.DateTime `fraiseql:"createdAt,type=DateTime"` // → "DateTime"
//	}
//
// FraiseQL Convention:
//   - `id` fields should ALWAYS use `ID` type (UUID v4 at runtime)
//   - Foreign keys (e.g., `authorId`) should also use `ID`
//
// Custom Scalars:
// You can define your own custom scalars using type aliases:
//
//	type MyCustomScalar string
//
// Then use the `type=MyCustomScalar` tag to specify the GraphQL type.
package fraiseql

// =============================================================================
// Core GraphQL Scalars
// =============================================================================

// ID is the GraphQL ID scalar - used for unique identifiers.
// FraiseQL enforces UUID v4 format for all ID fields at runtime.
// This is the REQUIRED type for `id` fields and foreign key references.
type ID string

// =============================================================================
// Date/Time Scalars
// =============================================================================

// DateTime is an ISO 8601 DateTime scalar (e.g., "2025-01-10T12:00:00Z").
type DateTime string

// Date is an ISO 8601 Date scalar (e.g., "2025-01-10").
type Date string

// Time is an ISO 8601 Time scalar (e.g., "12:00:00").
type Time string

// DateRange is a date range scalar (e.g., "[2025-01-01,2025-12-31)").
type DateRange string

// Duration is an ISO 8601 Duration scalar (e.g., "P1Y2M3D").
type Duration string

// =============================================================================
// Complex Core Scalars
// =============================================================================

// Json is an arbitrary JSON value scalar. Maps to PostgreSQL JSONB.
type Json any

// UUID is a UUID scalar (explicit UUID type, distinct from ID).
type UUID string

// Decimal is a Decimal/BigDecimal scalar for precise numeric values.
type Decimal string

// Vector is a vector scalar for pgvector embeddings.
type Vector []float64

// =============================================================================
// Contact/Communication Scalars
// =============================================================================

// Email is an email address scalar with RFC 5322 validation.
type Email string

// PhoneNumber is a phone number scalar (E.164 format recommended).
type PhoneNumber string

// URL is a URL scalar with RFC 3986 validation.
type URL string

// DomainName is a domain name scalar.
type DomainName string

// Hostname is a hostname scalar.
type Hostname string

// =============================================================================
// Location/Address Scalars
// =============================================================================

// PostalCode is a postal/ZIP code scalar.
type PostalCode string

// Latitude is a latitude coordinate (-90 to 90).
type Latitude float64

// Longitude is a longitude coordinate (-180 to 180).
type Longitude float64

// Coordinates is a geographic coordinates scalar (lat,lng or GeoJSON).
type Coordinates string

// Timezone is an IANA timezone identifier (e.g., "America/New_York").
type Timezone string

// LocaleCode is a locale code scalar (e.g., "en-US").
type LocaleCode string

// LanguageCode is an ISO 639-1 language code (e.g., "en").
type LanguageCode string

// CountryCode is an ISO 3166-1 alpha-2 country code (e.g., "US").
type CountryCode string

// =============================================================================
// Financial Scalars
// =============================================================================

// IBAN is an International Bank Account Number.
type IBAN string

// CUSIP is a CUSIP identifier for North American securities.
type CUSIP string

// ISIN is an International Securities Identification Number.
type ISIN string

// SEDOL is a Stock Exchange Daily Official List number.
type SEDOL string

// LEI is a Legal Entity Identifier.
type LEI string

// MIC is a Market Identifier Code.
type MIC string

// CurrencyCode is an ISO 4217 currency code (e.g., "USD").
type CurrencyCode string

// Money is a monetary amount with currency (e.g., "USD 100.00").
type Money string

// ExchangeCode is a stock exchange code.
type ExchangeCode string

// ExchangeRate is a currency exchange rate.
type ExchangeRate string

// StockSymbol is a stock ticker symbol.
type StockSymbol string

// Percentage is a percentage value (0-100 or 0-1 depending on context).
type Percentage float64

// =============================================================================
// Identifier Scalars
// =============================================================================

// Slug is a URL-safe slug (lowercase, hyphens, no spaces).
type Slug string

// SemanticVersion is a semantic version string (e.g., "1.2.3").
type SemanticVersion string

// HashSHA256 is a SHA-256 hash string (64 hex characters).
type HashSHA256 string

// APIKey is an API key string.
type APIKey string

// LicensePlate is a vehicle license plate number.
type LicensePlate string

// VIN is a Vehicle Identification Number.
type VIN string

// TrackingNumber is a shipping tracking number.
type TrackingNumber string

// ContainerNumber is a shipping container number (ISO 6346).
type ContainerNumber string

// =============================================================================
// Networking Scalars
// =============================================================================

// IPAddress is an IP address (IPv4 or IPv6).
type IPAddress string

// IPv4 is an IPv4 address.
type IPv4 string

// IPv6 is an IPv6 address.
type IPv6 string

// MACAddress is a MAC address.
type MACAddress string

// CIDR is CIDR notation for IP ranges.
type CIDR string

// Port is a network port number (0-65535).
type Port int

// =============================================================================
// Transportation Scalars
// =============================================================================

// AirportCode is an IATA airport code (e.g., "JFK").
type AirportCode string

// PortCode is a UN/LOCODE port code.
type PortCode string

// FlightNumber is a flight number (e.g., "AA123").
type FlightNumber string

// =============================================================================
// Content Scalars
// =============================================================================

// Markdown is markdown-formatted text.
type Markdown string

// HTML is HTML-formatted text.
type HTML string

// MimeType is a MIME type (e.g., "application/json").
type MimeType string

// Color is a color value (hex, RGB, or named).
type Color string

// Image is an image reference (URL or base64).
type Image string

// File is a file reference (URL or path).
type File string

// =============================================================================
// Database/PostgreSQL Specific Scalars
// =============================================================================

// LTree is a PostgreSQL ltree path (e.g., "root.child.leaf").
type LTree string

// =============================================================================
// Scalar Name Registry
// =============================================================================

// ScalarNames contains all known scalar type names.
// Used by the type system to recognize scalar types.
var ScalarNames = map[string]bool{
	// Core
	"ID":      true,
	"UUID":    true,
	"Json":    true,
	"Decimal": true,
	"Vector":  true,
	// Date/Time
	"DateTime":  true,
	"Date":      true,
	"Time":      true,
	"DateRange": true,
	"Duration":  true,
	// Contact/Communication
	"Email":       true,
	"PhoneNumber": true,
	"URL":         true,
	"DomainName":  true,
	"Hostname":    true,
	// Location/Address
	"PostalCode":   true,
	"Latitude":     true,
	"Longitude":    true,
	"Coordinates":  true,
	"Timezone":     true,
	"LocaleCode":   true,
	"LanguageCode": true,
	"CountryCode":  true,
	// Financial
	"IBAN":         true,
	"CUSIP":        true,
	"ISIN":         true,
	"SEDOL":        true,
	"LEI":          true,
	"MIC":          true,
	"CurrencyCode": true,
	"Money":        true,
	"ExchangeCode": true,
	"ExchangeRate": true,
	"StockSymbol":  true,
	"Percentage":   true,
	// Identifiers
	"Slug":            true,
	"SemanticVersion": true,
	"HashSHA256":      true,
	"APIKey":          true,
	"LicensePlate":    true,
	"VIN":             true,
	"TrackingNumber":  true,
	"ContainerNumber": true,
	// Networking
	"IPAddress":  true,
	"IPv4":       true,
	"IPv6":       true,
	"MACAddress": true,
	"CIDR":       true,
	"Port":       true,
	// Transportation
	"AirportCode":  true,
	"PortCode":     true,
	"FlightNumber": true,
	// Content
	"Markdown": true,
	"HTML":     true,
	"MimeType": true,
	"Color":    true,
	"Image":    true,
	"File":     true,
	// Database
	"LTree": true,
}

// IsScalarType checks if a type name is a known scalar type.
func IsScalarType(typeName string) bool {
	return ScalarNames[typeName]
}
