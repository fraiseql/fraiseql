# Feature Request: Specialized Filter Operators for Rich Scalar Types

## Summary

FraiseQL has 47+ rich scalar types (Email, VIN, IBAN, PhoneNumber, URL, etc.) that currently use basic `StringFilter` operators (`eq`, `neq`, `in`, `nin`, `is_null`). These types have domain-specific structure that could enable powerful specialized filtering.

This issue proposes adding specialized filter types for rich scalars, similar to how `NetworkAddressFilter`, `LTreeFilter`, and `VectorFilter` already provide domain-specific operators.

## Motivation

Consider filtering users by email domain:

```graphql
# Current: Must use generic string matching
users(where: { email: { _ilike: "%@company.com" } })

# Proposed: Domain-aware filtering
users(where: { email: { _domain_eq: "company.com" } })
```

The proposed approach is:
- **Type-safe**: Invalid domains rejected at parse time
- **Optimized**: Can use functional indexes on extracted components
- **Intuitive**: Matches how developers think about the data

---

## Proposed Filter Types

### 1. Contact & Communication

#### EmailFilter

```graphql
input EmailFilter {
  # Basic (existing)
  _eq: Email
  _neq: Email
  _in: [Email!]
  _nin: [Email!]
  _is_null: Boolean

  # Domain extraction
  _domain_eq: String           # SPLIT_PART(email, '@', 2) = 'company.com'
  _domain_neq: String
  _domain_in: [String!]        # Domain IN ('company.com', 'corp.com')
  _domain_nin: [String!]
  _domain_endswith: String     # Domain LIKE '%.edu'
  _domain_contains: String     # Domain LIKE '%google%'

  # Local part extraction
  _local_eq: String            # SPLIT_PART(email, '@', 1) = 'john.doe'
  _local_startswith: String    # Local part LIKE 'sales.%'
  _local_contains: String

  # Pattern detection
  _is_freemail: Boolean        # Domain IN (gmail, yahoo, hotmail, ...)
  _is_corporate: Boolean       # NOT freemail
  _is_disposable: Boolean      # Known disposable email domains
}
```

**SQL Implementation:**
```sql
-- Functional index for domain queries
CREATE INDEX idx_user_email_domain ON tb_user (SPLIT_PART(email, '@', 2));

-- Filter: _domain_eq: "company.com"
WHERE SPLIT_PART(data->>'email', '@', 2) = 'company.com'
```

#### PhoneNumberFilter

```graphql
input PhoneNumberFilter {
  # Basic
  _eq: PhoneNumber
  _neq: PhoneNumber
  _in: [PhoneNumber!]
  _nin: [PhoneNumber!]
  _is_null: Boolean

  # Country code extraction (E.164 format: +{country}{number})
  _country_code_eq: String     # +1, +44, +33, etc.
  _country_code_in: [String!]
  _country_code_nin: [String!]

  # Geographic grouping
  _country_eq: CountryCode     # Derived from country code
  _country_in: [CountryCode!]
  _region_eq: String           # North America, Europe, Asia, etc.

  # Number type detection (requires libphonenumber)
  _is_mobile: Boolean
  _is_landline: Boolean
  _is_toll_free: Boolean
  _is_premium: Boolean

  # Pattern matching (on national number)
  _national_startswith: String # Area code matching
  _national_contains: String
}
```

#### URLFilter

```graphql
input URLFilter {
  # Basic
  _eq: URL
  _neq: URL
  _in: [URL!]
  _nin: [URL!]
  _is_null: Boolean

  # Protocol
  _protocol_eq: String         # 'https', 'http', 'ftp'
  _protocol_in: [String!]
  _is_secure: Boolean          # Protocol = 'https'

  # Host extraction
  _host_eq: String             # 'api.example.com'
  _host_in: [String!]
  _host_endswith: String       # '.example.com' (subdomain matching)
  _host_contains: String

  # Domain (excludes subdomain)
  _domain_eq: String           # 'example.com' (without subdomain)
  _domain_in: [String!]
  _tld_eq: String              # 'com', 'org', 'io'
  _tld_in: [String!]

  # Path
  _path_eq: String
  _path_startswith: String     # '/api/v1/'
  _path_contains: String
  _path_matches: String        # Regex on path

  # Query parameters
  _has_query_param: String     # Has parameter 'utm_source'
  _query_param_eq: QueryParamInput  # { key: "ref", value: "google" }
}
```

#### DomainNameFilter / HostnameFilter

```graphql
input DomainNameFilter {
  # Basic
  _eq: DomainName
  _neq: DomainName
  _in: [DomainName!]
  _nin: [DomainName!]
  _is_null: Boolean

  # TLD extraction
  _tld_eq: String              # 'com', 'org', 'co.uk'
  _tld_in: [String!]
  _tld_category: TLDCategory   # GENERIC, COUNTRY, SPONSORED, INFRASTRUCTURE

  # Hierarchy
  _parent_domain_eq: String    # 'example.com' matches 'api.example.com'
  _subdomain_of: String        # Is subdomain of given domain
  _depth_eq: Int               # Number of labels (api.example.com = 3)
  _depth_gte: Int
  _depth_lte: Int

  # Pattern
  _endswith: String            # '.example.com'
  _contains: String
}
```

---

### 2. Location & Geography

#### CountryCodeFilter

```graphql
input CountryCodeFilter {
  # Basic
  _eq: CountryCode
  _neq: CountryCode
  _in: [CountryCode!]
  _nin: [CountryCode!]
  _is_null: Boolean

  # Geographic grouping
  _continent_eq: Continent     # AF, AN, AS, EU, NA, OC, SA
  _continent_in: [Continent!]
  _region_eq: String           # 'Western Europe', 'Southeast Asia', etc.
  _subregion_eq: String        # More granular regions

  # Political/Economic groupings
  _in_eu: Boolean              # European Union member
  _in_eurozone: Boolean        # Uses Euro
  _in_schengen: Boolean        # Schengen Area
  _in_nato: Boolean
  _in_g7: Boolean
  _in_g20: Boolean
  _in_oecd: Boolean
  _in_opec: Boolean
  _in_commonwealth: Boolean
  _in_brics: Boolean

  # Development classification
  _income_level: IncomeLevel   # HIGH, UPPER_MIDDLE, LOWER_MIDDLE, LOW
  _is_developed: Boolean

  # Regulatory
  _gdpr_applicable: Boolean    # EU + EEA + adequacy decisions
  _sanctions_list: SanctionsList  # OFAC, EU, UN sanctions
}
```

**Implementation Note:** Country groupings stored in static lookup table, updated periodically.

#### CoordinatesFilter / LatitudeFilter / LongitudeFilter

```graphql
input CoordinatesFilter {
  # Basic (if stored as composite)
  _eq: Coordinates
  _is_null: Boolean

  # Geospatial (requires PostGIS)
  _within_radius: RadiusInput  # { center: {lat, lng}, radius_km: 50 }
  _within_bounds: BoundsInput  # { min_lat, max_lat, min_lng, max_lng }
  _within_polygon: [[Float!]!] # GeoJSON polygon coordinates
  _within_geojson: JSON        # Arbitrary GeoJSON geometry

  # Distance calculations
  _distance_from_lt: DistanceInput   # { point: {lat, lng}, km: 100 }
  _distance_from_lte: DistanceInput
  _distance_from_gt: DistanceInput
  _distance_from_gte: DistanceInput

  # Hemisphere/quadrant
  _hemisphere_lat: Hemisphere  # NORTH, SOUTH
  _hemisphere_lng: Hemisphere  # EAST, WEST

  # Named regions (requires geocoding data)
  _in_country: CountryCode
  _in_region: String           # State/province
  _in_city: String
  _in_timezone: Timezone
}

input RadiusInput {
  center: CoordinatesInput!
  radius_km: Float!
}

input BoundsInput {
  min_lat: Float!
  max_lat: Float!
  min_lng: Float!
  max_lng: Float!
}
```

#### PostalCodeFilter

```graphql
input PostalCodeFilter {
  # Basic
  _eq: PostalCode
  _neq: PostalCode
  _in: [PostalCode!]
  _nin: [PostalCode!]
  _is_null: Boolean

  # Pattern matching
  _startswith: String          # '90' for LA area codes
  _matches: String             # Regex

  # Country-specific parsing
  _country_eq: CountryCode     # Filter by postal code's country format

  # US ZIP codes
  _zip5_eq: String             # First 5 digits of ZIP+4
  _zip3_eq: String             # SCF (Sectional Center Facility)

  # UK postcodes
  _outcode_eq: String          # 'SW1A' part of 'SW1A 1AA'
  _area_eq: String             # 'SW' area
  _district_eq: String         # 'SW1A' district

  # Canadian postal codes
  _fsa_eq: String              # Forward Sortation Area (first 3 chars)
}
```

#### TimezoneFilter

```graphql
input TimezoneFilter {
  # Basic
  _eq: Timezone
  _neq: Timezone
  _in: [Timezone!]
  _nin: [Timezone!]
  _is_null: Boolean

  # Offset-based
  _offset_eq: Int              # UTC offset in minutes
  _offset_gte: Int
  _offset_lte: Int
  _offset_between: OffsetRange # { min: -300, max: 300 }

  # DST handling
  _observes_dst: Boolean
  _current_offset_eq: Int      # Current offset (DST-aware)

  # Geographic
  _continent_eq: String        # 'America', 'Europe', 'Asia'
  _region_eq: String           # 'America/New_York' region part
}
```

#### LanguageCodeFilter / LocaleCodeFilter

```graphql
input LanguageCodeFilter {
  # Basic
  _eq: LanguageCode
  _neq: LanguageCode
  _in: [LanguageCode!]
  _nin: [LanguageCode!]
  _is_null: Boolean

  # Language families
  _family_eq: String           # 'Indo-European', 'Sino-Tibetan', etc.
  _branch_eq: String           # 'Germanic', 'Romance', 'Slavic'

  # Script
  _script_eq: ScriptCode       # 'Latn', 'Cyrl', 'Hans', 'Arab'
  _script_in: [ScriptCode!]

  # Properties
  _is_rtl: Boolean             # Right-to-left script
  _is_official_in: CountryCode # Official language of country
  _speakers_gte: Int           # Native speakers (millions)
}

input LocaleCodeFilter {
  # Basic
  _eq: LocaleCode
  _in: [LocaleCode!]
  _is_null: Boolean

  # Component extraction (e.g., 'en-US', 'zh-Hans-CN')
  _language_eq: LanguageCode   # 'en', 'zh'
  _language_in: [LanguageCode!]
  _script_eq: ScriptCode       # 'Hans', 'Hant'
  _region_eq: CountryCode      # 'US', 'CN'
  _region_in: [CountryCode!]
}
```

---

### 3. Financial

#### CurrencyCodeFilter

```graphql
input CurrencyCodeFilter {
  # Basic
  _eq: CurrencyCode
  _neq: CurrencyCode
  _in: [CurrencyCode!]
  _nin: [CurrencyCode!]
  _is_null: Boolean

  # Classification
  _is_fiat: Boolean
  _is_crypto: Boolean
  _is_commodity: Boolean       # XAU (gold), XAG (silver)
  _is_supranational: Boolean   # EUR, XOF, XAF

  # Properties
  _decimals_eq: Int            # Minor unit decimals (USD=2, JPY=0, BTC=8)
  _country_eq: CountryCode     # Primary country (for national currencies)
  _countries_include: CountryCode  # Any country using this currency

  # Groupings
  _is_major: Boolean           # G10 currencies
  _is_exotic: Boolean
  _is_pegged: Boolean          # Pegged to another currency
  _pegged_to: CurrencyCode     # e.g., HKD pegged to USD
}
```

#### MoneyFilter

```graphql
input MoneyFilter {
  # Basic
  _eq: Money
  _neq: Money
  _is_null: Boolean

  # Amount comparisons (same currency)
  _amount_eq: Decimal
  _amount_neq: Decimal
  _amount_gt: Decimal
  _amount_gte: Decimal
  _amount_lt: Decimal
  _amount_lte: Decimal
  _amount_between: DecimalRange

  # Currency filtering
  _currency_eq: CurrencyCode
  _currency_in: [CurrencyCode!]
  _currency_nin: [CurrencyCode!]

  # Cross-currency (requires exchange rate table)
  _converted_gt: MoneyInput    # Compare after conversion
  _converted_gte: MoneyInput
  _converted_lt: MoneyInput
  _converted_lte: MoneyInput
}
```

#### IBANFilter

```graphql
input IBANFilter {
  # Basic
  _eq: IBAN
  _neq: IBAN
  _in: [IBAN!]
  _nin: [IBAN!]
  _is_null: Boolean

  # Country extraction (first 2 characters)
  _country_eq: CountryCode     # 'DE', 'FR', 'GB'
  _country_in: [CountryCode!]
  _country_nin: [CountryCode!]

  # Bank identification
  _bank_code_eq: String        # SWIFT/BIC bank code portion
  _bank_code_in: [String!]
  _bank_code_startswith: String

  # Branch identification (country-specific)
  _branch_code_eq: String

  # SEPA
  _is_sepa: Boolean            # SEPA zone country

  # Check digit validation (informational)
  _is_valid: Boolean           # Passes mod-97 check
}
```

#### BICFilter / SWIFTFilter

```graphql
input BICFilter {
  # Basic
  _eq: BIC
  _neq: BIC
  _in: [BIC!]
  _nin: [BIC!]
  _is_null: Boolean

  # Component extraction (8 or 11 characters: AAAABBCCXXX)
  _bank_code_eq: String        # First 4: Bank code
  _country_eq: CountryCode     # Chars 5-6: Country
  _location_eq: String         # Chars 7-8: Location
  _branch_eq: String           # Chars 9-11: Branch (optional)

  # Properties
  _is_test: Boolean            # Location code ends in '0'
  _is_passive: Boolean         # Location code ends in '1'
  _is_primary: Boolean         # Branch = 'XXX' or empty
}
```

#### Securities Identifiers (CUSIP, ISIN, SEDOL, LEI)

```graphql
input CUSIPFilter {
  # Basic
  _eq: CUSIP
  _in: [CUSIP!]
  _is_null: Boolean

  # Component extraction
  _issuer_eq: String           # First 6 characters (issuer)
  _issuer_startswith: String
  _issue_eq: String            # Characters 7-8 (issue)

  # Security type
  _is_equity: Boolean
  _is_fixed_income: Boolean
  _is_government: Boolean      # Government securities
}

input ISINFilter {
  # Basic
  _eq: ISIN
  _in: [ISIN!]
  _is_null: Boolean

  # Component extraction
  _country_eq: CountryCode     # First 2 characters
  _country_in: [CountryCode!]
  _nsin_eq: String             # National Securities ID (chars 3-11)
  _nsin_startswith: String
}

input LEIFilter {
  # Basic
  _eq: LEI
  _in: [LEI!]
  _is_null: Boolean

  # Component extraction
  _lou_eq: String              # First 4: Local Operating Unit
  _lou_in: [String!]
  _entity_eq: String           # Chars 5-18: Entity identifier

  # Status (requires GLEIF data)
  _status_eq: LEIStatus        # ISSUED, LAPSED, MERGED, etc.
  _is_active: Boolean
}
```

#### PercentageFilter

```graphql
input PercentageFilter {
  # Basic
  _eq: Percentage
  _neq: Percentage
  _is_null: Boolean

  # Numeric comparisons
  _gt: Float
  _gte: Float
  _lt: Float
  _lte: Float
  _between: FloatRange

  # Convenience
  _is_zero: Boolean
  _is_positive: Boolean
  _is_negative: Boolean
  _is_whole: Boolean           # 0%, 50%, 100% etc (no decimals)
}
```

---

### 4. Identifiers

#### VINFilter

```graphql
input VINFilter {
  # Basic
  _eq: VIN
  _neq: VIN
  _in: [VIN!]
  _nin: [VIN!]
  _is_null: Boolean

  # WMI - World Manufacturer Identifier (first 3 characters)
  _wmi_eq: String              # 'WVW' (Volkswagen Germany)
  _wmi_in: [String!]
  _wmi_startswith: String      # 'W' (Germany), '1' (USA), 'J' (Japan)

  # Manufacturer
  _manufacturer_eq: String     # Decoded manufacturer name
  _manufacturer_in: [String!]

  # VDS - Vehicle Descriptor Section (chars 4-9)
  _vds_eq: String

  # VIS - Vehicle Identifier Section (chars 10-17)
  _model_year_eq: Int          # Decoded from position 10
  _model_year_gte: Int
  _model_year_lte: Int
  _model_year_between: IntRange

  _plant_code_eq: String       # Position 11

  # Country of origin (derived from WMI)
  _country_eq: CountryCode
  _country_in: [CountryCode!]
  _region_eq: VINRegion        # EUROPE, NORTH_AMERICA, ASIA, etc.
}
```

#### LicensePlateFilter

```graphql
input LicensePlateFilter {
  # Basic
  _eq: LicensePlate
  _neq: LicensePlate
  _in: [LicensePlate!]
  _nin: [LicensePlate!]
  _is_null: Boolean

  # Pattern
  _startswith: String
  _endswith: String
  _contains: String
  _matches: String             # Regex

  # Country-specific parsing
  _country_eq: CountryCode

  # Regional (country-dependent)
  _region_eq: String           # State/province code
  _region_in: [String!]
}
```

#### TrackingNumberFilter

```graphql
input TrackingNumberFilter {
  # Basic
  _eq: TrackingNumber
  _in: [TrackingNumber!]
  _is_null: Boolean

  # Carrier detection
  _carrier_eq: Carrier         # UPS, FEDEX, USPS, DHL, etc.
  _carrier_in: [Carrier!]

  # Pattern
  _startswith: String
  _matches: String

  # Service type (carrier-dependent)
  _service_type_eq: String     # 'EXPRESS', 'GROUND', 'FREIGHT'
}
```

#### ContainerNumberFilter

```graphql
input ContainerNumberFilter {
  # Basic
  _eq: ContainerNumber
  _in: [ContainerNumber!]
  _is_null: Boolean

  # Owner code (first 3 letters)
  _owner_eq: String
  _owner_in: [String!]

  # Equipment category (4th character)
  _category_eq: ContainerCategory  # U=freight, J=detachable, Z=trailer

  # Size/type (from container type code)
  _size_eq: ContainerSize      # 20FT, 40FT, 45FT
  _type_eq: ContainerType      # DRY, REEFER, TANK, FLAT, OPEN_TOP
}
```

#### SlugFilter

```graphql
input SlugFilter {
  # Basic
  _eq: Slug
  _neq: Slug
  _in: [Slug!]
  _nin: [Slug!]
  _is_null: Boolean

  # Pattern matching
  _startswith: String
  _endswith: String
  _contains: String

  # Hierarchy (for path-like slugs: 'category/subcategory/item')
  _path_startswith: String     # 'category/' matches 'category/item'
  _path_depth_eq: Int          # Number of segments
  _path_depth_gte: Int
  _path_segment_eq: SlugSegmentInput  # { position: 0, value: "category" }
}
```

#### SemanticVersionFilter

```graphql
input SemanticVersionFilter {
  # Basic
  _eq: SemanticVersion
  _neq: SemanticVersion
  _in: [SemanticVersion!]
  _nin: [SemanticVersion!]
  _is_null: Boolean

  # Version comparison (semver-aware)
  _gt: SemanticVersion
  _gte: SemanticVersion
  _lt: SemanticVersion
  _lte: SemanticVersion

  # Component extraction
  _major_eq: Int
  _major_gte: Int
  _major_lte: Int
  _minor_eq: Int
  _minor_gte: Int
  _minor_lte: Int
  _patch_eq: Int
  _patch_gte: Int
  _patch_lte: Int

  # Pre-release / metadata
  _prerelease_eq: String       # 'alpha', 'beta.1', 'rc.2'
  _has_prerelease: Boolean
  _is_stable: Boolean          # No prerelease tag
  _build_eq: String            # Build metadata

  # Range expressions (npm/cargo style)
  _satisfies: String           # '^1.2.0', '~1.2.0', '>=1.0.0 <2.0.0'
  _compatible_with: SemanticVersion  # ^version
  _approximately: SemanticVersion    # ~version
}
```

#### APIKeyFilter

```graphql
input APIKeyFilter {
  # Basic
  _eq: APIKey
  _in: [APIKey!]
  _is_null: Boolean

  # Prefix (common pattern: 'sk_live_xxx', 'pk_test_xxx')
  _prefix_eq: String           # 'sk_live', 'pk_test'
  _prefix_in: [String!]
  _startswith: String

  # Environment detection
  _is_live: Boolean            # Contains 'live' or 'prod'
  _is_test: Boolean            # Contains 'test' or 'dev'

  # Key type detection
  _is_secret: Boolean          # Starts with 'sk_'
  _is_public: Boolean          # Starts with 'pk_'
}
```

#### HashSHA256Filter

```graphql
input HashSHA256Filter {
  # Basic
  _eq: HashSHA256
  _neq: HashSHA256
  _in: [HashSHA256!]
  _nin: [HashSHA256!]
  _is_null: Boolean

  # Prefix matching (for sharding/partitioning)
  _startswith: String          # First N characters
  _prefix_eq: String           # Explicit prefix
  _prefix_in: [String!]
}
```

---

### 5. Transportation

#### AirportCodeFilter

```graphql
input AirportCodeFilter {
  # Basic
  _eq: AirportCode
  _neq: AirportCode
  _in: [AirportCode!]
  _nin: [AirportCode!]
  _is_null: Boolean

  # Geographic (requires airport database)
  _country_eq: CountryCode
  _country_in: [CountryCode!]
  _region_eq: String           # State/province
  _city_eq: String
  _continent_eq: Continent

  # Airport properties
  _type_eq: AirportType        # INTERNATIONAL, DOMESTIC, REGIONAL
  _size_eq: AirportSize        # LARGE_HUB, MEDIUM_HUB, SMALL_HUB
  _is_hub: Boolean             # Major airline hub
  _has_customs: Boolean        # International arrivals

  # Geospatial
  _within_radius: AirportRadiusInput  # Airports within X km of point
  _within_bounds: BoundsInput

  # Timezone
  _timezone_eq: Timezone
  _timezone_offset_eq: Int
}
```

#### PortCodeFilter

```graphql
input PortCodeFilter {
  # Basic
  _eq: PortCode
  _neq: PortCode
  _in: [PortCode!]
  _nin: [PortCode!]
  _is_null: Boolean

  # UN/LOCODE parsing (5 chars: 2 country + 3 location)
  _country_eq: CountryCode
  _country_in: [CountryCode!]
  _location_eq: String         # 3-char location code

  # Port properties
  _type_eq: PortType           # SEA, RIVER, RAIL, ROAD, AIRPORT, MULTIMODAL
  _type_in: [PortType!]
  _is_seaport: Boolean
  _is_inland: Boolean

  # Geographic
  _continent_eq: Continent
  _region_eq: String
  _within_bounds: BoundsInput
}
```

#### FlightNumberFilter

```graphql
input FlightNumberFilter {
  # Basic
  _eq: FlightNumber
  _neq: FlightNumber
  _in: [FlightNumber!]
  _nin: [FlightNumber!]
  _is_null: Boolean

  # Component extraction (e.g., 'AA1234')
  _airline_eq: String          # IATA airline code: 'AA', 'UA', 'DL'
  _airline_in: [String!]
  _flight_eq: Int              # Numeric portion: 1234
  _flight_gte: Int
  _flight_lte: Int

  # Airline properties (requires airline database)
  _airline_country_eq: CountryCode
  _airline_alliance_eq: String  # ONEWORLD, STAR_ALLIANCE, SKYTEAM
  _is_codeshare: Boolean        # Operated by different airline
}
```

---

### 6. Content & Media

#### MimeTypeFilter

```graphql
input MimeTypeFilter {
  # Basic
  _eq: MimeType
  _neq: MimeType
  _in: [MimeType!]
  _nin: [MimeType!]
  _is_null: Boolean

  # Type/subtype parsing (e.g., 'application/json')
  _type_eq: String             # 'application', 'image', 'video', 'audio', 'text'
  _type_in: [String!]
  _subtype_eq: String          # 'json', 'png', 'mp4'
  _subtype_in: [String!]

  # Category grouping
  _is_image: Boolean           # type = 'image'
  _is_video: Boolean           # type = 'video'
  _is_audio: Boolean           # type = 'audio'
  _is_text: Boolean            # type = 'text'
  _is_document: Boolean        # pdf, doc, docx, etc.
  _is_archive: Boolean         # zip, tar, gz, etc.
  _is_binary: Boolean          # Not text-based

  # Parameters
  _charset_eq: String          # For text/* types
}
```

#### ColorFilter

```graphql
input ColorFilter {
  # Basic
  _eq: Color
  _neq: Color
  _in: [Color!]
  _nin: [Color!]
  _is_null: Boolean

  # Hex parsing
  _hex_eq: String              # '#FF5733'
  _hex_startswith: String

  # RGB components
  _red_eq: Int
  _red_gte: Int
  _red_lte: Int
  _green_eq: Int
  _green_gte: Int
  _green_lte: Int
  _blue_eq: Int
  _blue_gte: Int
  _blue_lte: Int

  # HSL components
  _hue_gte: Int               # 0-360
  _hue_lte: Int
  _hue_between: IntRange
  _saturation_gte: Float       # 0-100
  _saturation_lte: Float
  _lightness_gte: Float        # 0-100
  _lightness_lte: Float

  # Perceptual
  _is_light: Boolean           # Lightness > 50%
  _is_dark: Boolean
  _is_saturated: Boolean       # Saturation > 50%
  _is_grayscale: Boolean       # Saturation = 0
  _similar_to: ColorSimilarInput  # { color: "#FF0000", threshold: 0.1 }
}
```

#### MarkdownFilter / HTMLFilter

```graphql
input MarkdownFilter {
  # Basic
  _eq: Markdown
  _is_null: Boolean

  # Content search
  _contains: String            # Text content contains
  _icontains: String

  # Fulltext (if indexed)
  _search: String              # Full-text search

  # Structure detection
  _has_headings: Boolean
  _has_links: Boolean
  _has_images: Boolean
  _has_code_blocks: Boolean
  _has_tables: Boolean

  # Length
  _length_gte: Int             # Character count
  _length_lte: Int
  _word_count_gte: Int
  _word_count_lte: Int
}
```

---

### 7. Network (Extensions to existing NetworkAddressFilter)

#### PortFilter

```graphql
input PortFilter {
  # Basic
  _eq: Port
  _neq: Port
  _in: [Port!]
  _nin: [Port!]
  _is_null: Boolean

  # Numeric comparison
  _gt: Int
  _gte: Int
  _lt: Int
  _lte: Int
  _between: IntRange

  # Port categories
  _is_well_known: Boolean      # 0-1023
  _is_registered: Boolean      # 1024-49151
  _is_dynamic: Boolean         # 49152-65535
  _is_privileged: Boolean      # < 1024

  # Common services
  _is_http: Boolean            # 80, 8080
  _is_https: Boolean           # 443
  _is_ssh: Boolean             # 22
  _is_database: Boolean        # 3306, 5432, 27017, etc.
}
```

---

### 8. Database/Structural

#### DurationFilter

```graphql
input DurationFilter {
  # Basic
  _eq: Duration
  _neq: Duration
  _is_null: Boolean

  # Comparison
  _gt: Duration
  _gte: Duration
  _lt: Duration
  _lte: Duration

  # Component extraction
  _hours_eq: Int
  _hours_gte: Int
  _hours_lte: Int
  _minutes_eq: Int
  _minutes_gte: Int
  _minutes_lte: Int
  _seconds_eq: Float
  _seconds_gte: Float
  _seconds_lte: Float

  # Total conversion
  _total_seconds_gt: Float
  _total_seconds_gte: Float
  _total_seconds_lt: Float
  _total_seconds_lte: Float
  _total_minutes_gt: Float
  _total_hours_gt: Float
}
```

---

## Implementation Approach

### Phase 1: High-Value Filters (Quick Wins)

These use simple SQL string functions, no external dependencies:

1. **EmailFilter** - `SPLIT_PART(email, '@', 2)` for domain
2. **URLFilter** - Parse with regex or `parse_url()` extension
3. **SemanticVersionFilter** - `STRING_TO_ARRAY()` for component extraction
4. **VINFilter** - `SUBSTRING()` for WMI/VDS/VIS extraction
5. **IBANFilter** - `SUBSTRING()` for country/bank code
6. **SlugFilter** - `STRING_TO_ARRAY()` for path segments

### Phase 2: Lookup-Based Filters

Require static lookup tables (can be generated at compile time):

1. **CountryCodeFilter** - Continent/region/membership tables
2. **CurrencyCodeFilter** - Currency metadata table
3. **AirportCodeFilter** - Airport database (OpenFlights data)
4. **TimezoneFilter** - Timezone metadata

### Phase 3: Advanced Filters

Require external libraries or PostgreSQL extensions:

1. **CoordinatesFilter** - PostGIS for geospatial
2. **PhoneNumberFilter** - libphonenumber for carrier detection
3. **ColorFilter** - Color space conversions

---

## SQL Examples

### EmailFilter Implementation

```sql
-- Index for domain queries
CREATE INDEX idx_users_email_domain
ON tv_user ((SPLIT_PART(data->>'email', '@', 2)));

-- Filter: _domain_eq: "company.com"
WHERE SPLIT_PART(data->>'email', '@', 2) = 'company.com'

-- Filter: _domain_endswith: ".edu"
WHERE SPLIT_PART(data->>'email', '@', 2) LIKE '%.edu'

-- Filter: _is_freemail: true
WHERE SPLIT_PART(data->>'email', '@', 2) = ANY(
  ARRAY['gmail.com', 'yahoo.com', 'hotmail.com', 'outlook.com', ...]
)
```

### VINFilter Implementation

```sql
-- Index for manufacturer queries
CREATE INDEX idx_vehicles_vin_wmi
ON tv_vehicle ((SUBSTRING(data->>'vin', 1, 3)));

-- Filter: _wmi_eq: "WVW"
WHERE SUBSTRING(data->>'vin', 1, 3) = 'WVW'

-- Filter: _model_year_gte: 2020
-- (Requires VIN year decode function)
WHERE decode_vin_year(data->>'vin') >= 2020

-- Filter: _country_eq: "DE"
-- (WMI lookup: W = Germany)
WHERE SUBSTRING(data->>'vin', 1, 1) = 'W'
```

### SemanticVersionFilter Implementation

```sql
-- Filter: _major_gte: 2
WHERE (STRING_TO_ARRAY(data->>'version', '.'))[1]::int >= 2

-- Filter: _satisfies: "^1.2.0"
WHERE semver_satisfies(data->>'version', '^1.2.0')

-- Custom semver comparison function
CREATE FUNCTION semver_compare(a TEXT, b TEXT) RETURNS INT AS $$
  SELECT CASE
    WHEN (STRING_TO_ARRAY(a, '.'))[1]::int != (STRING_TO_ARRAY(b, '.'))[1]::int
    THEN (STRING_TO_ARRAY(a, '.'))[1]::int - (STRING_TO_ARRAY(b, '.'))[1]::int
    WHEN (STRING_TO_ARRAY(a, '.'))[2]::int != (STRING_TO_ARRAY(b, '.'))[2]::int
    THEN (STRING_TO_ARRAY(a, '.'))[2]::int - (STRING_TO_ARRAY(b, '.'))[2]::int
    ELSE (STRING_TO_ARRAY(a, '.'))[3]::int - (STRING_TO_ARRAY(b, '.'))[3]::int
  END;
$$ LANGUAGE SQL IMMUTABLE;
```

### CountryCodeFilter with Lookup Table

```sql
-- Static lookup table (generated at compile time)
CREATE TABLE _fraiseql_country_meta (
  code CHAR(2) PRIMARY KEY,
  name TEXT NOT NULL,
  continent CHAR(2) NOT NULL,  -- AF, AN, AS, EU, NA, OC, SA
  region TEXT,
  subregion TEXT,
  in_eu BOOLEAN DEFAULT FALSE,
  in_eurozone BOOLEAN DEFAULT FALSE,
  in_schengen BOOLEAN DEFAULT FALSE,
  in_nato BOOLEAN DEFAULT FALSE,
  income_level TEXT  -- HIGH, UPPER_MIDDLE, LOWER_MIDDLE, LOW
);

-- Filter: _continent_eq: "EU"
WHERE data->>'country' IN (
  SELECT code FROM _fraiseql_country_meta WHERE continent = 'EU'
)

-- Filter: _in_eu: true
WHERE data->>'country' IN (
  SELECT code FROM _fraiseql_country_meta WHERE in_eu = TRUE
)
```

---

## Backward Compatibility

All changes are additive. Existing filters continue to work. New operators are opt-in through the filter type assignment logic.

---

## Open Questions

1. **Operator naming**: Use `_domain_eq` or `_domainEq`? (Current convention: snake_case with underscore prefix)

2. **Lookup table generation**: Should country/airport/timezone metadata be:
   - Embedded in binary at compile time?
   - Stored in database tables?
   - Fetched from external API at runtime?

3. **Performance tradeoffs**: Some filters (like `_is_freemail`) require list membership checks. Should we:
   - Use `= ANY(ARRAY[...])` inline?
   - Create a lookup table with index?
   - Use a GIN index on a JSONB array?

4. **Extension dependencies**: Filters like `CoordinatesFilter._within_radius` require PostGIS. Should we:
   - Make them conditional on extension availability?
   - Document as optional features?
   - Provide fallback implementations?

---

## References

- [Existing NetworkAddressFilter implementation](src/fraiseql/sql/graphql_where_generator.py#L273)
- [Operator Registry](crates/fraiseql-core/src/utils/operators.rs)
- [Scalar Type Definitions](src/fraiseql/types/scalars/)
