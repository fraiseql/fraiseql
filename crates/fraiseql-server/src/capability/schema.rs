//! The JSON Schema of each section a capability is configured by.
//!
//! `fraiseql capabilities --json` reports, for every registration, the schema of its
//! compiled-schema sections and of its server-config keys, derived from the very types serde
//! reads them into: an agent writing a section reads the contract the server enforces, not a
//! description of it. A key is looked up by name, the name [`CapabilityMeta`] lists it under,
//! so a registration cannot name one section and describe another.
//!
//! [`CapabilityMeta`]: fraiseql_core::capability::CapabilityMeta

use serde_json::Value;

/// The schema of `T`, as `schemars` derives it from the type serde reads.
fn of<T: schemars::JsonSchema>() -> Value {
    schemars::schema_for!(T).to_value()
}

/// The schema of a compiled-schema section, by the key the registry lists it under.
///
/// A key no arm names is `None`, and the registry's test refuses that for every section a
/// registration lists. Kept apart from [`of_runtime_key`]: `sources` names both a
/// compiled section and a server-config key, of two different types.
pub(super) fn of_compiled(key: &str) -> Option<Value> {
    use fraiseql_core::schema::{
        FederationConfig, GrpcConfig, McpConfig, ObserversConfig, RestConfig, SourceDefinition,
    };

    Some(match key {
        "functions" => of::<fraiseql_functions::FunctionsConfig>(),
        "sources" => of::<Vec<SourceDefinition>>(),
        "mcp_config" => of::<McpConfig>(),
        "rest_config" => of::<RestConfig>(),
        "grpc_config" => of::<GrpcConfig>(),
        "federation" => of::<FederationConfig>(),
        "observers_config" => of::<ObserversConfig>(),
        _ => return None,
    })
}

/// The schema of a server-config key, by the `ServerConfig` field name.
///
/// `None` for a key this build cannot describe: each field exists only under its
/// capability's feature. A key no arm names is `None` too, and the registry's test
/// refuses that for every key a compiled-in registration lists.
// Reason: with no capability that has server-config keys compiled in, only `_ => None` is left.
#[cfg_attr(
    not(any(
        feature = "sources",
        feature = "rest",
        feature = "observers",
        feature = "arrow",
        feature = "inbound",
        feature = "inbound-email",
        feature = "auth",
        feature = "auth-saml",
        feature = "cdc-outbound",
        feature = "subscription-kafka",
    )),
    allow(clippy::missing_const_for_fn)
)]
pub(super) fn of_runtime_key(key: &str) -> Option<Value> {
    match key {
        #[cfg(feature = "sources")]
        "sources" => Some(of::<crate::server_config::SourcesConfig>()),
        #[cfg(feature = "rest")]
        "export" => Some(of::<crate::routes::rest::export_config::ExportConfig>()),
        #[cfg(feature = "observers")]
        "observers" => Some(of::<crate::server_config::ObserverConfig>()),
        #[cfg(feature = "arrow")]
        "flight_bind_addr" => Some(of::<std::net::SocketAddr>()),
        #[cfg(feature = "arrow")]
        "flight_upload_tables" | "flight_views" => Some(of::<Vec<String>>()),
        #[cfg(feature = "inbound")]
        "webhooks" => {
            Some(of::<std::collections::HashMap<String, crate::config::WebhookRouteConfig>>())
        },
        #[cfg(feature = "inbound")]
        "inbound_dispatch" => Some(of::<crate::server_config::InboundDispatchConfig>()),
        #[cfg(feature = "inbound-email")]
        "mailbox" => {
            Some(of::<std::collections::HashMap<String, crate::inbound::email::MailboxConfig>>())
        },
        #[cfg(feature = "inbound-email")]
        "send" => Some(of::<crate::inbound::email::SendSettings>()),
        #[cfg(feature = "auth")]
        "identity" => Some(of::<crate::identity::IdentityConfig>()),
        #[cfg(feature = "auth")]
        "session_state" => {
            Some(of::<crate::server_config::session_state::SessionStateServerConfig>())
        },
        #[cfg(feature = "auth-saml")]
        "saml" => Some(of::<crate::server_config::saml::SamlServerConfig>()),
        #[cfg(feature = "cdc-outbound")]
        "cdc_outbound" => Some(of::<crate::server_config::CdcOutboundConfig>()),
        #[cfg(feature = "subscription-kafka")]
        "subscription_kafka" => {
            Some(of::<crate::server_config::subscription_kafka::SubscriptionKafkaConfig>())
        },
        _ => None,
    }
}
