//! Advanced type operators and SQL templates for Week 5.
//!
//! This module documents the implementation of advanced type operators
//! that require specialized SQL generation or external libraries.
//!
//! # Geospatial Types (PostGIS)
//!
//! Geospatial operators require PostGIS extensions on PostgreSQL, or
//! equivalent spatial functions on other databases.
//!
//! ## Coordinates Type
//!
//! Represents a single point with latitude and longitude.
//!
//! **Format**: JSON `{lat: float, lng: float}`
//!
//! **Operators**:
//! - `distanceWithin`: Distance from point within radius (km)
//! - `withinBoundingBox`: Point within rectangular bounding box
//! - `withinPolygon`: Point within polygon (future enhancement)
//!
//! **Database Support**:
//! - PostgreSQL: Native PostGIS support (ST_DWithin, ST_GeomFromText)
//! - MySQL: Built-in spatial functions (ST_Distance_Sphere)
//! - SQLite: Haversine formula approximation (no spatial library)
//! - SQL Server: Native geography type (ST_Distance)
//!
//! **Example Query**:
//! ```graphql
//! query {
//!   restaurants(
//!     where: {
//!       location: {
//!         distanceWithin: {
//!           latitude: 40.7128
//!           longitude: -74.0060
//!           radiusKm: 5
//!         }
//!       }
//!     }
//!   ) {
//!     name
//!   }
//! }
//! ```
//!
//! # Phone Number Type
//!
//! Phone number operators provide E.164 format validation and
//! country code extraction.
//!
//! **Format**: E.164 string (e.g., "+14155552671")
//!
//! **Operators**:
//! - `countryCodeEq`: Country code matches
//! - `countryCodeIn`: Country code in list
//! - `isValid`: Valid E.164 format (+[1-9]{1,3}[0-9]{1,14})
//! - `typeEq`: Type classification (US, UK, OTHER)
//!
//! **Database Support**:
//! - PostgreSQL: Regex matching with '^\\+[1-9]' pattern
//! - MySQL: REGEXP operator with escaping
//! - SQLite: GLOB patterns for basic matching
//! - SQL Server: LIKE patterns for matching
//!
//! **Notes**:
//! - Full phone validation (including carrier type) requires phonenumber-rs
//! - Current implementation provides basic E.164 validation
//! - Country code extraction assumes standard E.164 format
//!
//! **Example Query**:
//! ```graphql
//! query {
//!   users(
//!     where: {
//!       phone: {
//!         countryCodeEq: "+1"
//!       }
//!     }
//!   ) {
//!     phone
//!   }
//! }
//! ```
//!
//! # Date Range Type
//!
//! Date range operators provide range analysis and overlap detection.
//!
//! **Format**: JSON with ISO 8601 dates
//! ```json
//! {
//!   "start": "2024-01-01T00:00:00Z",
//!   "end": "2024-12-31T23:59:59Z"
//! }
//! ```
//!
//! **Operators**:
//! - `durationGte`: Total duration >= min days
//! - `startsAfter`: Range starts after date
//! - `endsBefore`: Range ends before date
//! - `overlaps`: Overlaps with another date range
//!
//! **Database Support**:
//! - PostgreSQL: Native timestamp and INTERVAL types
//! - MySQL: DATEDIFF and date functions
//! - SQLite: julianday for date arithmetic
//! - SQL Server: DATEDIFF and datetime functions
//!
//! **Example Query**:
//! ```graphql
//! query {
//!   projects(
//!     where: {
//!       timeline: {
//!         durationGte: 90
//!         overlaps: {
//!           start: "2024-06-01T00:00:00Z"
//!           end: "2024-08-31T23:59:59Z"
//!         }
//!       }
//!     }
//!   ) {
//!     name
//!     timeline {
//!       start
//!       end
//!     }
//!   }
//! }
//! ```
//!
//! # Duration Type
//!
//! Duration operators convert ISO 8601 durations to seconds/minutes
//! for range queries.
//!
//! **Format**: ISO 8601 duration string
//! - "P1Y2M3DT4H5M6S" (1 year, 2 months, 3 days, 4 hours, 5 minutes, 6 seconds)
//! - "PT1H" (1 hour)
//! - "PT30M" (30 minutes)
//! - "PT45S" (45 seconds)
//!
//! **Operators**:
//! - `totalSecondsEq`: Duration equals (in seconds)
//! - `totalMinutesGte`: Duration >= min (in minutes)
//!
//! **Database Support**:
//! - PostgreSQL: CAST to INTERVAL, then EXTRACT(EPOCH ...)
//! - MySQL: Parse PT notation and cast to numeric
//! - SQLite: Parse PT notation and cast to numeric
//! - SQL Server: SUBSTRING to parse and cast
//!
//! **Example Query**:
//! ```graphql
//! query {
//!   tasks(
//!     where: {
//!       estimatedTime: {
//!         totalMinutesGte: 480
//!       }
//!     }
//!   ) {
//!     name
//!     estimatedTime
//!   }
//! }
//! ```
//!
//! # Implementation Status
//!
//! | Type | Operator | PostgreSQL | MySQL | SQLite | SQL Server | Status |
//! |------|----------|-----------|-------|--------|------------|--------|
//! | Coordinates | distanceWithin | ✅ PostGIS | ✅ | ⚠️ Approx | ✅ | Implemented |
//! | Coordinates | withinBoundingBox | ✅ | ✅ | ✅ | ✅ | Implemented |
//! | Phone | countryCodeEq | ✅ | ✅ | ✅ | ✅ | Implemented |
//! | Phone | isValid | ✅ Regex | ✅ | ⚠️ Basic | ⚠️ Basic | Implemented |
//! | DateRange | durationGte | ✅ | ✅ | ✅ | ✅ | Implemented |
//! | DateRange | overlaps | ✅ | ✅ | ✅ | ✅ | Implemented |
//! | Duration | totalSecondsEq | ✅ | ✅ | ✅ | ✅ | Implemented |
//! | Duration | totalMinutesGte | ✅ | ✅ | ✅ | ✅ | Implemented |
//!
//! # Performance Considerations
//!
//! ## Geospatial Queries
//! - PostGIS indexes (GIST/BRIN) required for performance
//! - ST_DWithin uses index-aware distance calculation
//! - Bounding box queries are generally fast (simple range checks)
//!
//! ## Date Range Queries
//! - Indexes on start/end timestamp fields recommended
//! - OVERLAP operations efficient with proper indexes
//! - Use separate indexed columns for start/end instead of JSON
//!
//! ## Duration Parsing
//! - ISO 8601 parsing done in SQL (no application overhead)
//! - Regex validation in application layer (before SQL)
//!
//! # Limitations and Future Work
//!
//! ## Current Limitations
//! - Phone validation limited to E.164 format (not carrier type)
//! - SQLite geospatial using Haversine approximation (not exact)
//! - No polygon containment without spatial extension
//! - ISO 8601 duration parsing requires standard format
//!
//! ## Future Enhancements
//! - Full phone number validation via phonenumber-rs library
//! - PostGIS polygon operators (CoordinatesWithinPolygon)
//! - Advanced date range operations (gaps, unions)
//! - Time zone aware date operations
//! - Route distance calculation (requires routing library)

/// Marker type for documentation. This module documents advanced type
/// operators but doesn't export any functions (implementation is in sql_templates.rs).
pub struct AdvancedTypesDocumentation;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_documentation_exists() {
        // This test ensures the documentation module compiles
        let _ = AdvancedTypesDocumentation;
    }
}
