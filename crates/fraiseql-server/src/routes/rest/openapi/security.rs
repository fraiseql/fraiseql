//! `OpenAPI` security scheme and requirement building.

use serde_json::{Value, json};

use super::OpenApiGenerator;

impl OpenApiGenerator<'_> {
    /// Stamp the transport's security posture onto one operation object.
    ///
    /// **The single site that decides what an operation advertises.** Every operation
    /// builder must route through it. Three did not: the `openapi.json` meta entry, the
    /// SSE stream endpoint, and the two bulk operations each hand-built their operation
    /// JSON and never consulted the security helper, so they advertised no
    /// authentication no matter how the deployment was configured.
    ///
    /// That is the same shape as the defect the `security_required` flag exists to close
    /// (#810) — a guard that exists, is documented, and has call sites routing around
    /// it — which is why the check lives in one function and the test that drives it
    /// walks *every* operation in the served document rather than a representative one.
    ///
    /// The posture is transport-wide (a mount-level auth layer and `require_auth` both
    /// apply to the whole router), so this takes no per-route argument. If REST ever
    /// gains genuinely per-route auth, this signature is where that distinction belongs
    /// — not in a second copy at each builder.
    pub(super) fn apply_security(&self, operation: &mut Value) {
        if !self.security_required {
            return;
        }

        operation["security"] = json!([{ "BearerAuth": [] }]);

        if let Some(responses) = operation.get_mut("responses") {
            responses["401"] = json!({ "description": "Unauthorized" });
            responses["403"] = json!({ "description": "Forbidden" });
        }
    }
}
