//! `fraiseql functions invoke` — a local test harness for function authors.
//!
//! Runs a compiled function in a **real V8 isolate** against a fixture payload with
//! **mocked host ops**, printing the guest's result and every host-op call it made.
//! This is the author's inner loop: fixture → run → observe, without a live server,
//! database, or network.
//!
//! The module loads exactly as the server does — one `FunctionModule` read from the
//! compiled schema's `module_dir`, dispatched through a `FunctionObserver` with the
//! Deno runtime registered (mirroring
//! `fraiseql-server::subsystems::loader::build_functions_subsystem`). The host is a
//! recording mock ([`RecordingHost`]): each op is logged, HTTP/query responses come
//! from `--mock-http` / `--mock-query` (a matched entry → canned response; a miss
//! against a configured mock fails loud), and other ops return benign defaults so a
//! smoke-run surfaces which ops a function calls before its mocks are written.
//!
//! **One isolate per process** (the V8 constraint documented in
//! `docs/architecture/functions.md`): a single `invoke` call is inherently safe;
//! only harness *tests* that run two invocations must fork per test (nextest).

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, anyhow, bail};
use fraiseql_functions::{
    EventPayload, FunctionDefinition, FunctionModule, FunctionObserver, LogLevel, ResourceLimits,
    RuntimeType,
    host::{HostContext, HttpResponse, dyn_context::DynHostContext},
    runtime::deno::{DenoConfig, DenoRuntime},
    triggers::{
        mutation::{EventKind, TriggerPredicate},
        registry::ParsedTrigger,
    },
};
use serde::Deserialize;

/// Exit codes, scriptable in an adopter's CI (`functions.md`).
///
/// A harness/config error (module missing, bad fixture) surfaces as the default
/// `anyhow` error path (exit 1); these are the *outcome* codes.
pub mod exit {
    /// The guest ran and returned.
    pub const OK: i32 = 0;
    /// The `when` predicate did not match — nothing would fire; the guest was not run.
    pub const PREDICATE_NO_MATCH: i32 = 3;
    /// The guest ran but errored (threw, or the runtime rejected it).
    pub const GUEST_ERROR: i32 = 4;
}

/// The `functions` section of a compiled schema (the subset the harness needs).
///
/// A local mirror of `fraiseql-server`'s `FunctionsConfig` so the harness does not
/// pull the whole server crate in; the JSON shape is identical (it is the same
/// compiled-schema `"functions"` object).
#[derive(Debug, Deserialize)]
struct FunctionsSection {
    module_dir:  std::path::PathBuf,
    definitions: Vec<FunctionDefinition>,
}

/// Run `fraiseql functions invoke`.
///
/// # Errors
///
/// Returns a config/harness error (exit 1) if the schema or fixture cannot be read,
/// the function is unknown, its module file is missing, or the trigger kind is not
/// yet supported by the harness. A successful load returns `Ok(code)` with the
/// outcome [`exit`] code — the caller maps it to the process exit.
#[allow(clippy::too_many_arguments)] // Reason: a CLI command's flags; a struct would just relocate them.
pub async fn invoke(
    name: &str,
    payload_path: &Path,
    schema_path: &Path,
    mock_http_path: Option<&Path>,
    mock_query_path: Option<&Path>,
    idempotency_token: Option<String>,
    explain: bool,
    json: bool,
) -> Result<i32> {
    // ── Load the compiled schema's functions section + the named definition. ──
    let functions = load_functions_section(schema_path)?;
    let definition = functions
        .definitions
        .iter()
        .find(|def| def.name == name)
        .ok_or_else(|| anyhow!("no function named {name:?} in {}", schema_path.display()))?;

    let trigger = ParsedTrigger::parse(&definition.trigger)
        .map_err(|error| anyhow!("function {name:?} has an unparseable trigger: {error}"))?;

    // ── Synthesize the dispatch payload from the fixture, per trigger kind. ───
    let fixture: serde_json::Value = read_json(payload_path).context("reading --payload")?;
    let synth = synthesize_payload(name, &trigger, &fixture)?;

    // ── Predicates (#597): evaluate before running anything (--explain). ─────
    if let Some(outcome) = evaluate_predicates(&definition.when, &synth, explain, json) {
        if !outcome {
            // A false predicate is the dispatcher's zero-cost skip: no isolate spins.
            return Ok(exit::PREDICATE_NO_MATCH);
        }
    }

    // ── Load the module exactly like the server, register the Deno runtime. ──
    let module = load_module(&functions.module_dir, definition)?;
    if module.runtime != RuntimeType::Deno {
        bail!(
            "the invoke harness runs TypeScript/JavaScript (Deno) functions; {name:?} is {:?}",
            module.runtime
        );
    }
    let mut observer = FunctionObserver::new();
    observer.register_runtime(RuntimeType::Deno, DenoRuntime::new(&DenoConfig::default())?);

    // ── Build the recording mock host from the fixtures. ─────────────────────
    let mocks = HostMocks::load(mock_http_path, mock_query_path)?;
    let host = Arc::new(RecordingHost::new(synth.payload.clone(), mocks, idempotency_token));
    let dyn_host: Arc<dyn DynHostContext> = host.clone();

    // ── Run in a real isolate. ───────────────────────────────────────────────
    let result = observer
        .invoke_with_context(&module, synth.payload, dyn_host, ResourceLimits::default())
        .await;

    let calls = host.calls();
    match result {
        Ok(function_result) => {
            print_success(name, &function_result, &calls, json);
            Ok(exit::OK)
        },
        Err(error) => {
            print_guest_error(name, &error, &calls, json);
            Ok(exit::GUEST_ERROR)
        },
    }
}

/// Read + deserialize a JSON file with a clear error.
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing JSON in {}", path.display()))
}

/// Parse the compiled schema and extract its `functions` section.
fn load_functions_section(schema_path: &Path) -> Result<FunctionsSection> {
    let schema: serde_json::Value = read_json(schema_path).context("reading --schema")?;
    let functions = schema
        .get("functions")
        .filter(|value| !value.is_null())
        .ok_or_else(|| anyhow!("{} has no \"functions\" section", schema_path.display()))?;
    serde_json::from_value(functions.clone())
        .with_context(|| format!("parsing the functions section of {}", schema_path.display()))
}

/// Load the `FunctionModule` from `module_dir`, mirroring the server's loader
/// (`build_functions_subsystem` → `load_one_module`): try each supported extension,
/// first existing file wins, fail loud otherwise.
fn load_module(module_dir: &Path, definition: &FunctionDefinition) -> Result<FunctionModule> {
    for extension in definition.runtime.supported_extensions() {
        let path = module_dir.join(format!("{}{extension}", definition.name));
        if !path.exists() {
            continue;
        }
        return match definition.runtime {
            RuntimeType::Deno => {
                let source = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading module {}", path.display()))?;
                Ok(FunctionModule::from_source(definition.name.clone(), source, RuntimeType::Deno))
            },
            RuntimeType::Wasm => {
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("reading module {}", path.display()))?;
                Ok(FunctionModule::from_bytecode(definition.name.clone(), bytes.into()))
            },
            other => bail!("unsupported function runtime {other:?}"),
        };
    }
    bail!(
        "no module file for {:?} in {} (looked for {:?})",
        definition.name,
        module_dir.display(),
        definition.runtime.supported_extensions(),
    )
}

/// A synthesized dispatch payload plus the row images the predicates evaluate on.
struct Synthesized {
    payload: EventPayload,
    /// The pre/post images (after:mutation/after:capture) for `--explain`; `None`
    /// for kinds that carry no row images.
    old:     Option<serde_json::Value>,
    new:     Option<serde_json::Value>,
}

/// Build the dispatch payload from a fixture, validated against the trigger kind.
///
/// - **after:mutation / after:capture** — the fixture is `{ "event_kind": "insert|update|delete",
///   "old": {…}|null, "new": {…}|null }` (or a bare object, treated as an insert's `new`).
///   Synthesized via the same `{event_kind, old, new}` payload the dispatcher builds.
/// - other kinds are not yet supported by the harness (see the follow-up note).
fn synthesize_payload(
    name: &str,
    trigger: &ParsedTrigger,
    fixture: &serde_json::Value,
) -> Result<Synthesized> {
    match trigger {
        ParsedTrigger::AfterMutation { entity_type, .. }
        | ParsedTrigger::AfterCapture { entity_type, .. } => {
            let (kind_prefix, entity) = match trigger {
                ParsedTrigger::AfterCapture { .. } => ("after:capture", entity_type),
                _ => ("after:mutation", entity_type),
            };
            let (event_kind, old, new) = mutation_images(fixture)?;
            let data = serde_json::json!({
                "event_kind": event_kind.as_str(),
                "old": old,
                "new": new,
            });
            Ok(Synthesized {
                payload: EventPayload {
                    trigger_type: format!("{kind_prefix}:{name}"),
                    entity: entity.clone(),
                    event_kind: event_kind.to_string(),
                    data,
                    timestamp: chrono::Utc::now(),
                },
                old,
                new,
            })
        },
        other => bail!(
            "the invoke harness does not yet synthesize a payload for {other:?} — supported \
             kinds: after:mutation, after:capture. (cron / after:ingest are a tracked follow-up.)"
        ),
    }
}

/// Extract `(event_kind, old, new)` from an after:mutation fixture.
///
/// A bare object (no `event_kind`/`old`/`new` keys) is treated as an insert's `new`
/// image — the common quick-fixture shape.
fn mutation_images(
    fixture: &serde_json::Value,
) -> Result<(EventKind, Option<serde_json::Value>, Option<serde_json::Value>)> {
    let obj = fixture
        .as_object()
        .ok_or_else(|| anyhow!("an after:mutation fixture must be a JSON object"))?;

    let has_shape =
        obj.contains_key("event_kind") || obj.contains_key("old") || obj.contains_key("new");
    if !has_shape {
        // Bare object → an insert carrying it as the new image.
        return Ok((EventKind::Insert, None, Some(fixture.clone())));
    }

    let event_kind = match obj.get("event_kind").and_then(serde_json::Value::as_str) {
        Some("insert") => EventKind::Insert,
        Some("update") => EventKind::Update,
        Some("delete") => EventKind::Delete,
        None => EventKind::Update,
        Some(other) => bail!("unknown event_kind {other:?} (insert | update | delete)"),
    };
    let take = |key: &str| obj.get(key).filter(|v| !v.is_null()).cloned();
    Ok((event_kind, take("old"), take("new")))
}

/// Evaluate the function's `when` predicates for `--explain`, printing a per-predicate
/// verdict. Returns `Some(held)` for a kind that carries row images, `None` when there
/// are no predicates to evaluate (always fires).
fn evaluate_predicates(
    predicates: &[TriggerPredicate],
    synth: &Synthesized,
    explain: bool,
    json: bool,
) -> Option<bool> {
    if predicates.is_empty() {
        if explain && !json {
            println!("predicates: none declared — the function fires on every matching event.");
        }
        return None;
    }

    let mut all_hold = true;
    let mut lines = Vec::new();
    for predicate in predicates {
        let held = predicate.matches(synth.old.as_ref(), synth.new.as_ref());
        all_hold &= held;
        lines.push((predicate, held));
    }

    if explain {
        if json {
            let report: Vec<_> = lines
                .iter()
                .map(|(p, held)| serde_json::json!({ "field": p.field, "holds": held }))
                .collect();
            println!(
                "{}",
                serde_json::json!({ "predicates_hold": all_hold, "predicates": report })
            );
        } else {
            println!("predicates ({}):", if all_hold { "MATCH" } else { "NO MATCH" });
            for (predicate, held) in &lines {
                let op = predicate
                    .eq
                    .as_ref()
                    .map(|v| format!("eq {v}"))
                    .or_else(|| predicate.changed_to.as_ref().map(|v| format!("changed_to {v}")))
                    .unwrap_or_default();
                println!("  [{}] {} {}", if *held { "✓" } else { "✗" }, predicate.field, op);
            }
        }
    }
    Some(all_hold)
}

// ── Mock host ────────────────────────────────────────────────────────────────

/// A recorded host-op call, for the output.
#[derive(Debug, Clone, serde::Serialize)]
struct HostOpCall {
    op:      String,
    /// A short, human-readable summary of the call arguments.
    summary: String,
}

/// Canned HTTP/query responses loaded from `--mock-http` / `--mock-query`.
#[derive(Default)]
struct HostMocks {
    http:  Option<Vec<HttpMock>>,
    query: Option<Vec<QueryMock>>,
}

#[derive(Debug, Deserialize)]
struct HttpMock {
    /// Match when the request URL contains this substring.
    url_contains: String,
    /// Optional method filter (case-insensitive).
    #[serde(default)]
    method:       Option<String>,
    #[serde(default = "default_status")]
    status:       u16,
    #[serde(default)]
    body:         serde_json::Value,
}

const fn default_status() -> u16 {
    200
}

#[derive(Debug, Deserialize)]
struct QueryMock {
    /// Match when the GraphQL/SQL text contains this substring; omit to match any.
    #[serde(default)]
    query_contains: Option<String>,
    /// The canned response value returned to the guest.
    response:       serde_json::Value,
}

impl HostMocks {
    fn load(http_path: Option<&Path>, query_path: Option<&Path>) -> Result<Self> {
        let http = http_path.map(read_json::<Vec<HttpMock>>).transpose().context("--mock-http")?;
        let query = query_path
            .map(read_json::<Vec<QueryMock>>)
            .transpose()
            .context("--mock-query")?;
        Ok(Self { http, query })
    }
}

/// A `HostContext` that records every op call and answers HTTP/query from the mocks.
struct RecordingHost {
    event:             EventPayload,
    mocks:             HostMocks,
    idempotency_token: Option<String>,
    calls:             Mutex<Vec<HostOpCall>>,
    logs:              Mutex<Vec<String>>,
}

impl RecordingHost {
    fn new(event: EventPayload, mocks: HostMocks, idempotency_token: Option<String>) -> Self {
        Self {
            event,
            mocks,
            idempotency_token,
            calls: Mutex::new(Vec::new()),
            logs: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, op: &str, summary: String) {
        self.calls.lock().expect("calls mutex poisoned").push(HostOpCall {
            op: op.to_string(),
            summary,
        });
    }

    fn calls(&self) -> Vec<HostOpCall> {
        self.calls.lock().expect("calls mutex poisoned").clone()
    }
}

impl HostContext for RecordingHost {
    async fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> fraiseql_error::Result<serde_json::Value> {
        self.record("query", summarize(graphql));
        match self.mocks.query.as_ref() {
            Some(mocks) => mocks
                .iter()
                .find(|m| m.query_contains.as_ref().is_none_or(|c| graphql.contains(c.as_str())))
                .map(|m| m.response.clone())
                .ok_or_else(|| {
                    fraiseql_error::FraiseQLError::validation(format!(
                        "--mock-query has no entry matching query {:?} (vars {variables})",
                        summarize(graphql)
                    ))
                }),
            // No mock configured → benign default so the call is visible in the output.
            None => Ok(serde_json::json!({ "data": null })),
        }
    }

    async fn sql_query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> fraiseql_error::Result<Vec<serde_json::Value>> {
        self.record("sql_query", summarize(sql));
        Ok(Vec::new())
    }

    async fn http_request(
        &self,
        method: &str,
        url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> fraiseql_error::Result<HttpResponse> {
        self.record("http_request", format!("{method} {url}"));
        match self.mocks.http.as_ref() {
            Some(mocks) => mocks
                .iter()
                .find(|m| {
                    url.contains(&m.url_contains)
                        && m.method.as_ref().is_none_or(|wm| wm.eq_ignore_ascii_case(method))
                })
                .map(|m| HttpResponse {
                    status:  m.status,
                    headers: Vec::new(),
                    body:    m.body.to_string().into_bytes(),
                })
                .ok_or_else(|| {
                    fraiseql_error::FraiseQLError::validation(format!(
                        "--mock-http has no entry matching {method} {url}"
                    ))
                }),
            None => Ok(HttpResponse {
                status:  200,
                headers: Vec::new(),
                body:    b"{}".to_vec(),
            }),
        }
    }

    async fn storage_get(&self, bucket: &str, key: &str) -> fraiseql_error::Result<Vec<u8>> {
        self.record("storage_get", format!("{bucket}/{key}"));
        Ok(Vec::new())
    }

    async fn storage_put(
        &self,
        bucket: &str,
        key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> fraiseql_error::Result<()> {
        self.record("storage_put", format!("{bucket}/{key}"));
        Ok(())
    }

    async fn send_email(
        &self,
        request: &fraiseql_functions::outbound::SendEmailRequest,
    ) -> fraiseql_error::Result<fraiseql_functions::outbound::SendEmailResponse> {
        self.record("send_email", format!("to {}", request.to));
        Ok(fraiseql_functions::outbound::SendEmailResponse {
            accepted:   true,
            message_id: Some("mock-send-id".to_string()),
        })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        self.record("auth_context", String::new());
        Ok(serde_json::json!({}))
    }

    fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
        self.record("env_var", name.to_string());
        Ok(None)
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event
    }

    fn log(&self, level: LogLevel, message: &str) {
        self.logs
            .lock()
            .expect("logs mutex poisoned")
            .push(format!("[{level:?}] {message}"));
    }

    fn idempotency_token(&self) -> Option<String> {
        self.idempotency_token.clone()
    }
}

/// Truncate a query/SQL string to a single readable summary line.
fn summarize(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.len() > 80 {
        format!("{}…", &one_line[..80])
    } else {
        one_line
    }
}

// ── Output ─────────────────────────────────────────────────────────────────

fn print_success(
    name: &str,
    result: &fraiseql_functions::FunctionResult,
    calls: &[HostOpCall],
    json: bool,
) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "function": name,
                "ok": true,
                "result": result.value,
                "duration_ms": result.duration.as_millis(),
                "logs": result.logs.iter().map(|l| l.message.clone()).collect::<Vec<_>>(),
                "host_ops": calls,
            })
        );
        return;
    }
    println!("✓ {name} ran in {} ms", result.duration.as_millis());
    if let Some(value) = &result.value {
        println!("result: {value}");
    }
    print_logs_and_ops(&result.logs, calls);
}

fn print_guest_error(
    name: &str,
    error: &fraiseql_error::FraiseQLError,
    calls: &[HostOpCall],
    json: bool,
) {
    if json {
        println!(
            "{}",
            serde_json::json!({ "function": name, "ok": false, "error": error.to_string(), "host_ops": calls })
        );
        return;
    }
    eprintln!("✗ {name} errored: {error}");
    print_logs_and_ops(&[], calls);
}

fn print_logs_and_ops(logs: &[fraiseql_functions::LogEntry], calls: &[HostOpCall]) {
    if !logs.is_empty() {
        println!("logs:");
        for entry in logs {
            println!("  [{:?}] {}", entry.level, entry.message);
        }
    }
    if calls.is_empty() {
        println!("host ops: (none)");
    } else {
        println!("host ops ({}):", calls.len());
        for call in calls {
            println!("  {} {}", call.op, call.summary);
        }
    }
}
