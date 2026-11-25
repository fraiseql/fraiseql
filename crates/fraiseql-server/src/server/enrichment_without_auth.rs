//! A build without the `auth` feature resolves no enriched identity: it carries no
//! `[identity.enrichment]` resolver, and no transport runs the enrichment step before
//! dispatch. A compiled schema that reads the enriched identity is refused at boot, on a
//! reload and at tenant registration, as a build with `auth` refuses one when no
//! resolver is configured.

use fraiseql_core::schema::CompiledSchema;

/// Refuse a schema that reads the enriched identity in a build that cannot resolve one.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` naming the enriched fields the schema reads.
pub fn enrichment_readers_need_auth_check(schema: &CompiledSchema) -> crate::Result<()> {
    if !schema.declares_enrichment_consumer() {
        return Ok(());
    }
    let read = schema
        .enriched_fields_read()
        .into_iter()
        .map(|field| format!("`{field}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Err(crate::ServerError::ConfigError(format!(
        "The compiled schema reads the enriched fields {read} (through an `enrichment`-sourced \
         session variable or inject param, a [locale] source, or a subscription policy), but \
         this server was built without the `auth` feature, which resolves the enriched \
         identity. Nothing would resolve one, so every request reading an enriched field \
         would fail and every other request would be served without the fail-closed check \
         the schema implies. Build the server with the `auth` feature and enable \
         `[identity.enrichment]`, or remove the readers from the schema."
    )))
}
