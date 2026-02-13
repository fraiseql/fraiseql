<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * FraiseQL scalar type constants for schema authoring.
 *
 * These are string constants used in PHP to generate the correct GraphQL
 * scalar types in schema.json. They have no runtime behavior - validation
 * and serialization happen in the Rust runtime after compilation.
 *
 * Architecture:
 *   PHP type annotation → schema.json type string → Rust FieldType → codegen/introspection
 *
 * Example:
 * ```php
 * use FraiseQL\Attributes\GraphQLType;
 * use FraiseQL\Attributes\GraphQLField;
 * use FraiseQL\Scalars;
 *
 * #[GraphQLType(name: 'User')]
 * class User {
 *     #[GraphQLField(type: Scalars::ID)]
 *     public string $id;                    // → "ID" in schema.json
 *
 *     #[GraphQLField]
 *     public string $name;                  // → "String"
 *
 *     #[GraphQLField(type: Scalars::EMAIL)]
 *     public string $email;                 // → "Email"
 *
 *     #[GraphQLField(type: Scalars::URL, nullable: true)]
 *     public ?string $website;              // → "URL" (nullable)
 *
 *     #[GraphQLField(type: Scalars::DATE_TIME)]
 *     public string $createdAt;             // → "DateTime"
 * }
 * ```
 *
 * FraiseQL Convention:
 *   - `id` fields should ALWAYS use Scalars::ID (UUID v4 at runtime)
 *   - Foreign keys (e.g., `authorId`) should also use Scalars::ID
 *
 * Custom Scalars:
 * You can define your own custom scalars by using any string in the type parameter:
 * ```php
 * #[GraphQLField(type: 'MyCustomScalar')]
 * public string $customField;
 * ```
 */
final class Scalars
{
    // =========================================================================
    // Core GraphQL Scalars
    // =========================================================================

    /** GraphQL ID scalar - used for unique identifiers. UUID v4 at runtime. */
    public const ID = 'ID';

    // =========================================================================
    // Date/Time Scalars
    // =========================================================================

    /** ISO 8601 DateTime scalar (e.g., "2025-01-10T12:00:00Z"). */
    public const DATE_TIME = 'DateTime';

    /** ISO 8601 Date scalar (e.g., "2025-01-10"). */
    public const DATE = 'Date';

    /** ISO 8601 Time scalar (e.g., "12:00:00"). */
    public const TIME = 'Time';

    /** Date range scalar (e.g., "[2025-01-01,2025-12-31)"). */
    public const DATE_RANGE = 'DateRange';

    /** ISO 8601 Duration scalar (e.g., "P1Y2M3D"). */
    public const DURATION = 'Duration';

    // =========================================================================
    // Complex Core Scalars
    // =========================================================================

    /** Arbitrary JSON value scalar. Maps to PostgreSQL JSONB. */
    public const JSON = 'Json';

    /** UUID scalar (explicit UUID type, distinct from ID). */
    public const UUID = 'UUID';

    /** Decimal/BigDecimal scalar for precise numeric values. */
    public const DECIMAL = 'Decimal';

    /** Vector scalar for pgvector embeddings. */
    public const VECTOR = 'Vector';

    // =========================================================================
    // Contact/Communication Scalars
    // =========================================================================

    /** Email address scalar with RFC 5322 validation. */
    public const EMAIL = 'Email';

    /** Phone number scalar (E.164 format recommended). */
    public const PHONE_NUMBER = 'PhoneNumber';

    /** URL scalar with RFC 3986 validation. */
    public const URL = 'URL';

    /** Domain name scalar. */
    public const DOMAIN_NAME = 'DomainName';

    /** Hostname scalar. */
    public const HOSTNAME = 'Hostname';

    // =========================================================================
    // Location/Address Scalars
    // =========================================================================

    /** Postal/ZIP code scalar. */
    public const POSTAL_CODE = 'PostalCode';

    /** Latitude coordinate (-90 to 90). */
    public const LATITUDE = 'Latitude';

    /** Longitude coordinate (-180 to 180). */
    public const LONGITUDE = 'Longitude';

    /** Geographic coordinates scalar (lat,lng or GeoJSON). */
    public const COORDINATES = 'Coordinates';

    /** IANA timezone identifier (e.g., "America/New_York"). */
    public const TIMEZONE = 'Timezone';

    /** Locale code scalar (e.g., "en-US"). */
    public const LOCALE_CODE = 'LocaleCode';

    /** ISO 639-1 language code (e.g., "en"). */
    public const LANGUAGE_CODE = 'LanguageCode';

    /** ISO 3166-1 alpha-2 country code (e.g., "US"). */
    public const COUNTRY_CODE = 'CountryCode';

    // =========================================================================
    // Financial Scalars
    // =========================================================================

    /** International Bank Account Number. */
    public const IBAN = 'IBAN';

    /** CUSIP identifier for North American securities. */
    public const CUSIP = 'CUSIP';

    /** International Securities Identification Number. */
    public const ISIN = 'ISIN';

    /** Stock Exchange Daily Official List number. */
    public const SEDOL = 'SEDOL';

    /** Legal Entity Identifier. */
    public const LEI = 'LEI';

    /** Market Identifier Code. */
    public const MIC = 'MIC';

    /** ISO 4217 currency code (e.g., "USD"). */
    public const CURRENCY_CODE = 'CurrencyCode';

    /** Monetary amount with currency (e.g., "USD 100.00"). */
    public const MONEY = 'Money';

    /** Stock exchange code. */
    public const EXCHANGE_CODE = 'ExchangeCode';

    /** Currency exchange rate. */
    public const EXCHANGE_RATE = 'ExchangeRate';

    /** Stock ticker symbol. */
    public const STOCK_SYMBOL = 'StockSymbol';

    /** Percentage value (0-100 or 0-1 depending on context). */
    public const PERCENTAGE = 'Percentage';

    // =========================================================================
    // Identifier Scalars
    // =========================================================================

    /** URL-safe slug (lowercase, hyphens, no spaces). */
    public const SLUG = 'Slug';

    /** Semantic version string (e.g., "1.2.3"). */
    public const SEMANTIC_VERSION = 'SemanticVersion';

    /** SHA-256 hash string (64 hex characters). */
    public const HASH_SHA256 = 'HashSHA256';

    /** API key string. */
    public const API_KEY = 'APIKey';

    /** Vehicle license plate number. */
    public const LICENSE_PLATE = 'LicensePlate';

    /** Vehicle Identification Number. */
    public const VIN = 'VIN';

    /** Shipping tracking number. */
    public const TRACKING_NUMBER = 'TrackingNumber';

    /** Shipping container number (ISO 6346). */
    public const CONTAINER_NUMBER = 'ContainerNumber';

    // =========================================================================
    // Networking Scalars
    // =========================================================================

    /** IP address (IPv4 or IPv6). */
    public const IP_ADDRESS = 'IPAddress';

    /** IPv4 address. */
    public const IPV4 = 'IPv4';

    /** IPv6 address. */
    public const IPV6 = 'IPv6';

    /** MAC address. */
    public const MAC_ADDRESS = 'MACAddress';

    /** CIDR notation for IP ranges. */
    public const CIDR = 'CIDR';

    /** Network port number (0-65535). */
    public const PORT = 'Port';

    // =========================================================================
    // Transportation Scalars
    // =========================================================================

    /** IATA airport code (e.g., "JFK"). */
    public const AIRPORT_CODE = 'AirportCode';

    /** UN/LOCODE port code. */
    public const PORT_CODE = 'PortCode';

    /** Flight number (e.g., "AA123"). */
    public const FLIGHT_NUMBER = 'FlightNumber';

    // =========================================================================
    // Content Scalars
    // =========================================================================

    /** Markdown-formatted text. */
    public const MARKDOWN = 'Markdown';

    /** HTML-formatted text. */
    public const HTML = 'HTML';

    /** MIME type (e.g., "application/json"). */
    public const MIME_TYPE = 'MimeType';

    /** Color value (hex, RGB, or named). */
    public const COLOR = 'Color';

    /** Image reference (URL or base64). */
    public const IMAGE = 'Image';

    /** File reference (URL or path). */
    public const FILE = 'File';

    // =========================================================================
    // Database/PostgreSQL Specific Scalars
    // =========================================================================

    /** PostgreSQL ltree path (e.g., "root.child.leaf"). */
    public const LTREE = 'LTree';

    // =========================================================================
    // Scalar Name Registry
    // =========================================================================

    /** @var array<string, true> Set of all known scalar type names. */
    private const SCALAR_NAMES = [
        // Core
        'ID' => true,
        'UUID' => true,
        'Json' => true,
        'Decimal' => true,
        'Vector' => true,
        // Date/Time
        'DateTime' => true,
        'Date' => true,
        'Time' => true,
        'DateRange' => true,
        'Duration' => true,
        // Contact/Communication
        'Email' => true,
        'PhoneNumber' => true,
        'URL' => true,
        'DomainName' => true,
        'Hostname' => true,
        // Location/Address
        'PostalCode' => true,
        'Latitude' => true,
        'Longitude' => true,
        'Coordinates' => true,
        'Timezone' => true,
        'LocaleCode' => true,
        'LanguageCode' => true,
        'CountryCode' => true,
        // Financial
        'IBAN' => true,
        'CUSIP' => true,
        'ISIN' => true,
        'SEDOL' => true,
        'LEI' => true,
        'MIC' => true,
        'CurrencyCode' => true,
        'Money' => true,
        'ExchangeCode' => true,
        'ExchangeRate' => true,
        'StockSymbol' => true,
        'Percentage' => true,
        // Identifiers
        'Slug' => true,
        'SemanticVersion' => true,
        'HashSHA256' => true,
        'APIKey' => true,
        'LicensePlate' => true,
        'VIN' => true,
        'TrackingNumber' => true,
        'ContainerNumber' => true,
        // Networking
        'IPAddress' => true,
        'IPv4' => true,
        'IPv6' => true,
        'MACAddress' => true,
        'CIDR' => true,
        'Port' => true,
        // Transportation
        'AirportCode' => true,
        'PortCode' => true,
        'FlightNumber' => true,
        // Content
        'Markdown' => true,
        'HTML' => true,
        'MimeType' => true,
        'Color' => true,
        'Image' => true,
        'File' => true,
        // Database
        'LTree' => true,
    ];

    /**
     * Check if a type name is a known scalar type.
     *
     * @param string $typeName The type name to check
     * @return bool True if the type is a known scalar
     */
    public static function isScalarType(string $typeName): bool
    {
        return isset(self::SCALAR_NAMES[$typeName]);
    }

    /**
     * Get all known scalar type names.
     *
     * @return list<string> Array of scalar type names
     */
    public static function getScalarNames(): array
    {
        return array_keys(self::SCALAR_NAMES);
    }
}
