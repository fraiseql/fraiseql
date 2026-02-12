//! Extended filter operators for rich scalar types.
//!
//! This module defines specialized operators for 44 rich scalar types including
//! Email, PhoneNumber, VIN, IBAN, CountryCode, and many others.
//!
//! These operators are organized by type category and compiled conditionally
//! via feature flags to minimize binary size.

use serde::{Deserialize, Serialize};

/// Extended operators for rich scalar type filtering.
///
/// These operators enable specialized filtering on structured data types,
/// going beyond basic comparison and string matching. For example:
/// - Extract email domain and filter by it
/// - Parse VIN and filter by manufacturer
/// - Look up country membership in EU/Schengen
/// - Geospatial queries on coordinates
///
/// # Feature Gating
///
/// Operators are conditionally compiled based on feature flags:
/// - String-based types are always available (no dependencies)
/// - Lookup-based types require embedded data
/// - Advanced types (PostGIS, phone) are optional
///
/// # Example
///
/// ```rust,ignore
/// use fraiseql_core::filters::ExtendedOperator;
///
/// // Email domain extraction
/// let op = ExtendedOperator::EmailDomainEq("example.com".to_string());
///
/// // Country lookup
/// let op = ExtendedOperator::CountryInContinent("Europe".to_string());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ExtendedOperator {
    // ========================================================================
    // CONTACT/COMMUNICATION TYPES (5 types)
    // Email, PhoneNumber, URL, DomainName, Hostname
    // ========================================================================
    /// Email: Match domain part (e.g., 'example.com')
    EmailDomainEq(String),
    /// Email: Domain in list
    EmailDomainIn(Vec<String>),
    /// Email: Domain ends with suffix (e.g., '.edu')
    EmailDomainEndswith(String),
    /// Email: Local part (before @) starts with prefix
    EmailLocalPartStartswith(String),

    /// PhoneNumber: Country code matches (e.g., 'US' for +1)
    PhoneCountryCodeEq(String),
    /// PhoneNumber: Country code in list
    PhoneCountryCodeIn(Vec<String>),
    /// PhoneNumber: Is valid E.164 format
    PhoneIsValid(bool),
    /// PhoneNumber: Type equals (mobile, fixed, etc.)
    PhoneTypeEq(String),

    /// URL: Protocol matches (http, https, etc.)
    UrlProtocolEq(String),
    /// URL: Host matches
    UrlHostEq(String),
    /// URL: Path starts with
    UrlPathStartswith(String),

    /// DomainName: TLD matches
    DomainNameTldEq(String),
    /// DomainName: TLD in list
    DomainNameTldIn(Vec<String>),

    /// Hostname: Is fully qualified domain name (FQDN)
    HostnameIsFqdn(bool),
    /// Hostname: Depth equals (number of labels)
    HostnameDepthEq(u32),

    // ========================================================================
    // LOCATION/ADDRESS TYPES (8 types)
    // PostalCode, Latitude, Longitude, Coordinates, Timezone, LocaleCode,
    // LanguageCode, CountryCode
    // ========================================================================
    /// PostalCode: Country code matches
    PostalCodeCountryEq(String),
    /// PostalCode: Format valid for country
    PostalCodeFormatValidForCountry(String),

    /// Latitude: Within range (degrees)
    LatitudeWithinRange { min: f64, max: f64 },
    /// Latitude: Hemisphere (North or South)
    LatitudeHemisphereEq(String),

    /// Longitude: Within range (degrees)
    LongitudeWithinRange { min: f64, max: f64 },
    /// Longitude: Hemisphere (East or West)
    LongitudeHemisphereEq(String),

    /// Coordinates: Distance within radius (km)
    CoordinatesDistanceWithin {
        lat:       f64,
        lng:       f64,
        radius_km: f64,
    },
    /// Coordinates: Within bounding box
    CoordinatesWithinBoundingBox {
        north: f64,
        south: f64,
        east:  f64,
        west:  f64,
    },
    /// Coordinates: Within polygon (list of lat/lng pairs)
    CoordinatesWithinPolygon(Vec<(f64, f64)>),

    /// Timezone: UTC offset matches (in minutes, e.g., -300 for EST)
    TimezoneOffsetEq(i32),
    /// Timezone: Has daylight saving time
    TimezoneHasDst(bool),
    /// Timezone: Region/continent matches
    TimezoneRegionEq(String),

    /// LocaleCode: Language part matches
    LocaleCodeLanguageEq(String),
    /// LocaleCode: Country part matches
    LocaleCodeCountryEq(String),
    /// LocaleCode: Script matches (Hans, Hant, etc.)
    LocaleCodeScriptEq(String),

    /// LanguageCode: Language family matches (Indo-European, Sino-Tibetan, etc.)
    LanguageCodeFamilyEq(String),

    /// CountryCode: Continent matches
    CountryCodeContinentEq(String),
    /// CountryCode: Region matches (UN geographic region)
    CountryCodeRegionEq(String),
    /// CountryCode: Is EU member
    CountryCodeInEu(bool),
    /// CountryCode: Is Schengen member
    CountryCodeInSchengen(bool),

    // ========================================================================
    // FINANCIAL TYPES (11 types)
    // IBAN, CUSIP, ISIN, SEDOL, LEI, MIC, CurrencyCode, Money,
    // ExchangeCode, ExchangeRate, StockSymbol
    // ========================================================================
    /// IBAN: Country code matches
    IbanCountryEq(String),
    /// IBAN: Country code in list
    IbanCountryIn(Vec<String>),
    /// IBAN: Is valid (mod-97 checksum)
    IbanIsValid(bool),

    /// CUSIP: Issuer type matches (equity, bond, etc.)
    CusipIssuerTypeEq(String),

    /// ISIN: Country matches
    IsinCountryEq(String),
    /// ISIN: Asset class matches (equity, bond, fund, etc.)
    IsinAssetClassEq(String),

    /// SEDOL: Country matches
    SedolCountryEq(String),

    /// LEI: Entity category matches
    LeiEntityCategoryEq(String),

    /// MIC: Country matches
    MicCountryEq(String),
    /// MIC: Segment matches
    MicSegmentEq(String),

    /// CurrencyCode: Region matches
    CurrencyCodeRegionEq(String),
    /// CurrencyCode: Decimal places equals
    CurrencyCodeDecimalPlacesEq(u32),

    /// Money: Currency code matches
    MoneyInCurrency(String),

    /// ExchangeCode: Country matches
    ExchangeCodeCountryEq(String),

    /// ExchangeRate: Currency pair matches
    ExchangeRateCurrencyPairEq(String),

    /// StockSymbol: Exchange matches (NYSE, NASDAQ, etc.)
    StockSymbolExchangeEq(String),
    /// StockSymbol: Sector matches
    StockSymbolSectorEq(String),

    // ========================================================================
    // IDENTIFIER TYPES (8 types)
    // ========================================================================
    // Slug, SemanticVersion, HashSHA256, APIKey, LicensePlate,
    // VIN, TrackingNumber, ContainerNumber
    /// Slug: Depth equals (number of segments)
    SlugDepthEq(u32),
    /// Slug: Segment matches
    SlugSegmentEq(String),

    /// SemanticVersion: Major version equals
    SemanticVersionMajorEq(u32),
    /// SemanticVersion: Minor version equals
    SemanticVersionMinorEq(u32),
    /// SemanticVersion: Patch version equals
    SemanticVersionPatchEq(u32),
    /// SemanticVersion: Has prerelease
    SemanticVersionHasPrerelease(bool),

    /// HashSHA256: Length equals (always 64 hex chars)
    HashSha256LengthEq(u32),

    /// APIKey: Length equals
    ApiKeyLengthEq(u32),
    /// APIKey: Prefix matches
    ApiKeyPrefixEq(String),

    /// LicensePlate: Country matches
    LicensePlateCountryEq(String),
    /// LicensePlate: Format valid for country
    LicensePlateFormatValidForCountry(String),

    /// VIN: World Manufacturer Identifier (WMI) matches
    VinWmiEq(String),
    /// VIN: WMI in list
    VinWmiIn(Vec<String>),
    /// VIN: Country (first character) matches
    VinCountryEq(String),
    /// VIN: Model year equals
    VinModelYearEq(i32),
    /// VIN: Is valid (checksum)
    VinIsValid(bool),

    /// TrackingNumber: Carrier matches
    TrackingNumberCarrierEq(String),
    /// TrackingNumber: Format valid for carrier
    TrackingNumberFormatValidForCarrier(String),

    /// ContainerNumber: Owner code matches
    ContainerNumberOwnerEq(String),
    /// ContainerNumber: Is valid (ISO 6346 checksum)
    ContainerNumberIsValid(bool),

    // ========================================================================
    // NETWORKING TYPES (6 types)
    // ========================================================================
    // IPAddress, IPv4, IPv6, MACAddress, CIDR, Port
    /// IPAddress: Version equals (4 or 6)
    IpAddressVersionEq(u8),
    /// IPAddress: Is private (RFC 1918)
    IpAddressIsPrivate(bool),

    /// IPv4: CIDR range contains
    Ipv4CidrContains(String),
    /// IPv4: Is multicast
    Ipv4IsMulticast(bool),
    /// IPv4: Is reserved
    Ipv4IsReserved(bool),

    /// IPv6: CIDR range contains
    Ipv6CidrContains(String),
    /// IPv6: Is multicast
    Ipv6IsMulticast(bool),

    /// MACAddress: Vendor code (OUI) matches
    MacAddressVendorEq(String),
    /// MACAddress: OUI in list
    MacAddressOuiIn(Vec<String>),
    /// MACAddress: Is unicast
    MacAddressIsUnicast(bool),

    /// CIDR: Overlaps with
    CidrOverlapsWith(String),
    /// CIDR: Contains IP
    CidrContainsIp(String),
    /// CIDR: Version equals
    CidrVersionEq(u8),

    /// Port: Service name matches
    PortServiceEq(String),
    /// Port: Is well-known (0-1023)
    PortIsWellKnown(bool),
    /// Port: Is registered (1024-49151)
    PortIsRegistered(bool),

    // ========================================================================
    // TRANSPORTATION TYPES (3 types)
    // ========================================================================
    // AirportCode, PortCode, FlightNumber
    /// AirportCode: Country matches
    AirportCodeCountryEq(String),
    /// AirportCode: Is major airport
    AirportCodeIsMajor(bool),

    /// PortCode: Country matches
    PortCodeCountryEq(String),

    /// FlightNumber: Airline code matches
    FlightNumberAirlineEq(String),
    /// FlightNumber: Aircraft type matches
    FlightNumberAircraftTypeEq(String),

    // ========================================================================
    // CONTENT TYPES (6 types)
    // ========================================================================
    // Markdown, HTML, MimeType, Color, Image, File
    /// Markdown: Is valid CommonMark
    MarkdownIsValid(bool),

    /// HTML: Is valid HTML5
    HtmlIsValid(bool),
    /// HTML: Contains tag
    HtmlContainsTag(String),

    /// MimeType: Type part matches (e.g., 'image')
    MimeTypeTypeEq(String),
    /// MimeType: Subtype matches (e.g., 'png')
    MimeTypeSubtypeEq(String),
    /// MimeType: Charset matches
    MimeTypeCharsetEq(String),

    /// Color: Hex value matches
    ColorHexEq(String),
    /// Color: RGB in range
    ColorRgbInRange {
        r: (u8, u8),
        g: (u8, u8),
        b: (u8, u8),
    },
    /// Color: HSL in range
    ColorHslInRange {
        h: (u32, u32),
        s: (u8, u8),
        l: (u8, u8),
    },

    /// Image: Format matches (jpeg, png, etc.)
    ImageFormatEq(String),
    /// Image: Width >= min
    ImageWidthGte(u32),
    /// Image: Height >= min
    ImageHeightGte(u32),
    /// Image: Size <= max (bytes)
    ImageSizeLte(u64),

    /// File: Extension matches
    FileExtensionEq(String),
    /// File: MIME type matches
    FileMimeTypeEq(String),
    /// File: Size <= max (bytes)
    FileSizeLte(u64),

    // ========================================================================
    // DATABASE/POSTGRESQL-SPECIFIC TYPES (1 type)
    // ========================================================================
    // LTree
    /// LTree: Depth equals
    LtreeDepthEq(u32),
    /// LTree: Ancestor matches
    LtreeAncestorEq(String),
    /// LTree: Descendant matches
    LtreeDescendantEq(String),

    // ========================================================================
    // RANGE TYPES (3 types)
    // ========================================================================
    // DateRange, Duration, Percentage
    /// DateRange: Duration >= min days
    DateRangeDurationGte(u32),
    /// DateRange: Starts after date
    DateRangeStartsAfter(String),
    /// DateRange: Ends before date
    DateRangeEndsBefore(String),

    /// Duration: Total seconds equals
    DurationTotalSecondsEq(u64),
    /// Duration: Total minutes >= min
    DurationTotalMinutesGte(u64),

    /// Percentage: Value in range (0-100)
    PercentageInRange { min: f32, max: f32 },
    /// Percentage: Percentile matches
    PercentagePercentileEq(f32),
}

impl std::fmt::Display for ExtendedOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Contact/Communication
            ExtendedOperator::EmailDomainEq(_) => write!(f, "email_domain_eq"),
            ExtendedOperator::EmailDomainIn(_) => write!(f, "email_domain_in"),
            ExtendedOperator::EmailDomainEndswith(_) => write!(f, "email_domain_endswith"),
            ExtendedOperator::EmailLocalPartStartswith(_) => {
                write!(f, "email_local_part_startswith")
            },
            ExtendedOperator::PhoneCountryCodeEq(_) => write!(f, "phone_country_code_eq"),
            ExtendedOperator::PhoneCountryCodeIn(_) => write!(f, "phone_country_code_in"),
            ExtendedOperator::PhoneIsValid(_) => write!(f, "phone_is_valid"),
            ExtendedOperator::PhoneTypeEq(_) => write!(f, "phone_type_eq"),
            ExtendedOperator::UrlProtocolEq(_) => write!(f, "url_protocol_eq"),
            ExtendedOperator::UrlHostEq(_) => write!(f, "url_host_eq"),
            ExtendedOperator::UrlPathStartswith(_) => write!(f, "url_path_startswith"),
            ExtendedOperator::DomainNameTldEq(_) => write!(f, "domain_name_tld_eq"),
            ExtendedOperator::DomainNameTldIn(_) => write!(f, "domain_name_tld_in"),
            ExtendedOperator::HostnameIsFqdn(_) => write!(f, "hostname_is_fqdn"),
            ExtendedOperator::HostnameDepthEq(_) => write!(f, "hostname_depth_eq"),

            // Location/Address
            ExtendedOperator::PostalCodeCountryEq(_) => write!(f, "postal_code_country_eq"),
            ExtendedOperator::PostalCodeFormatValidForCountry(_) => {
                write!(f, "postal_code_format_valid_for_country")
            },
            ExtendedOperator::LatitudeWithinRange { .. } => write!(f, "latitude_within_range"),
            ExtendedOperator::LatitudeHemisphereEq(_) => write!(f, "latitude_hemisphere_eq"),
            ExtendedOperator::LongitudeWithinRange { .. } => write!(f, "longitude_within_range"),
            ExtendedOperator::LongitudeHemisphereEq(_) => write!(f, "longitude_hemisphere_eq"),
            ExtendedOperator::CoordinatesDistanceWithin { .. } => {
                write!(f, "coordinates_distance_within")
            },
            ExtendedOperator::CoordinatesWithinBoundingBox { .. } => {
                write!(f, "coordinates_within_bounding_box")
            },
            ExtendedOperator::CoordinatesWithinPolygon(_) => {
                write!(f, "coordinates_within_polygon")
            },
            ExtendedOperator::TimezoneOffsetEq(_) => write!(f, "timezone_offset_eq"),
            ExtendedOperator::TimezoneHasDst(_) => write!(f, "timezone_has_dst"),
            ExtendedOperator::TimezoneRegionEq(_) => write!(f, "timezone_region_eq"),
            ExtendedOperator::LocaleCodeLanguageEq(_) => write!(f, "locale_code_language_eq"),
            ExtendedOperator::LocaleCodeCountryEq(_) => write!(f, "locale_code_country_eq"),
            ExtendedOperator::LocaleCodeScriptEq(_) => write!(f, "locale_code_script_eq"),
            ExtendedOperator::LanguageCodeFamilyEq(_) => write!(f, "language_code_family_eq"),
            ExtendedOperator::CountryCodeContinentEq(_) => {
                write!(f, "country_code_continent_eq")
            },
            ExtendedOperator::CountryCodeRegionEq(_) => write!(f, "country_code_region_eq"),
            ExtendedOperator::CountryCodeInEu(_) => write!(f, "country_code_in_eu"),
            ExtendedOperator::CountryCodeInSchengen(_) => {
                write!(f, "country_code_in_schengen")
            },

            // Financial
            ExtendedOperator::IbanCountryEq(_) => write!(f, "iban_country_eq"),
            ExtendedOperator::IbanCountryIn(_) => write!(f, "iban_country_in"),
            ExtendedOperator::IbanIsValid(_) => write!(f, "iban_is_valid"),
            ExtendedOperator::CusipIssuerTypeEq(_) => write!(f, "cusip_issuer_type_eq"),
            ExtendedOperator::IsinCountryEq(_) => write!(f, "isin_country_eq"),
            ExtendedOperator::IsinAssetClassEq(_) => write!(f, "isin_asset_class_eq"),
            ExtendedOperator::SedolCountryEq(_) => write!(f, "sedol_country_eq"),
            ExtendedOperator::LeiEntityCategoryEq(_) => write!(f, "lei_entity_category_eq"),
            ExtendedOperator::MicCountryEq(_) => write!(f, "mic_country_eq"),
            ExtendedOperator::MicSegmentEq(_) => write!(f, "mic_segment_eq"),
            ExtendedOperator::CurrencyCodeRegionEq(_) => {
                write!(f, "currency_code_region_eq")
            },
            ExtendedOperator::CurrencyCodeDecimalPlacesEq(_) => {
                write!(f, "currency_code_decimal_places_eq")
            },
            ExtendedOperator::MoneyInCurrency(_) => write!(f, "money_in_currency"),
            ExtendedOperator::ExchangeCodeCountryEq(_) => {
                write!(f, "exchange_code_country_eq")
            },
            ExtendedOperator::ExchangeRateCurrencyPairEq(_) => {
                write!(f, "exchange_rate_currency_pair_eq")
            },
            ExtendedOperator::StockSymbolExchangeEq(_) => {
                write!(f, "stock_symbol_exchange_eq")
            },
            ExtendedOperator::StockSymbolSectorEq(_) => write!(f, "stock_symbol_sector_eq"),

            // Identifiers
            ExtendedOperator::SlugDepthEq(_) => write!(f, "slug_depth_eq"),
            ExtendedOperator::SlugSegmentEq(_) => write!(f, "slug_segment_eq"),
            ExtendedOperator::SemanticVersionMajorEq(_) => {
                write!(f, "semantic_version_major_eq")
            },
            ExtendedOperator::SemanticVersionMinorEq(_) => {
                write!(f, "semantic_version_minor_eq")
            },
            ExtendedOperator::SemanticVersionPatchEq(_) => {
                write!(f, "semantic_version_patch_eq")
            },
            ExtendedOperator::SemanticVersionHasPrerelease(_) => {
                write!(f, "semantic_version_has_prerelease")
            },
            ExtendedOperator::HashSha256LengthEq(_) => write!(f, "hash_sha256_length_eq"),
            ExtendedOperator::ApiKeyLengthEq(_) => write!(f, "api_key_length_eq"),
            ExtendedOperator::ApiKeyPrefixEq(_) => write!(f, "api_key_prefix_eq"),
            ExtendedOperator::LicensePlateCountryEq(_) => {
                write!(f, "license_plate_country_eq")
            },
            ExtendedOperator::LicensePlateFormatValidForCountry(_) => {
                write!(f, "license_plate_format_valid_for_country")
            },
            ExtendedOperator::VinWmiEq(_) => write!(f, "vin_wmi_eq"),
            ExtendedOperator::VinWmiIn(_) => write!(f, "vin_wmi_in"),
            ExtendedOperator::VinCountryEq(_) => write!(f, "vin_country_eq"),
            ExtendedOperator::VinModelYearEq(_) => write!(f, "vin_model_year_eq"),
            ExtendedOperator::VinIsValid(_) => write!(f, "vin_is_valid"),
            ExtendedOperator::TrackingNumberCarrierEq(_) => {
                write!(f, "tracking_number_carrier_eq")
            },
            ExtendedOperator::TrackingNumberFormatValidForCarrier(_) => {
                write!(f, "tracking_number_format_valid_for_carrier")
            },
            ExtendedOperator::ContainerNumberOwnerEq(_) => {
                write!(f, "container_number_owner_eq")
            },
            ExtendedOperator::ContainerNumberIsValid(_) => {
                write!(f, "container_number_is_valid")
            },

            // Networking
            ExtendedOperator::IpAddressVersionEq(_) => write!(f, "ip_address_version_eq"),
            ExtendedOperator::IpAddressIsPrivate(_) => write!(f, "ip_address_is_private"),
            ExtendedOperator::Ipv4CidrContains(_) => write!(f, "ipv4_cidr_contains"),
            ExtendedOperator::Ipv4IsMulticast(_) => write!(f, "ipv4_is_multicast"),
            ExtendedOperator::Ipv4IsReserved(_) => write!(f, "ipv4_is_reserved"),
            ExtendedOperator::Ipv6CidrContains(_) => write!(f, "ipv6_cidr_contains"),
            ExtendedOperator::Ipv6IsMulticast(_) => write!(f, "ipv6_is_multicast"),
            ExtendedOperator::MacAddressVendorEq(_) => write!(f, "mac_address_vendor_eq"),
            ExtendedOperator::MacAddressOuiIn(_) => write!(f, "mac_address_oui_in"),
            ExtendedOperator::MacAddressIsUnicast(_) => write!(f, "mac_address_is_unicast"),
            ExtendedOperator::CidrOverlapsWith(_) => write!(f, "cidr_overlaps_with"),
            ExtendedOperator::CidrContainsIp(_) => write!(f, "cidr_contains_ip"),
            ExtendedOperator::CidrVersionEq(_) => write!(f, "cidr_version_eq"),
            ExtendedOperator::PortServiceEq(_) => write!(f, "port_service_eq"),
            ExtendedOperator::PortIsWellKnown(_) => write!(f, "port_is_well_known"),
            ExtendedOperator::PortIsRegistered(_) => write!(f, "port_is_registered"),

            // Transportation
            ExtendedOperator::AirportCodeCountryEq(_) => {
                write!(f, "airport_code_country_eq")
            },
            ExtendedOperator::AirportCodeIsMajor(_) => write!(f, "airport_code_is_major"),
            ExtendedOperator::PortCodeCountryEq(_) => write!(f, "port_code_country_eq"),
            ExtendedOperator::FlightNumberAirlineEq(_) => {
                write!(f, "flight_number_airline_eq")
            },
            ExtendedOperator::FlightNumberAircraftTypeEq(_) => {
                write!(f, "flight_number_aircraft_type_eq")
            },

            // Content
            ExtendedOperator::MarkdownIsValid(_) => write!(f, "markdown_is_valid"),
            ExtendedOperator::HtmlIsValid(_) => write!(f, "html_is_valid"),
            ExtendedOperator::HtmlContainsTag(_) => write!(f, "html_contains_tag"),
            ExtendedOperator::MimeTypeTypeEq(_) => write!(f, "mime_type_type_eq"),
            ExtendedOperator::MimeTypeSubtypeEq(_) => write!(f, "mime_type_subtype_eq"),
            ExtendedOperator::MimeTypeCharsetEq(_) => write!(f, "mime_type_charset_eq"),
            ExtendedOperator::ColorHexEq(_) => write!(f, "color_hex_eq"),
            ExtendedOperator::ColorRgbInRange { .. } => write!(f, "color_rgb_in_range"),
            ExtendedOperator::ColorHslInRange { .. } => write!(f, "color_hsl_in_range"),
            ExtendedOperator::ImageFormatEq(_) => write!(f, "image_format_eq"),
            ExtendedOperator::ImageWidthGte(_) => write!(f, "image_width_gte"),
            ExtendedOperator::ImageHeightGte(_) => write!(f, "image_height_gte"),
            ExtendedOperator::ImageSizeLte(_) => write!(f, "image_size_lte"),
            ExtendedOperator::FileExtensionEq(_) => write!(f, "file_extension_eq"),
            ExtendedOperator::FileMimeTypeEq(_) => write!(f, "file_mime_type_eq"),
            ExtendedOperator::FileSizeLte(_) => write!(f, "file_size_lte"),

            // Database
            ExtendedOperator::LtreeDepthEq(_) => write!(f, "ltree_depth_eq"),
            ExtendedOperator::LtreeAncestorEq(_) => write!(f, "ltree_ancestor_eq"),
            ExtendedOperator::LtreeDescendantEq(_) => write!(f, "ltree_descendant_eq"),

            // Ranges
            ExtendedOperator::DateRangeDurationGte(_) => {
                write!(f, "date_range_duration_gte")
            },
            ExtendedOperator::DateRangeStartsAfter(_) => {
                write!(f, "date_range_starts_after")
            },
            ExtendedOperator::DateRangeEndsBefore(_) => {
                write!(f, "date_range_ends_before")
            },
            ExtendedOperator::DurationTotalSecondsEq(_) => {
                write!(f, "duration_total_seconds_eq")
            },
            ExtendedOperator::DurationTotalMinutesGte(_) => {
                write!(f, "duration_total_minutes_gte")
            },
            ExtendedOperator::PercentageInRange { .. } => write!(f, "percentage_in_range"),
            ExtendedOperator::PercentagePercentileEq(_) => {
                write!(f, "percentage_percentile_eq")
            },
        }
    }
}

impl ExtendedOperator {
    /// Get the camelCase template name for this operator.
    ///
    /// Converts snake_case display name to camelCase for template lookup.
    /// Example: "email_domain_eq" -> "domainEq"
    pub fn template_name(&self) -> String {
        // Map operator variants to template names
        match self {
            // Email operators
            ExtendedOperator::EmailDomainEq(_) => "domainEq".to_string(),
            ExtendedOperator::EmailDomainIn(_) => "domainIn".to_string(),
            ExtendedOperator::EmailDomainEndswith(_) => "domainEndswith".to_string(),
            ExtendedOperator::EmailLocalPartStartswith(_) => "localPartStartswith".to_string(),

            // Phone operators
            ExtendedOperator::PhoneCountryCodeEq(_) => "countryCodeEq".to_string(),
            ExtendedOperator::PhoneCountryCodeIn(_) => "countryCodeIn".to_string(),
            ExtendedOperator::PhoneIsValid(_) => "isValid".to_string(),
            ExtendedOperator::PhoneTypeEq(_) => "typeEq".to_string(),

            // URL operators
            ExtendedOperator::UrlProtocolEq(_) => "protocolEq".to_string(),
            ExtendedOperator::UrlHostEq(_) => "hostEq".to_string(),
            ExtendedOperator::UrlPathStartswith(_) => "pathStartswith".to_string(),

            // Domain/Hostname operators
            ExtendedOperator::DomainNameTldEq(_) => "tldEq".to_string(),
            ExtendedOperator::DomainNameTldIn(_) => "tldIn".to_string(),
            ExtendedOperator::HostnameIsFqdn(_) => "isFqdn".to_string(),
            ExtendedOperator::HostnameDepthEq(_) => "depthEq".to_string(),

            // Location operators
            ExtendedOperator::PostalCodeCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::PostalCodeFormatValidForCountry(_) => "formatValidForCountry".to_string(),
            ExtendedOperator::LatitudeWithinRange { .. } => "withinRange".to_string(),
            ExtendedOperator::LatitudeHemisphereEq(_) => "hemisphereEq".to_string(),
            ExtendedOperator::LongitudeWithinRange { .. } => "withinRange".to_string(),
            ExtendedOperator::LongitudeHemisphereEq(_) => "hemisphereEq".to_string(),

            // Coordinate operators
            ExtendedOperator::CoordinatesDistanceWithin { .. } => "distanceWithin".to_string(),
            ExtendedOperator::CoordinatesWithinBoundingBox { .. } => "withinBoundingBox".to_string(),
            ExtendedOperator::CoordinatesWithinPolygon(_) => "withinPolygon".to_string(),

            // Timezone operators
            ExtendedOperator::TimezoneOffsetEq(_) => "offsetEq".to_string(),
            ExtendedOperator::TimezoneHasDst(_) => "hasDst".to_string(),
            ExtendedOperator::TimezoneRegionEq(_) => "regionEq".to_string(),

            // Locale/Language operators
            ExtendedOperator::LocaleCodeLanguageEq(_) => "languageEq".to_string(),
            ExtendedOperator::LocaleCodeCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::LocaleCodeScriptEq(_) => "scriptEq".to_string(),
            ExtendedOperator::LanguageCodeFamilyEq(_) => "familyEq".to_string(),

            // Country operators
            ExtendedOperator::CountryCodeContinentEq(_) => "continentEq".to_string(),
            ExtendedOperator::CountryCodeRegionEq(_) => "regionEq".to_string(),
            ExtendedOperator::CountryCodeInEu(_) => "inEu".to_string(),
            ExtendedOperator::CountryCodeInSchengen(_) => "inSchengen".to_string(),

            // Financial operators
            ExtendedOperator::IbanCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::IbanCountryIn(_) => "countryIn".to_string(),
            ExtendedOperator::IbanIsValid(_) => "isValid".to_string(),
            ExtendedOperator::CusipIssuerTypeEq(_) => "issuerTypeEq".to_string(),
            ExtendedOperator::IsinCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::IsinAssetClassEq(_) => "assetClassEq".to_string(),
            ExtendedOperator::SedolCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::LeiEntityCategoryEq(_) => "entityCategoryEq".to_string(),
            ExtendedOperator::MicCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::MicSegmentEq(_) => "segmentEq".to_string(),
            ExtendedOperator::CurrencyCodeRegionEq(_) => "regionEq".to_string(),
            ExtendedOperator::CurrencyCodeDecimalPlacesEq(_) => "decimalPlacesEq".to_string(),
            ExtendedOperator::MoneyInCurrency(_) => "inCurrency".to_string(),
            ExtendedOperator::ExchangeCodeCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::ExchangeRateCurrencyPairEq(_) => "currencyPairEq".to_string(),
            ExtendedOperator::StockSymbolExchangeEq(_) => "exchangeEq".to_string(),
            ExtendedOperator::StockSymbolSectorEq(_) => "sectorEq".to_string(),

            // Identifier operators
            ExtendedOperator::SlugDepthEq(_) => "depthEq".to_string(),
            ExtendedOperator::SlugSegmentEq(_) => "segmentEq".to_string(),
            ExtendedOperator::SemanticVersionMajorEq(_) => "majorEq".to_string(),
            ExtendedOperator::SemanticVersionMinorEq(_) => "minorEq".to_string(),
            ExtendedOperator::SemanticVersionPatchEq(_) => "patchEq".to_string(),
            ExtendedOperator::SemanticVersionHasPrerelease(_) => "hasPrerelease".to_string(),
            ExtendedOperator::HashSha256LengthEq(_) => "lengthEq".to_string(),
            ExtendedOperator::ApiKeyLengthEq(_) => "lengthEq".to_string(),
            ExtendedOperator::ApiKeyPrefixEq(_) => "prefixEq".to_string(),
            ExtendedOperator::LicensePlateCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::LicensePlateFormatValidForCountry(_) => "formatValidForCountry".to_string(),

            // VIN operators
            ExtendedOperator::VinWmiEq(_) => "wmiEq".to_string(),
            ExtendedOperator::VinWmiIn(_) => "wmiIn".to_string(),
            ExtendedOperator::VinCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::VinModelYearEq(_) => "modelYearEq".to_string(),
            ExtendedOperator::VinIsValid(_) => "isValid".to_string(),

            // Tracking/Container operators
            ExtendedOperator::TrackingNumberCarrierEq(_) => "carrierEq".to_string(),
            ExtendedOperator::TrackingNumberFormatValidForCarrier(_) => "formatValidForCarrier".to_string(),
            ExtendedOperator::ContainerNumberOwnerEq(_) => "ownerEq".to_string(),
            ExtendedOperator::ContainerNumberIsValid(_) => "isValid".to_string(),

            // Networking operators
            ExtendedOperator::IpAddressVersionEq(_) => "versionEq".to_string(),
            ExtendedOperator::IpAddressIsPrivate(_) => "isPrivate".to_string(),
            ExtendedOperator::Ipv4CidrContains(_) => "cidrContains".to_string(),
            ExtendedOperator::Ipv4IsMulticast(_) => "isMulticast".to_string(),
            ExtendedOperator::Ipv4IsReserved(_) => "isReserved".to_string(),
            ExtendedOperator::Ipv6CidrContains(_) => "cidrContains".to_string(),
            ExtendedOperator::Ipv6IsMulticast(_) => "isMulticast".to_string(),
            ExtendedOperator::MacAddressVendorEq(_) => "vendorEq".to_string(),
            ExtendedOperator::MacAddressOuiIn(_) => "ouiIn".to_string(),
            ExtendedOperator::MacAddressIsUnicast(_) => "isUnicast".to_string(),
            ExtendedOperator::CidrOverlapsWith(_) => "overlapsWith".to_string(),
            ExtendedOperator::CidrContainsIp(_) => "containsIp".to_string(),
            ExtendedOperator::CidrVersionEq(_) => "versionEq".to_string(),
            ExtendedOperator::PortServiceEq(_) => "serviceEq".to_string(),
            ExtendedOperator::PortIsWellKnown(_) => "isWellKnown".to_string(),
            ExtendedOperator::PortIsRegistered(_) => "isRegistered".to_string(),

            // Transportation operators
            ExtendedOperator::AirportCodeCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::AirportCodeIsMajor(_) => "isMajor".to_string(),
            ExtendedOperator::PortCodeCountryEq(_) => "countryEq".to_string(),
            ExtendedOperator::FlightNumberAirlineEq(_) => "airlineEq".to_string(),
            ExtendedOperator::FlightNumberAircraftTypeEq(_) => "aircraftTypeEq".to_string(),

            // Content operators
            ExtendedOperator::MarkdownIsValid(_) => "isValid".to_string(),
            ExtendedOperator::HtmlIsValid(_) => "isValid".to_string(),
            ExtendedOperator::HtmlContainsTag(_) => "containsTag".to_string(),
            ExtendedOperator::MimeTypeTypeEq(_) => "typeEq".to_string(),
            ExtendedOperator::MimeTypeSubtypeEq(_) => "subtypeEq".to_string(),
            ExtendedOperator::MimeTypeCharsetEq(_) => "charsetEq".to_string(),
            ExtendedOperator::ColorHexEq(_) => "hexEq".to_string(),
            ExtendedOperator::ColorRgbInRange { .. } => "rgbInRange".to_string(),
            ExtendedOperator::ColorHslInRange { .. } => "hslInRange".to_string(),
            ExtendedOperator::ImageFormatEq(_) => "formatEq".to_string(),
            ExtendedOperator::ImageWidthGte(_) => "widthGte".to_string(),
            ExtendedOperator::ImageHeightGte(_) => "heightGte".to_string(),
            ExtendedOperator::ImageSizeLte(_) => "sizeLte".to_string(),
            ExtendedOperator::FileExtensionEq(_) => "extensionEq".to_string(),
            ExtendedOperator::FileMimeTypeEq(_) => "mimeTypeEq".to_string(),
            ExtendedOperator::FileSizeLte(_) => "sizeLte".to_string(),

            // LTree operators
            ExtendedOperator::LtreeDepthEq(_) => "depthEq".to_string(),
            ExtendedOperator::LtreeAncestorEq(_) => "ancestorEq".to_string(),
            ExtendedOperator::LtreeDescendantEq(_) => "descendantEq".to_string(),

            // Range operators
            ExtendedOperator::DateRangeDurationGte(_) => "durationGte".to_string(),
            ExtendedOperator::DateRangeStartsAfter(_) => "startsAfter".to_string(),
            ExtendedOperator::DateRangeEndsBefore(_) => "endsBefore".to_string(),
            ExtendedOperator::DurationTotalSecondsEq(_) => "totalSecondsEq".to_string(),
            ExtendedOperator::DurationTotalMinutesGte(_) => "totalMinutesGte".to_string(),
            ExtendedOperator::PercentageInRange { .. } => "inRange".to_string(),
            ExtendedOperator::PercentagePercentileEq(_) => "percentileEq".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extended_operator_display() {
        let op = ExtendedOperator::EmailDomainEq("example.com".to_string());
        assert_eq!(op.to_string(), "email_domain_eq");

        let op = ExtendedOperator::CountryCodeInEu(true);
        assert_eq!(op.to_string(), "country_code_in_eu");

        let op = ExtendedOperator::VinWmiEq("1HG".to_string());
        assert_eq!(op.to_string(), "vin_wmi_eq");
    }

    #[test]
    fn test_extended_operator_serialization() {
        let op = ExtendedOperator::EmailDomainEq("example.com".to_string());
        let json = serde_json::to_string(&op).unwrap();
        let deserialized: ExtendedOperator = serde_json::from_str(&json).unwrap();
        assert_eq!(op, deserialized);
    }
}
