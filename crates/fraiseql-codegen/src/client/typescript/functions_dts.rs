//! `functions.d.ts` generator — typed guest payloads + host-op declarations.
//!
//! Companion to the `TypeScript` client: a function author references this file to get
//! editor type-checking for both the host surface (`Deno.core.ops.fraiseql_*`) and
//! their function's event payload. The host-op block is schema-independent; the
//! per-function event interfaces are derived from each function's trigger — an
//! `after:mutation`/`after:capture` function on entity `E` gets
//! `{ event_kind, old: E | null, new: E | null }` (the `E` imported from the
//! generated `./types`), a `cron` function gets its schedule context, an
//! `after:ingest` function gets the normalized inbound-message shape.
//!
//! The caller (the CLI) resolves each [`FunctionTypeSpec`] from the compiled schema's
//! `functions` section; this module owns only the rendering, mirroring how
//! [`emit`](super::emit) renders the client files.

use std::{collections::BTreeSet, fmt::Write as _};

use fraiseql_core::schema::CompiledSchema;

use crate::Result;

/// The payload shape a function's trigger implies.
pub enum FunctionPayloadShape {
    /// `after:mutation` / `after:capture` — `{ event_kind, old, new }` over an entity.
    Entity {
        /// The entity type named in the trigger; `None` when it does not name a type in
        /// the compiled schema (payload images then fall back to `unknown`).
        entity: Option<String>,
    },
    /// `cron` — schedule context.
    Cron,
    /// `after:ingest` — a normalized inbound message.
    Ingest,
}

/// One function's name + resolved payload shape, from the compiled schema's
/// `functions` section.
pub struct FunctionTypeSpec {
    /// The function name (as declared).
    pub name:  String,
    /// The payload shape its trigger implies.
    pub shape: FunctionPayloadShape,
}

/// The host-op ambient block — the typed FraiseQL host surface every function
/// reaches via `Deno.core.ops.fraiseql_*`. Schema-independent (identical for every
/// project); mirrors `examples/native-functions/fraiseql-host.d.ts`. Emitted with a
/// `declare global` so it augments the ambient `Deno` namespace even though the file
/// is a module (it exports the per-function event interfaces).
const HOST_OPS: &str = r"/** An HTTP response from `fraiseql_http_request`. */
export interface FraiseqlHttpResponse {
  status: number;
  headers: Array<[string, string]>;
  body: Uint8Array;
}

/** The FraiseQL host operations, reached as `Deno.core.ops.fraiseql_*`. */
export interface FraiseqlHostOps {
  /** Execute a GraphQL query/mutation. `variables` is a JSON string; returns a JSON string. */
  fraiseql_query(graphql: string, variables: string): Promise<string>;
  /** Execute a raw SQL query. `params` is a JSON array string; returns a JSON array string. */
  fraiseql_sql_query(sql: string, params: string): Promise<string>;
  /** Make an outbound HTTP request (SSRF-allowlisted by the host). */
  fraiseql_http_request(
    method: string,
    url: string,
    headers: Array<[string, string]>,
    body: Uint8Array | null,
  ): Promise<FraiseqlHttpResponse>;
  /** Retrieve an object from storage. */
  fraiseql_storage_get(bucket: string, key: string): Promise<Uint8Array>;
  /** Store an object to storage. */
  fraiseql_storage_put(
    bucket: string,
    key: string,
    body: Uint8Array,
    contentType: string,
  ): Promise<void>;
  /** Send an email. `from` is host-owned; the request JSON carries only { to, subject, text?, html?, reply_to? }. */
  fraiseql_send_email(request: string): Promise<string>;
  /** The authenticated caller's context, as a JSON string. */
  fraiseql_auth_context(): string;
  /** Read a host-allowlisted environment variable, or null. */
  fraiseql_env_var(name: string): string | null;
  /** Per-dispatch idempotency token, or null on a non-durably-dispatched invocation. */
  fraiseql_idempotency_token(): string | null;
  /** Structured log. Levels: 0=debug, 1=info, 2=warn, 3=error. */
  fraiseql_log(level: number, message: string): void;
}

declare global {
  namespace Deno {
    namespace core {
      const ops: FraiseqlHostOps;
      /** Encode a string to UTF-8 bytes. */
      function encode(text: string): Uint8Array;
      /** Decode UTF-8 bytes to a string. */
      function decode(bytes: Uint8Array): string;
    }
  }
}
";

/// Render `functions.d.ts` for the given function specs.
///
/// The file is a module (it exports the per-function event interfaces and the host-op
/// interfaces), so the ambient `Deno` augmentation is inside a `declare global`. Entity
/// payload images import their type from the generated `./types`.
///
/// # Errors
///
/// Returns an error if the schema cannot be hashed for the auto-generated stamp.
pub fn generate_functions_dts(
    schema: &CompiledSchema,
    specs: &[FunctionTypeSpec],
) -> Result<String> {
    let hash = crate::client::schema_hash(schema)?;

    let mut body = String::new();

    // Import exactly the entity types the payloads reference, from the client's types.
    let entities: BTreeSet<&str> = specs
        .iter()
        .filter_map(|spec| match &spec.shape {
            FunctionPayloadShape::Entity { entity: Some(name) } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if !entities.is_empty() {
        let list = entities.into_iter().collect::<Vec<_>>().join(", ");
        let _ = writeln!(body, "import type {{ {list} }} from \"./types\";\n");
    }

    body.push_str(HOST_OPS);

    for spec in specs {
        let ty = format!("{}Event", pascal_case(&spec.name));
        let _ = writeln!(body, "\n/** Event payload for the `{}` function. */", spec.name);
        match &spec.shape {
            FunctionPayloadShape::Entity { entity } => {
                let image = entity.as_deref().unwrap_or("unknown");
                let _ = writeln!(body, "export interface {ty} {{");
                let _ = writeln!(body, "  event_kind: \"insert\" | \"update\" | \"delete\";");
                let _ = writeln!(body, "  old: {image} | null;");
                let _ = writeln!(body, "  new: {image} | null;");
                let _ = writeln!(body, "}}");
            },
            FunctionPayloadShape::Cron => {
                let _ = writeln!(body, "export interface {ty} {{");
                let _ = writeln!(body, "  schedule: string;");
                let _ = writeln!(body, "  timezone: string;");
                let _ = writeln!(body, "  executed_at: string;");
                let _ = writeln!(body, "}}");
            },
            FunctionPayloadShape::Ingest => {
                let _ = writeln!(body, "export interface {ty} {{");
                let _ = writeln!(body, "  source: string;");
                let _ = writeln!(body, "  idempotency_key: string;");
                let _ = writeln!(body, "  subject: string | null;");
                let _ = writeln!(body, "  payload: unknown;");
                let _ = writeln!(body, "}}");
            },
        }
    }

    Ok(super::stamp(&body, &hash))
}

/// `PascalCase` a function name for a type name: split on `_`/`-`, capitalize each
/// segment's first character (preserving any interior `camelCase`). `notify_approved`
/// → `NotifyApproved`, `syncDeal` → `SyncDeal`.
fn pascal_case(name: &str) -> String {
    name.split(['_', '-'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
