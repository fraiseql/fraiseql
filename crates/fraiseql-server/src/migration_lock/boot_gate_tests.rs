//! The boot-DDL gate: every DDL script the server runs goes through
//! [`super::run_migration`].
//!
//! A source scan of `fraiseql-server`'s non-test code, by position: each violation names
//! its file and line. Three shapes are refused:
//!
//! 1. a `sqlx::raw_sql` / `sqlx::query` whose argument is DDL text, anywhere but
//!    `migration_lock.rs`, or a schema constant or migration function (`*_SCHEMA_SQL`,
//!    `*_MIGRATION_SQL`, `*_sql()`) used anywhere but as a `run_migration` or `ProvisionStep::new`
//!    argument — which also catches a script split on `;` and run statement by statement. Macro
//!    bodies are opaque to the scan, so a schema constant inside `vec![…]` or `format!(…)` is not
//!    seen: pass slices;
//! 2. a call to another crate's DDL method that the server must not run unlocked
//!    (`init_dispatch_ledger`, `run_storage_migration`);
//! 3. an `.init()` call beyond the per-file allowance below, so a new crate store's `init` cannot
//!    slip into a boot path next to the allowed ones.
//!
//! A second scan holds the boot path itself to provisioning: the constructor, the serve
//! entry points, the start loop, the capability impls and the router run no DDL of their
//! own, locked or not. Every boot script is a step in the provisioning table, which runs
//! before anything is started or mounted.

#![allow(clippy::panic)] // Reason: test module — a scan that cannot read the source must fail loudly

use std::path::{Path, PathBuf};

use syn::visit::Visit;

/// `.init()` calls the server may make, per file, and why each is safe.
///
/// Each allowed `init` belongs to a server-owned type whose DDL runs through
/// `run_migration` inside it, so the rule-1 scan covers its body.
const INIT_ALLOWANCE: &[(&str, usize, &str)] = &[
    ("src/main.rs", 2, "the tracing subscriber, not DDL"),
    ("src/inbound/webhook.rs", 1, "PostgresInboundSpine::init"),
    ("src/server/functions_setup.rs", 2, "PgFunctionDlq::init, PgSendTracker::init"),
    ("src/server/capabilities/functions.rs", 1, "PgCronState::init"),
    (
        "src/server/capabilities/inbound_email.rs",
        2,
        "PostgresInboundSpine, PgSendTracker init",
    ),
];

/// The boot path outside provisioning: files, or directories ending `/`, that may run no
/// DDL at all. A store constructed here is constructed without its tables; the
/// provisioning table creates them.
const BOOT_PATH: &[&str] = &[
    // `build_drains`, which the constructor calls.
    "src/cdc_outbound/mod.rs",
    "src/server/builder.rs",
    "src/server/initialization.rs",
    "src/server/lifecycle.rs",
    "src/server/start.rs",
    "src/server/capabilities/",
    "src/server/routing/",
];

/// `init_…` functions the boot path calls that run no DDL.
///
/// `init_observer_runtime` builds the runtime; the table it checkpoints into is created
/// when the runtime starts.
const INIT_FN_EXEMPT: &[&str] = &["init_observer_runtime"];

/// Methods of other crates that execute DDL on their own connection.
const BANNED_METHODS: &[&str] = &["init_dispatch_ledger", "run_storage_migration"];

const DDL_KEYWORDS: &[&str] = &[
    "CREATE TABLE",
    "CREATE INDEX",
    "CREATE UNIQUE INDEX",
    "CREATE SCHEMA",
    "CREATE POLICY",
    "ALTER TABLE",
    "CREATE OR REPLACE FUNCTION",
    "CREATE TRIGGER",
];

fn is_test_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name == "tests.rs"
        || name.ends_with("_tests.rs")
        || path.components().any(|c| c.as_os_str() == "tests")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") && !is_test_file(&path) {
            out.push(path);
        }
    }
}

/// Whether an executed argument is DDL: literal text with a DDL keyword, or a path or
/// call naming a schema constant or migration function.
fn is_ddl_argument(expr: &syn::Expr) -> bool {
    let text = quote_free_text(expr);
    let upper = text.to_uppercase();
    DDL_KEYWORDS.iter().any(|k| upper.contains(k))
        || upper.contains("SCHEMA_SQL")
        || upper.contains("MIGRATION_SQL")
        || text.contains("_sql()")
}

/// The source text an expression was parsed from, without the token-stream dependency.
fn quote_free_text(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => s.value(),
        syn::Expr::Reference(r) => quote_free_text(&r.expr),
        syn::Expr::Path(p) => p
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        syn::Expr::Call(c) => format!("{}()", quote_free_text(&c.func)),
        syn::Expr::MethodCall(m) => format!("{}.{}()", quote_free_text(&m.receiver), m.method),
        _ => String::new(),
    }
}

struct Scan<'a> {
    file:              &'a str,
    in_lock:           bool,
    in_migration_args: usize,
    violations:        Vec<String>,
    inits:             Vec<usize>,
    migrations:        Vec<usize>,
}

impl Scan<'_> {
    fn schema_reference(&mut self, name: &str, line: usize) {
        let upper = name.to_uppercase();
        let names_ddl = upper.ends_with("SCHEMA_SQL") || upper.ends_with("MIGRATION_SQL");
        if names_ddl && !self.in_lock && self.in_migration_args == 0 {
            self.violations.push(format!(
                "{}:{line}: `{name}` used outside a run_migration argument",
                self.file
            ));
        }
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if let Some(last) = path.path.segments.last() {
            self.schema_reference(&last.ident.to_string(), last.ident.span().start().line);
        }
        syn::visit::visit_expr_path(self, path);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*call.func {
            let last = p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
            let line = p.path.segments[0].ident.span().start().line;
            let migration_fn = last.ends_with("_sql")
                && (last.contains("migration")
                    || p.path.segments.iter().any(|s| s.ident == "migrations"));
            if migration_fn {
                self.schema_reference(&format!("{last}() -> MIGRATION_SQL"), line);
            }
            if last == "init" {
                self.inits.push(line);
            }
            let sanctioned = last == "run_migration"
                || last == "run_migrations"
                || (last == "new" && p.path.segments.iter().any(|s| s.ident == "ProvisionStep"));
            // A free function named `init…` (`init_cursor_store`) wraps a store's DDL as
            // surely as its `init()` method does.
            let init_fn = last.starts_with("init_") && !INIT_FN_EXEMPT.contains(&last.as_str());
            if last == "run_migration" || last == "run_migrations" || init_fn {
                self.migrations.push(line);
            }
            if sanctioned {
                self.visit_expr(&call.func);
                self.in_migration_args += 1;
                for arg in &call.args {
                    self.visit_expr(arg);
                }
                self.in_migration_args -= 1;
                return;
            }
            let executes = last == "raw_sql" || (last == "query" && p.path.segments.len() > 1);
            if executes && !self.in_lock && call.args.first().is_some_and(is_ddl_argument) {
                let line = p.path.segments[0].ident.span().start().line;
                self.violations
                    .push(format!("{}:{line}: DDL executed outside run_migration", self.file));
            }
            if BANNED_METHODS.contains(&last.as_str()) {
                let line = p.path.segments[0].ident.span().start().line;
                self.violations
                    .push(format!("{}:{line}: `{last}` runs DDL unlocked", self.file));
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let name = call.method.to_string();
        let line = call.method.span().start().line;
        if BANNED_METHODS.contains(&name.as_str()) {
            self.violations
                .push(format!("{}:{line}: `{name}` runs DDL unlocked", self.file));
        }
        if name == "init" && call.args.is_empty() {
            self.inits.push(line);
        }
        syn::visit::visit_expr_method_call(self, call);
    }
}

#[test]
fn the_server_runs_boot_ddl_only_through_run_migration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    files.sort();

    let mut violations = Vec::new();
    for path in &files {
        let relative =
            path.strip_prefix(root).expect("under the crate").to_string_lossy().into_owned();
        let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {relative}: {e}"));
        let parsed = syn::parse_file(&src).unwrap_or_else(|e| panic!("parse {relative}: {e}"));
        let mut scan = Scan {
            file:              &relative,
            in_lock:           relative == "src/migration_lock.rs",
            in_migration_args: 0,
            violations:        Vec::new(),
            inits:             Vec::new(),
            migrations:        Vec::new(),
        };
        scan.visit_file(&parsed);
        violations.append(&mut scan.violations);
        let allowed =
            INIT_ALLOWANCE.iter().find(|(f, ..)| *f == relative).map_or(0, |(_, n, _)| *n);
        if scan.inits.len() > allowed {
            violations.push(format!(
                "{relative}: {} `.init()` calls at lines {:?}, {allowed} allowed — a store's \
                 `init()` runs its DDL without the migration lock; apply its published DDL \
                 through run_migration instead",
                scan.inits.len(),
                scan.inits
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "boot DDL that bypasses the migration lock ({}):\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

fn on_boot_path(relative: &str) -> bool {
    BOOT_PATH.iter().any(|entry| {
        if entry.ends_with('/') {
            relative.starts_with(entry)
        } else {
            relative == *entry
        }
    })
}

#[test]
fn the_boot_path_runs_no_ddl_outside_provisioning() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    files.sort();

    let mut violations = Vec::new();
    for path in &files {
        let relative =
            path.strip_prefix(root).expect("under the crate").to_string_lossy().into_owned();
        if !on_boot_path(&relative) {
            continue;
        }
        let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {relative}: {e}"));
        let parsed = syn::parse_file(&src).unwrap_or_else(|e| panic!("parse {relative}: {e}"));
        let mut scan = Scan {
            file:              &relative,
            in_lock:           false,
            in_migration_args: 0,
            violations:        Vec::new(),
            inits:             Vec::new(),
            migrations:        Vec::new(),
        };
        scan.visit_file(&parsed);
        for line in scan.migrations {
            violations.push(format!("{relative}:{line}: run_migration"));
        }
        for line in scan.inits {
            violations.push(format!("{relative}:{line}: `.init()`"));
        }
    }
    assert!(
        violations.is_empty(),
        "boot DDL outside the provisioning table ({}); make it a `Capability::provision` step \
         or a core step in `provision_steps`:\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

/// Every boot-path entry names a real file or directory, so a moved file does not leave the
/// rule covering nothing.
#[test]
fn every_boot_path_entry_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for entry in BOOT_PATH {
        assert!(root.join(entry).exists(), "BOOT_PATH names `{entry}`, which does not exist");
    }
}

/// Every allowance names a file that exists, so a moved file does not leave a stale
/// allowance silently covering nothing.
#[test]
fn every_init_allowance_names_a_real_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (file, ..) in INIT_ALLOWANCE {
        assert!(root.join(file).is_file(), "INIT_ALLOWANCE names `{file}`, which does not exist");
    }
}
