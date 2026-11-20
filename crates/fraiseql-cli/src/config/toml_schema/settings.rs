//! The settings of a `fraiseql.toml`, applied to a schema document.
//!
//! One function for both roles the file plays: the compile input with its types from a
//! document (`--types`, a schema directory, includes), and the file read beside an SDK's
//! `schema.json`. Each table the file writes meets a copy the document carries the same
//! way in both: where the two set a key, they must agree; where only the document does,
//! its copy stands.

use anyhow::Result;

use super::TomlSchema;
use crate::schema::intermediate::IntermediateSchema;

/// What a refusal calls each file.
#[derive(Debug, Clone, Copy)]
pub struct Labels<'a> {
    /// The `fraiseql.toml`.
    pub toml:     &'a str,
    /// The schema document.
    pub document: &'a str,
}

impl TomlSchema {
    /// Apply this file's settings to `schema`, reconciling each with the copy the document
    /// carries.
    ///
    /// `document_declares_naming` says whether the document writes its own
    /// `naming_convention`, which the parsed schema cannot tell from its default.
    ///
    /// # Errors
    ///
    /// A table the file writes that disagrees with the document's copy, a section either
    /// file carries that fails its checks, or a `[[caching.rules]]` entry naming an
    /// operation the document does not declare.
    pub(crate) fn apply_settings(
        &self,
        schema: &mut IntermediateSchema,
        labels: Labels<'_>,
        document_declares_naming: bool,
    ) -> Result<()> {
        self.security.warn_pkce_without_state_encryption();

        // A `security` block in the document is merged with this section, key by key: a
        // key both set must agree, a key only the document sets is kept. The row-tenancy
        // check reads the merged section.
        schema.security = Some(crate::config::security_merge::reconcile(
            labels.toml,
            serde_json::to_value(self.security.to_compiled(&self.tenancy))?,
            &self.declared_security,
            schema.security.take(),
        )?);

        // Session variables are the mechanism RLS policies read.
        if self.declares("session_variables") {
            super::runtime_sections::reconcile_values(
                "session_variables",
                "session_variables",
                &serde_json::to_value(&self.session_variables)?,
                schema
                    .session_variables
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()?
                    .as_ref(),
                labels.document,
            )?;
            schema.session_variables = Some(self.session_variables.clone());
        }

        // `[locale]` is configuration; a document carrying another one is an
        // authoring conflict.
        if let Some(locale) = self.locale.clone() {
            if schema.locale.as_ref().is_some_and(|l| *l != locale) {
                return Err(crate::config::refusal(
                    fraiseql_core::utils::config_diagnostic::DiagnosticCode::Disagrees,
                    "locale",
                    "",
                    format!(
                        "[locale] in {} disagrees with the `locale` {} declares; declare it \
                         once, in fraiseql.toml",
                        labels.toml, labels.document
                    ),
                ));
            }
            schema.locale = Some(locale);
        }

        schema.functions_config = self.functions.reconcile(schema.functions_config.take())?;

        if self.declares("observers")
            && self.observers.backend == fraiseql_core::schema::ObserversBackend::Nats
            && self.observers.nats_url.is_none()
        {
            tracing::warn!(
                "observers.backend is \"nats\" but observers.nats_url is not set; the runtime \
                 will require FRAISEQL_NATS_URL to be configured"
            );
        }
        schema.observers_config = super::ObserversConfig::reconcile(
            self.declares("observers").then_some(&self.observers),
            schema.observers_config.take(),
        )?;

        // The runtime sections (`[mcp]`, `[rest]`, `[federation]`, `[auth]`, …).
        for section in self.lower_runtime_sections(&|table| self.declares(table))? {
            let document = schema.runtime_section(section.key)?;
            if let Some(value) = section.reconcile(document, labels.document)? {
                schema.set_runtime_section(section.key, value)?;
            }
        }
        // A copy only the document carries is held to the same checks.
        for (_, key) in super::runtime_sections::RUNTIME_SECTIONS {
            if let Some(copy) = schema.runtime_section(key)? {
                super::runtime_sections::check_document_copy(key, &copy)?;
            }
        }

        // `[[caching.rules]]` lower onto the document's own queries and mutations.
        crate::schema::merger::apply_caching_rules_to_schema(&self.caching, schema)?;

        // `[inject_defaults]`, which the SDK loaders read too.
        schema.inject_defaults = crate::config::inject_defaults::InjectDefaultsToml::reconcile(
            self.inject_defaults.as_ref(),
            schema.inject_defaults.take(),
        )?;

        // `[naming] convention` decides when written; otherwise the document's own
        // convention stands, and camelCase where neither says.
        schema.naming_convention = self
            .naming
            .convention_over(document_declares_naming.then_some(schema.naming_convention));

        check_pkce_client(schema)
    }
}

/// `[security.pkce]` mounts `/auth/start` and `/auth/callback`, which need the `[auth]`
/// PKCE client. Read from the merged sections, so either file may write either one; the
/// server refuses the same schema at boot.
fn check_pkce_client(schema: &IntermediateSchema) -> Result<()> {
    let enabled = schema
        .security
        .as_ref()
        .and_then(|security| security.pointer("/pkce/enabled"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let client = schema
        .auth
        .as_ref()
        .and_then(|auth| auth.get("pkce"))
        .is_some_and(|client| !client.is_null());
    if enabled && !client {
        anyhow::bail!(
            "[security.pkce] enabled = true, but [auth] declares no PKCE client: \
             /auth/start and /auth/callback need one. Add discovery_url, client_id, \
             client_secret_env and server_redirect_uri to [auth], or disable [security.pkce]"
        );
    }
    Ok(())
}
