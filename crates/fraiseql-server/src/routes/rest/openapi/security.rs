//! `OpenAPI` security scheme and requirement building.

use serde_json::{Value, json};

use super::OpenApiGenerator;
use crate::routes::rest::resource::RestRoute;

impl OpenApiGenerator<'_> {
    /// Build the security requirement for a route, or `None` if auth is not required.
    pub(super) fn build_security(&self, _route: &RestRoute) -> Option<Value> {
        if !self.config.require_auth {
            return None;
        }

        Some(json!([{ "BearerAuth": [] }]))
    }
}
