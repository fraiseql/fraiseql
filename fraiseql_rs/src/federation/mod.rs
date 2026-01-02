//! Apollo Federation 2.0 support for FraiseQL
//!
//! Implements Federation Lite and Federation Standard modes with auto-key detection,
//! automatic entity resolution, and schema generation.
//!
//! # Architecture
//!
//! Federation support is organized into progressive modes:
//! - **Lite**: Auto-key detection, `@entity` only (80% of users)
//! - **Standard**: Type extensions, `@requires`, `@provides` (15% of users)
//! - **Advanced**: All 18 directives (5% of users, Phase 17b)

pub mod auto_detect;
pub mod entities_resolver;

pub use auto_detect::{auto_detect_key, AutoDetectError, FieldInfo};
pub use entities_resolver::{EntityMetadata, EntityResolver, EntityResolverError};

/// Version of federation support
pub const FEDERATION_VERSION: &str = "2.5";
