//! The runtime sections of a `fraiseql.toml`, lowered once for both workflows (#1719).
//!
//! `[mcp]`, `[rest]`, `[grpc]`, `[debug]`, `[validation]`, `[subscriptions]`,
//! `[federation]`, `[auth]`, `[changelog]`, `[hierarchies]` and `[query_defaults]` each
//! compile to one section of the schema. A schema document may carry its own copy of that
//! section, under the key named here. Two authors of one setting is where a compile used to
//! pick one silently, so there is one lowering ([`TomlSchema::lower_runtime_sections`]) and
//! one reconciliation ([`LoweredSection::reconcile`]), and both workflows go through them:
//! a TOML schema with its types document, and a project `fraiseql.toml` beside an SDK
//! `schema.json`.

use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::TomlSchema;

/// Each runtime section: its TOML table, and the key a schema document carries it under.
pub const RUNTIME_SECTIONS: &[(&str, &str)] = &[
    ("federation", "federation_config"),
    ("auth", "auth"),
    ("subscriptions", "subscriptions_config"),
    ("validation", "validation_config"),
    ("debug", "debug_config"),
    ("mcp", "mcp_config"),
    ("rest", "rest_config"),
    ("grpc", "grpc_config"),
    ("changelog", "changelog_config"),
    ("hierarchies", "hierarchies_config"),
    ("query_defaults", "query_defaults"),
];

/// One declared runtime section, as the schema reads it.
#[derive(Debug, Clone)]
pub struct LoweredSection {
    /// The TOML table (`mcp`).
    pub table: &'static str,
    /// The key of the schema document's copy (`mcp_config`).
    pub key:   &'static str,
    /// The section's value under that key.
    pub value: Value,
    /// Whether the section is written to the schema. A transport declared `enabled = false`
    /// is compared with the document's copy and written nowhere, as before.
    pub emit:  bool,
}

/// Whether `value` is what its type defaults to: the test for "the author wrote nothing"
/// where the grammar cannot tell an absent table from one left at its defaults.
pub fn is_default<T: Serialize + Default>(value: &T) -> bool {
    serde_json::to_value(value).ok() == serde_json::to_value(T::default()).ok()
}

impl TomlSchema {
    /// Whether this `fraiseql.toml` declares section `table`.
    ///
    /// A table the file writes is declared, at its defaults or not. A schema built in code
    /// rather than parsed records no written tables, so for one a table that differs from
    /// its defaults is declared too.
    pub fn declares(&self, table: &str) -> bool {
        if self.declared_tables.contains(table) {
            return true;
        }
        match table {
            "federation" => !is_default(&self.federation),
            "auth" => self.auth.is_some(),
            "subscriptions" => !is_default(&self.subscriptions),
            "validation" => !is_default(&self.validation),
            "debug" => !is_default(&self.debug),
            "mcp" => !is_default(&self.mcp),
            "rest" => !is_default(&self.rest),
            "grpc" => !is_default(&self.grpc),
            "changelog" => self.changelog.is_some(),
            "hierarchies" => self.hierarchies.is_some(),
            "query_defaults" => !is_default(&self.query_defaults),
            _ => false,
        }
    }

    /// Lower each runtime section `declared` names to the value the schema reads.
    ///
    /// The one place a section's checks and its compiled shape are decided, whichever file
    /// the section was written in.
    ///
    /// # Errors
    ///
    /// What a section's own checks refuse: a `[rest]` path that does not start with `/`, a
    /// `[grpc]` with no descriptor or a zero batch size.
    pub fn lower_runtime_sections(
        &self,
        declared: &dyn Fn(&str) -> bool,
    ) -> Result<Vec<LoweredSection>> {
        let mut sections = Vec::new();
        for (table, key) in RUNTIME_SECTIONS {
            if !declared(table) {
                continue;
            }
            let (value, emit) = match *table {
                // Lowered into the compiled shape explicitly: a raw passthrough dropped
                // `service_name`/`version` and the `per_database` → `per_entity` overrides.
                "federation" => {
                    (to_json(table, &self.federation.to_compiled())?, self.federation.enabled)
                },
                "auth" => {
                    let auth = self.auth_section()?;
                    let written = auth.as_object().is_some_and(|groups| !groups.is_empty());
                    (auth, written)
                },
                "subscriptions" => {
                    let value = to_json(table, &self.subscriptions)?;
                    let written = value != serde_json::json!({});
                    (value, written)
                },
                "validation" => {
                    let value = to_json(table, &self.validation)?;
                    let written = value != serde_json::json!({});
                    (value, written)
                },
                "debug" => (to_json(table, &self.debug)?, self.debug.enabled),
                "mcp" => (to_json(table, &self.mcp)?, self.mcp.enabled),
                "rest" => {
                    let rest: fraiseql_core::schema::RestConfig = self.rest.clone().into();
                    check_rest(&rest)?;
                    (to_json(table, &rest)?, rest.enabled)
                },
                "grpc" => {
                    check_grpc(&self.grpc)?;
                    (to_json(table, &self.grpc)?, self.grpc.enabled)
                },
                "changelog" => (to_json(table, &self.changelog)?, true),
                "hierarchies" => (to_json(table, &self.hierarchies)?, true),
                "query_defaults" => (
                    to_json(
                        table,
                        &crate::schema::intermediate::IntermediateQueryDefaults {
                            where_clause:     self.query_defaults.where_clause,
                            order_by:         self.query_defaults.order_by,
                            limit:            self.query_defaults.limit,
                            offset:           self.query_defaults.offset,
                            pagination_order: self.query_defaults.pagination_order,
                        },
                    )?,
                    true,
                ),
                _ => continue,
            };
            sections.push(LoweredSection {
                table,
                key,
                value,
                emit,
            });
        }
        Ok(sections)
    }

    /// The `[auth]` OAuth-client groups as the compiled `auth` object.
    ///
    /// The PKCE group is all-four-or-none (`OidcClientConfig::validate`), so if `client_id`
    /// is present the whole group is; it lowers into `auth.pkce`, which carries the client
    /// identity and the provider `discovery_url` only: the runtime resolves the endpoints
    /// at boot, and the secret is read from `client_secret_env`, never embedded. The other
    /// groups are authored in the compiled type and embedded as written.
    fn auth_section(&self) -> Result<Value> {
        let mut groups = serde_json::Map::new();
        let Some(auth) = &self.auth else {
            return Ok(Value::Object(groups));
        };
        if let (Some(discovery_url), Some(client_id), Some(client_secret_env), Some(redirect)) = (
            auth.discovery_url.as_ref(),
            auth.client_id.as_ref(),
            auth.client_secret_env.as_ref(),
            auth.server_redirect_uri.as_ref(),
        ) {
            groups.insert(
                "pkce".to_string(),
                serde_json::json!({
                    "discovery_url": discovery_url,
                    "client_id": client_id,
                    "client_secret_env": client_secret_env,
                    "server_redirect_uri": redirect,
                }),
            );
        }
        if let Some(social) = &auth.social {
            groups.insert("social".to_string(), to_json("auth.social", social)?);
        }
        if let Some(local) = &auth.local {
            groups.insert("local".to_string(), to_json("auth.local", local)?);
        }
        if let Some(session) = &auth.session {
            groups.insert("session".to_string(), to_json("auth.session", session)?);
        }
        if let Some(database) = &auth.database {
            groups.insert("database".to_string(), to_json("auth.database", database)?);
        }
        Ok(Value::Object(groups))
    }
}

fn to_json<T: Serialize>(table: &str, value: &T) -> Result<Value> {
    serde_json::to_value(value).with_context(|| format!("Failed to serialize [{table}]"))
}

/// A REST transport that is on is mounted under a path, which starts with `/`.
fn check_rest(rest: &fraiseql_core::schema::RestConfig) -> Result<()> {
    if rest.enabled && !rest.path.starts_with('/') {
        bail!("REST config: `path` must start with '/' (got {:?})", rest.path);
    }
    Ok(())
}

/// A gRPC transport that is on reads its `FileDescriptorSet` from `descriptor_path`, so an
/// empty one guarantees a server that cannot mount the transport it was told to enable.
fn check_grpc(grpc: &fraiseql_core::schema::GrpcConfig) -> Result<()> {
    if !grpc.enabled {
        return Ok(());
    }
    if grpc.descriptor_path.trim().is_empty() {
        bail!(
            "[grpc] enabled = true requires `descriptor_path`, the path to the \
             FileDescriptorSet binary (.binpb) the transport serves. Generate one with `protoc \
             --descriptor_set_out`, or set `enabled = false`."
        );
    }
    if grpc.stream_batch_size == 0 {
        bail!(
            "[grpc] stream_batch_size must be greater than 0 (got 0) — a zero batch size would \
             make every server-streaming response empty."
        );
    }
    Ok(())
}

/// Run the checks a section's lowering makes on the copy a schema document carries under
/// `key`, so a setting is held to them whichever file it was written in.
///
/// # Errors
///
/// What [`TomlSchema::lower_runtime_sections`] refuses for the same section. A copy that
/// does not have the section's shape is left to the converter, which names it.
pub fn check_document_copy(key: &str, copy: &Value) -> Result<()> {
    match key {
        "rest_config" => {
            serde_json::from_value(copy.clone()).map_or(Ok(()), |rest| check_rest(&rest))
        },
        "grpc_config" => {
            serde_json::from_value(copy.clone()).map_or(Ok(()), |grpc| check_grpc(&grpc))
        },
        _ => Ok(()),
    }
}

/// `value` read as `T` and written back: the section with every default spelled, so two
/// copies that say the same thing compare equal however much each one wrote.
fn spelled_out<T: Serialize + DeserializeOwned>(value: &Value) -> Option<Value> {
    serde_json::from_value::<T>(value.clone())
        .ok()
        .and_then(|typed| serde_json::to_value(typed).ok())
}

/// The section under `key` with its defaults spelled, or as written when it does not have
/// the section's shape (the converter refuses that, by name).
fn normalised(key: &str, value: &Value) -> Value {
    use fraiseql_core::schema as core;
    let spelled = match key {
        "federation_config" => spelled_out::<core::FederationConfig>(value),
        "auth" => spelled_out::<core::AuthClientConfig>(value),
        "subscriptions_config" => spelled_out::<core::SubscriptionsConfig>(value),
        "validation_config" => spelled_out::<core::ValidationConfig>(value),
        "debug_config" => spelled_out::<core::DebugConfig>(value),
        "mcp_config" => spelled_out::<core::McpConfig>(value),
        "rest_config" => spelled_out::<core::RestConfig>(value),
        "grpc_config" => spelled_out::<core::GrpcConfig>(value),
        "changelog_config" => spelled_out::<core::ChangelogConfig>(value),
        "hierarchies_config" => spelled_out::<core::HierarchiesConfig>(value),
        "query_defaults" => {
            spelled_out::<crate::schema::intermediate::IntermediateQueryDefaults>(value)
        },
        "session_variables" => spelled_out::<core::SessionVariablesConfig>(value),
        _ => None,
    };
    spelled.unwrap_or_else(|| value.clone())
}

/// The first key `ours` and `theirs` differ on, as a dotted path; `None` when they agree.
fn first_difference(ours: &Value, theirs: &Value) -> Option<String> {
    if ours == theirs {
        return None;
    }
    let (Some(ours), Some(theirs)) = (ours.as_object(), theirs.as_object()) else {
        return Some(String::new());
    };
    let mut keys: Vec<&String> = ours.keys().chain(theirs.keys()).collect();
    keys.sort_unstable();
    keys.dedup();
    keys.into_iter().find_map(|key| {
        let absent = Value::Null;
        let inner =
            first_difference(ours.get(key).unwrap_or(&absent), theirs.get(key).unwrap_or(&absent))?;
        Some(if inner.is_empty() {
            key.clone()
        } else {
            format!("{key}.{inner}")
        })
    })
}

/// Reconcile a setting declared in `fraiseql.toml` (`[table]`, lowered to `ours`) with the
/// copy `document` carries under `key`.
///
/// One source alone applies. Two that agree compile. Two that differ are refused, naming
/// both sources, the first key they differ on, and what each says: neither is preferred.
///
/// # Errors
///
/// When the two sources differ.
pub fn reconcile_values(
    table: &str,
    key: &str,
    ours: &Value,
    document: Option<&Value>,
    document_name: &str,
) -> Result<()> {
    let Some(theirs) = document else {
        return Ok(());
    };
    let (ours, theirs) = (normalised(key, ours), normalised(key, theirs));
    let Some(differing) = first_difference(&ours, &theirs) else {
        return Ok(());
    };
    let differing = if differing.is_empty() {
        key
    } else {
        differing.as_str()
    };
    bail!(
        "fraiseql.toml [{table}] and {document_name} {key} disagree on `{differing}`.\n  \
         fraiseql.toml [{table}]: {ours}\n  {document_name} {key}: {theirs}\nDeclare the \
         {table} settings once, in fraiseql.toml."
    )
}

impl LoweredSection {
    /// Reconcile this section with the copy `document` carries, and answer what the schema
    /// reads under [`Self::key`]: this section's value when it is written, the document's
    /// agreeing copy when it is not (a transport both sources leave off), nothing when
    /// neither writes one.
    ///
    /// # Errors
    ///
    /// When the two sources differ ([`reconcile_values`]).
    pub fn reconcile(&self, document: Option<Value>, document_name: &str) -> Result<Option<Value>> {
        reconcile_values(self.table, self.key, &self.value, document.as_ref(), document_name)?;
        Ok(if self.emit {
            Some(self.value.clone())
        } else {
            document
        })
    }
}
