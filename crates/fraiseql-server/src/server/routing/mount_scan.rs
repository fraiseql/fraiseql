//! The mounted surface, read from the source.
//!
//! axum cannot list a `Router`'s routes, and nothing in the server records the paths it
//! mounts, so a test that must cover "every mounted transport" has nothing to iterate.
//! This scan supplies it: every function in a crate that can build an axum router is
//! parsed, and each call that adds routes to one (`.route`, `.nest`, `.merge`, …) is
//! keyed by the function it sits in. The admission matrix then requires every such
//! function to be classified as a [`Transport`](super::admission_matrix_tests::Transport),
//! so a new mount function fails until someone answers which refusals it owes.
//!
//! The scan also checks layer order. axum's `.layer` and `.route_layer` wrap only the
//! routes already on the router, so a route added to a router *after* its authentication
//! layer is served without it, while every other route on that transport still refuses.
//! A matrix cell that requests one route per transport cannot see that; this check can.
//!
//! The source is read through `CARGO_MANIFEST_DIR`, not `git`, so the scan runs the same
//! in a container that has the tree but no repository.

#![allow(clippy::panic)] // Reason: test module — a scan that cannot read the source must fail loudly

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use syn::visit::Visit;

/// Methods that add routes to an axum `Router`.
const MOUNT_METHODS: &[&str] = &[
    "route",
    "route_service",
    "nest",
    "nest_service",
    "merge",
    "fallback",
    "fallback_service",
];

/// Methods that wrap the routes already on a router.
const LAYER_METHODS: &[&str] = &["layer", "route_layer"];

/// A call to one of these guards everything it returns.
const GUARD_FNS: &[&str] = &["attach_auth"];

/// Words that mark a `.layer`/`.route_layer` argument as an admission guard: the layer
/// authenticates, authorises or binds a tenant, rather than shaping the response.
const GUARD_WORDS: &[&str] = &["auth", "tenant", "principal", "bearer", "require", "admin_"];

/// One function that adds routes to a router.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct MountFn {
    /// Path of the file, relative to `crates/`.
    pub(super) file:   String,
    /// `Type::method` for an associated function, the bare name otherwise.
    pub(super) name:   String,
    /// Whether it adds a route itself (`.route`, `.nest`, …) rather than only merging
    /// routers built elsewhere.
    pub(super) direct: bool,
}

impl MountFn {
    /// `file::name`, the key the classification table uses.
    pub(super) fn key(&self) -> String {
        format!("{}::{}", self.file, self.name)
    }
}

/// A route added to a router after that router's guard.
#[derive(Debug)]
pub(super) struct Unguarded {
    pub(super) mount:  MountFn,
    pub(super) method: String,
    pub(super) line:   usize,
}

/// What the scan found.
#[derive(Debug, Default)]
pub(super) struct Surface {
    pub(super) mounts:    BTreeSet<MountFn>,
    pub(super) unguarded: Vec<Unguarded>,
}

/// The workspace's `crates/` directory.
fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("CARGO_MANIFEST_DIR has no parent"))
        .to_path_buf()
}

/// Every workspace crate that can build an axum router: one that takes `axum` as a
/// normal (not dev-only) dependency. Derived, so a crate that starts serving routes is
/// scanned without anyone extending a list.
pub(super) fn router_crates() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let dir = crates_dir();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.unwrap_or_else(|e| panic!("entry: {e}")).path();
        let manifest = path.join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let parsed: toml::Table =
            toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", manifest.display()));
        let names_axum = |section: &str| {
            parsed
                .get(section)
                .and_then(toml::Value::as_table)
                .is_some_and(|t| t.contains_key("axum"))
        };
        if names_axum("dependencies") {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_else(|| panic!("crate dir name: {}", path.display()));
            out.insert(name.to_owned());
        }
    }
    out
}

/// Every workspace crate with a non-test source file that adds a route to a router,
/// found by text rather than by its manifest: the check on [`router_crates`], which a
/// crate reaching axum through a re-export would escape.
pub(super) fn crates_that_mount_routes() -> BTreeSet<String> {
    let dir = crates_dir();
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.unwrap_or_else(|e| panic!("entry: {e}")).path();
        let mut files = Vec::new();
        collect_rs(&path.join("src"), &mut files);
        let mounts = files.iter().any(|f| {
            std::fs::read_to_string(f)
                .is_ok_and(|text| text.contains("Router") && text.contains(".route("))
        });
        if mounts {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                out.insert(name.to_owned());
            }
        }
    }
    out
}

/// Scan every router crate's `src/`.
pub(super) fn scan() -> Surface {
    let crates = router_crates();
    let dir = crates_dir();
    let mut surface = Surface::default();
    let mut files = Vec::new();
    for krate in &crates {
        collect_rs(&dir.join(krate).join("src"), &mut files);
    }
    // Pass 1: the functions whose result is guarded, so that a route added to the router
    // they return is caught in pass 2 even though the guard is in another function.
    let mut guarding: BTreeSet<String> = GUARD_FNS.iter().map(|s| (*s).to_owned()).collect();
    let parsed: Vec<(String, syn::File)> = files
        .iter()
        .map(|path| {
            let src = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let file =
                syn::parse_file(&src).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            let rel = path
                .strip_prefix(&dir)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .to_string_lossy()
                .into_owned();
            (rel, file)
        })
        .collect();
    loop {
        let before = guarding.len();
        for (_, file) in &parsed {
            let mut finder = GuardingFns {
                guarding: &guarding,
                found:    BTreeSet::new(),
                owner:    None,
            };
            finder.visit_file(file);
            guarding.extend(finder.found);
        }
        if guarding.len() == before {
            break;
        }
    }
    for (rel, file) in &parsed {
        let mut visitor = FileVisitor {
            file:     rel.clone(),
            guarding: &guarding,
            owner:    None,
            surface:  &mut surface,
        };
        visitor.visit_file(file);
    }
    surface
}

/// Every `.rs` file under `dir`, skipping test files: their routers are fixtures, not
/// the mounted surface.
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap_or_else(|e| panic!("entry: {e}")).path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if path.is_dir() {
            if name != "tests" && name != "testing" {
                collect_rs(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs")
            && name != "tests.rs"
            && !name.ends_with("_tests.rs")
            && !name.ends_with("_test.rs")
        {
            out.push(path);
        }
    }
}

/// Whether an item carries `#[cfg(test)]`.
fn is_test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg") && a.parse_args::<syn::Ident>().is_ok_and(|i| i == "test")
    })
}

/// The function a call names: the last segment of a path call, or the method name.
fn called_name(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Call(call) => match &*call.func {
            syn::Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
            _ => None,
        },
        syn::Expr::MethodCall(m) => Some(m.method.to_string()),
        _ => None,
    }
}

/// Whether the router `expr` evaluates to is guarded: somewhere along its receiver
/// chain there is a guarding layer, or the chain starts from a call to a guarding
/// function. Arguments are not searched: in `app.merge(inner.route_layer(auth))` the
/// guard wraps `inner`, not `app`.
fn guarded_spine(expr: &syn::Expr, guarding: &BTreeSet<String>) -> bool {
    match expr {
        syn::Expr::MethodCall(m) => {
            let method = m.method.to_string();
            (LAYER_METHODS.contains(&method.as_str()) && is_guard_layer(m))
                || guarding.contains(&method)
                || guarded_spine(&m.receiver, guarding)
        },
        syn::Expr::Call(_) => called_name(expr).is_some_and(|n| guarding.contains(&n)),
        syn::Expr::Paren(p) => guarded_spine(&p.expr, guarding),
        syn::Expr::Try(t) => guarded_spine(&t.expr, guarding),
        _ => false,
    }
}

/// Whether a `.layer(..)`/`.route_layer(..)` call's argument names an admission concern.
fn is_guard_layer(call: &syn::ExprMethodCall) -> bool {
    let args = call.args.iter().map(|a| quote_tokens(a).to_lowercase()).collect::<String>();
    GUARD_WORDS.iter().any(|w| args.contains(w))
}

fn quote_tokens(expr: &syn::Expr) -> String {
    use quote::ToTokens;
    expr.to_token_stream().to_string()
}

/// The identifier a method chain starts from: `app` in `app.route(..).layer(..)`.
fn chain_root(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::MethodCall(m) => chain_root(&m.receiver),
        syn::Expr::Path(p) => p.path.get_ident().map(ToString::to_string),
        syn::Expr::Paren(p) => chain_root(&p.expr),
        _ => None,
    }
}

/// Pass 1: functions whose returned router is guarded.
struct GuardingFns<'a> {
    guarding: &'a BTreeSet<String>,
    found:    BTreeSet<String>,
    owner:    Option<String>,
}

impl<'ast> Visit<'ast> for GuardingFns<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_test_only(&m.attrs) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !is_test_only(&f.attrs) {
            self.check(&f.sig, &f.block);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_test_only(&f.attrs) {
            self.check(&f.sig, &f.block);
        }
    }
}

impl GuardingFns<'_> {
    fn check(&mut self, sig: &syn::Signature, block: &syn::Block) {
        let _ = &self.owner;
        let returns_router = match &sig.output {
            syn::ReturnType::Type(_, ty) => quote_type(ty).contains("Router"),
            syn::ReturnType::Default => false,
        };
        if !returns_router {
            return;
        }
        if let Some(syn::Stmt::Expr(tail, None)) = block.stmts.last() {
            if guarded_spine(tail, self.guarding) {
                self.found.insert(sig.ident.to_string());
            }
        }
    }
}

fn quote_type(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

/// Pass 2: mount functions and routes added after a guard.
struct FileVisitor<'a, 's> {
    file:     String,
    guarding: &'a BTreeSet<String>,
    owner:    Option<String>,
    surface:  &'s mut Surface,
}

impl<'ast> Visit<'ast> for FileVisitor<'_, '_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_test_only(&m.attrs) {
            syn::visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_test_only(&i.attrs) {
            return;
        }
        let previous = self.owner.take();
        self.owner = Some(quote_type(&i.self_ty).replace(' ', ""));
        syn::visit::visit_item_impl(self, i);
        self.owner = previous;
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if !is_test_only(&f.attrs) {
            self.function(f.sig.ident.to_string(), &f.sig, &f.block);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !is_test_only(&f.attrs) {
            let name = match &self.owner {
                Some(owner) => format!("{owner}::{}", f.sig.ident),
                None => f.sig.ident.to_string(),
            };
            self.function(name, &f.sig, &f.block);
        }
    }
}

impl FileVisitor<'_, '_> {
    /// A function is a mount when it adds routes to a router, or when it hands a router
    /// to its caller (a capability's `routes`, a transport's `*_router`): either way its
    /// routes reach the application through it.
    fn function(&mut self, name: String, sig: &syn::Signature, block: &syn::Block) {
        let mut mount = MountFn {
            file: self.file.clone(),
            name,
            direct: false,
        };
        let mut body = Body {
            guarding:  self.guarding,
            guarded:   BTreeSet::new(),
            handed:    router_parameters(sig),
            mounts:    false,
            direct:    false,
            unguarded: Vec::new(),
        };
        body.visit_block(block);
        mount.direct = body.direct;
        let returns_router = match &sig.output {
            syn::ReturnType::Type(_, ty) => quote_type(ty).contains("Router"),
            syn::ReturnType::Default => false,
        };
        if body.mounts || returns_router {
            self.surface.mounts.insert(mount.clone());
        }
        for (method, line) in body.unguarded {
            self.surface.unguarded.push(Unguarded {
                mount: mount.clone(),
                method,
                line,
            });
        }
    }
}

/// The names of `sig`'s parameters whose type is a `Router`.
fn router_parameters(sig: &syn::Signature) -> BTreeSet<String> {
    sig.inputs
        .iter()
        .filter_map(|input| match input {
            syn::FnArg::Typed(typed) if quote_type(&typed.ty).contains("Router") => {
                match &*typed.pat {
                    syn::Pat::Ident(id) => Some(id.ident.to_string()),
                    _ => None,
                }
            },
            _ => None,
        })
        .collect()
}

/// One function body, in source order.
struct Body<'a> {
    guarding:  &'a BTreeSet<String>,
    /// Locals holding a guarded router.
    guarded:   BTreeSet<String>,
    /// The function's own `Router` parameters: a router its caller built.
    handed:    BTreeSet<String>,
    mounts:    bool,
    direct:    bool,
    unguarded: Vec<(String, usize)>,
}

impl<'ast> Visit<'ast> for Body<'_> {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        syn::visit::visit_local(self, local);
        if let (syn::Pat::Ident(id), Some(init)) = (&local.pat, &local.init) {
            self.assign(&id.ident.to_string(), &init.expr);
        }
    }

    fn visit_expr_assign(&mut self, a: &'ast syn::ExprAssign) {
        syn::visit::visit_expr_assign(self, a);
        if let syn::Expr::Path(p) = &*a.left {
            if let Some(id) = p.path.get_ident() {
                self.assign(&id.to_string(), &a.right);
            }
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        syn::visit::visit_expr_method_call(self, m);
        let method = m.method.to_string();
        if !MOUNT_METHODS.contains(&method.as_str()) {
            return;
        }
        if method == "merge" && !self.looks_like_router_merge(m) {
            return;
        }
        self.mounts = true;
        self.direct |= method != "merge" && !self.nests_a_handed_router(&method, m);
        let on_guarded_local = chain_root(&m.receiver).is_some_and(|r| self.guarded.contains(&r));
        if on_guarded_local || guarded_spine(&m.receiver, self.guarding) {
            self.unguarded.push((method, m.method.span().start().line));
        }
    }
}

impl Body<'_> {
    /// Track which locals hold a guarded router: one assigned from a guarded
    /// expression, or from a chain rooted at a guarded local.
    fn assign(&mut self, name: &str, value: &syn::Expr) {
        let guarded = guarded_spine(value, self.guarding)
            || chain_root(value).is_some_and(|r| self.guarded.contains(&r));
        if guarded {
            self.guarded.insert(name.to_owned());
        } else {
            self.guarded.remove(name);
        }
    }

    /// Whether `m` nests, unchanged, a router the function was handed: the routes are its
    /// caller's, so nesting them adds none of its own, as merging does not.
    fn nests_a_handed_router(&self, method: &str, m: &syn::ExprMethodCall) -> bool {
        method == "nest"
            && m.args.last().is_some_and(|router| match router {
                syn::Expr::Path(p) => {
                    p.path.get_ident().is_some_and(|id| self.handed.contains(&id.to_string()))
                },
                _ => false,
            })
    }

    /// `.merge` names many unrelated operations (JSON, maps, configs); count it only
    /// when the receiver or the argument is plainly a router.
    fn looks_like_router_merge(&self, m: &syn::ExprMethodCall) -> bool {
        let text = format!(
            "{} {}",
            quote_tokens(&m.receiver),
            m.args.iter().map(quote_tokens).collect::<String>()
        );
        ["Router", "router", "routes", "app"].iter().any(|w| text.contains(w))
    }
}
