package com.fraiseql.core;

import java.util.Collections;
import java.util.HashSet;
import java.util.Set;

/**
 * FraiseQL scalar type markers for schema authoring.
 *
 * <p>These are marker classes/annotations used in Java to generate the correct GraphQL
 * scalar types in schema.json. They have no runtime behavior - validation and
 * serialization happen in the Rust runtime after compilation.
 *
 * <p>Architecture:
 * <pre>
 *   Java type annotation → schema.json type string → Rust FieldType → codegen/introspection
 * </pre>
 *
 * <p>Example:
 * <pre>{@code
 * import com.fraiseql.core.*;
 *
 * @GraphQLType(name = "User")
 * public class User {
 *     @GraphQLField(type = "ID")
 *     public String id;                    // → "ID" in schema.json
 *
 *     @GraphQLField
 *     public String name;                  // → "String"
 *
 *     @GraphQLField(type = "Email")
 *     public String email;                 // → "Email"
 *
 *     @GraphQLField(type = "URL", nullable = true)
 *     public String website;               // → "URL" (nullable)
 *
 *     @GraphQLField(type = "DateTime")
 *     public String createdAt;             // → "DateTime"
 * }
 * }</pre>
 *
 * <p>FraiseQL Convention:
 * <ul>
 *   <li>{@code id} fields should ALWAYS use "ID" type (UUID v4 at runtime)</li>
 *   <li>Foreign keys (e.g., {@code authorId}) should also use "ID"</li>
 * </ul>
 *
 * <p>Custom Scalars:
 * You can define your own custom scalars by using the type parameter in @GraphQLField:
 * <pre>{@code
 * @GraphQLField(type = "MyCustomScalar")
 * public String customField;
 * }</pre>
 */
public final class Scalars {

    private Scalars() {
        // Utility class - prevent instantiation
    }

    // ==========================================================================
    // Core GraphQL Scalars
    // ==========================================================================

    /** GraphQL ID scalar - used for unique identifiers. UUID v4 at runtime. */
    public static final String ID = "ID";

    // ==========================================================================
    // Date/Time Scalars
    // ==========================================================================

    /** ISO 8601 DateTime scalar (e.g., "2025-01-10T12:00:00Z"). */
    public static final String DATE_TIME = "DateTime";

    /** ISO 8601 Date scalar (e.g., "2025-01-10"). */
    public static final String DATE = "Date";

    /** ISO 8601 Time scalar (e.g., "12:00:00"). */
    public static final String TIME = "Time";

    /** Date range scalar (e.g., "[2025-01-01,2025-12-31)"). */
    public static final String DATE_RANGE = "DateRange";

    /** ISO 8601 Duration scalar (e.g., "P1Y2M3D"). */
    public static final String DURATION = "Duration";

    // ==========================================================================
    // Complex Core Scalars
    // ==========================================================================

    /** Arbitrary JSON value scalar. Maps to PostgreSQL JSONB. */
    public static final String JSON = "Json";

    /** UUID scalar (explicit UUID type, distinct from ID). */
    public static final String UUID = "UUID";

    /** Decimal/BigDecimal scalar for precise numeric values. */
    public static final String DECIMAL = "Decimal";

    /** Vector scalar for pgvector embeddings. */
    public static final String VECTOR = "Vector";

    // ==========================================================================
    // Contact/Communication Scalars
    // ==========================================================================

    /** Email address scalar with RFC 5322 validation. */
    public static final String EMAIL = "Email";

    /** Phone number scalar (E.164 format recommended). */
    public static final String PHONE_NUMBER = "PhoneNumber";

    /** URL scalar with RFC 3986 validation. */
    public static final String URL = "URL";

    /** Domain name scalar. */
    public static final String DOMAIN_NAME = "DomainName";

    /** Hostname scalar. */
    public static final String HOSTNAME = "Hostname";

    // ==========================================================================
    // Location/Address Scalars
    // ==========================================================================

    /** Postal/ZIP code scalar. */
    public static final String POSTAL_CODE = "PostalCode";

    /** Latitude coordinate (-90 to 90). */
    public static final String LATITUDE = "Latitude";

    /** Longitude coordinate (-180 to 180). */
    public static final String LONGITUDE = "Longitude";

    /** Geographic coordinates scalar (lat,lng or GeoJSON). */
    public static final String COORDINATES = "Coordinates";

    /** IANA timezone identifier (e.g., "America/New_York"). */
    public static final String TIMEZONE = "Timezone";

    /** Locale code scalar (e.g., "en-US"). */
    public static final String LOCALE_CODE = "LocaleCode";

    /** ISO 639-1 language code (e.g., "en"). */
    public static final String LANGUAGE_CODE = "LanguageCode";

    /** ISO 3166-1 alpha-2 country code (e.g., "US"). */
    public static final String COUNTRY_CODE = "CountryCode";

    // ==========================================================================
    // Financial Scalars
    // ==========================================================================

    /** International Bank Account Number. */
    public static final String IBAN = "IBAN";

    /** CUSIP identifier for North American securities. */
    public static final String CUSIP = "CUSIP";

    /** International Securities Identification Number. */
    public static final String ISIN = "ISIN";

    /** Stock Exchange Daily Official List number. */
    public static final String SEDOL = "SEDOL";

    /** Legal Entity Identifier. */
    public static final String LEI = "LEI";

    /** Market Identifier Code. */
    public static final String MIC = "MIC";

    /** ISO 4217 currency code (e.g., "USD"). */
    public static final String CURRENCY_CODE = "CurrencyCode";

    /** Monetary amount with currency (e.g., "USD 100.00"). */
    public static final String MONEY = "Money";

    /** Stock exchange code. */
    public static final String EXCHANGE_CODE = "ExchangeCode";

    /** Currency exchange rate. */
    public static final String EXCHANGE_RATE = "ExchangeRate";

    /** Stock ticker symbol. */
    public static final String STOCK_SYMBOL = "StockSymbol";

    /** Percentage value (0-100 or 0-1 depending on context). */
    public static final String PERCENTAGE = "Percentage";

    // ==========================================================================
    // Identifier Scalars
    // ==========================================================================

    /** URL-safe slug (lowercase, hyphens, no spaces). */
    public static final String SLUG = "Slug";

    /** Semantic version string (e.g., "1.2.3"). */
    public static final String SEMANTIC_VERSION = "SemanticVersion";

    /** SHA-256 hash string (64 hex characters). */
    public static final String HASH_SHA256 = "HashSHA256";

    /** API key string. */
    public static final String API_KEY = "APIKey";

    /** Vehicle license plate number. */
    public static final String LICENSE_PLATE = "LicensePlate";

    /** Vehicle Identification Number. */
    public static final String VIN = "VIN";

    /** Shipping tracking number. */
    public static final String TRACKING_NUMBER = "TrackingNumber";

    /** Shipping container number (ISO 6346). */
    public static final String CONTAINER_NUMBER = "ContainerNumber";

    // ==========================================================================
    // Networking Scalars
    // ==========================================================================

    /** IP address (IPv4 or IPv6). */
    public static final String IP_ADDRESS = "IPAddress";

    /** IPv4 address. */
    public static final String IPV4 = "IPv4";

    /** IPv6 address. */
    public static final String IPV6 = "IPv6";

    /** MAC address. */
    public static final String MAC_ADDRESS = "MACAddress";

    /** CIDR notation for IP ranges. */
    public static final String CIDR = "CIDR";

    /** Network port number (0-65535). */
    public static final String PORT = "Port";

    // ==========================================================================
    // Transportation Scalars
    // ==========================================================================

    /** IATA airport code (e.g., "JFK"). */
    public static final String AIRPORT_CODE = "AirportCode";

    /** UN/LOCODE port code. */
    public static final String PORT_CODE = "PortCode";

    /** Flight number (e.g., "AA123"). */
    public static final String FLIGHT_NUMBER = "FlightNumber";

    // ==========================================================================
    // Content Scalars
    // ==========================================================================

    /** Markdown-formatted text. */
    public static final String MARKDOWN = "Markdown";

    /** HTML-formatted text. */
    public static final String HTML = "HTML";

    /** MIME type (e.g., "application/json"). */
    public static final String MIME_TYPE = "MimeType";

    /** Color value (hex, RGB, or named). */
    public static final String COLOR = "Color";

    /** Image reference (URL or base64). */
    public static final String IMAGE = "Image";

    /** File reference (URL or path). */
    public static final String FILE = "File";

    // ==========================================================================
    // Database/PostgreSQL Specific Scalars
    // ==========================================================================

    /** PostgreSQL ltree path (e.g., "root.child.leaf"). */
    public static final String LTREE = "LTree";

    // ==========================================================================
    // Scalar Name Registry
    // ==========================================================================

    /** Set of all known scalar type names. */
    public static final Set<String> SCALAR_NAMES;

    static {
        Set<String> names = new HashSet<>();
        // Core
        names.add("ID");
        names.add("UUID");
        names.add("Json");
        names.add("Decimal");
        names.add("Vector");
        // Date/Time
        names.add("DateTime");
        names.add("Date");
        names.add("Time");
        names.add("DateRange");
        names.add("Duration");
        // Contact/Communication
        names.add("Email");
        names.add("PhoneNumber");
        names.add("URL");
        names.add("DomainName");
        names.add("Hostname");
        // Location/Address
        names.add("PostalCode");
        names.add("Latitude");
        names.add("Longitude");
        names.add("Coordinates");
        names.add("Timezone");
        names.add("LocaleCode");
        names.add("LanguageCode");
        names.add("CountryCode");
        // Financial
        names.add("IBAN");
        names.add("CUSIP");
        names.add("ISIN");
        names.add("SEDOL");
        names.add("LEI");
        names.add("MIC");
        names.add("CurrencyCode");
        names.add("Money");
        names.add("ExchangeCode");
        names.add("ExchangeRate");
        names.add("StockSymbol");
        names.add("Percentage");
        // Identifiers
        names.add("Slug");
        names.add("SemanticVersion");
        names.add("HashSHA256");
        names.add("APIKey");
        names.add("LicensePlate");
        names.add("VIN");
        names.add("TrackingNumber");
        names.add("ContainerNumber");
        // Networking
        names.add("IPAddress");
        names.add("IPv4");
        names.add("IPv6");
        names.add("MACAddress");
        names.add("CIDR");
        names.add("Port");
        // Transportation
        names.add("AirportCode");
        names.add("PortCode");
        names.add("FlightNumber");
        // Content
        names.add("Markdown");
        names.add("HTML");
        names.add("MimeType");
        names.add("Color");
        names.add("Image");
        names.add("File");
        // Database
        names.add("LTree");

        SCALAR_NAMES = Collections.unmodifiableSet(names);
    }

    /**
     * Check if a type name is a known scalar type.
     *
     * @param typeName the type name to check
     * @return true if the type is a known scalar
     */
    public static boolean isScalarType(String typeName) {
        return SCALAR_NAMES.contains(typeName);
    }
}
