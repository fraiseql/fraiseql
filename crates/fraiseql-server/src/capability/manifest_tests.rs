//! Manifest tests: the capability registry checked against the structs it describes.
//!
//! A hand list of feature-gated sections is only as good as the last time someone
//! remembered to extend it. These tests read the structs themselves, so a field added
//! behind a `#[cfg(feature = …)]`, or a compiled-schema section nobody classified, fails
//! here instead of slipping past a list nobody re-read. They read the source through
//! `CARGO_MANIFEST_DIR`, not `git`, so they run the same in a container that has the
//! tree but no repository.

#![allow(clippy::panic)] // Reason: test module — a scan that cannot read the source must fail loudly

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use super::REGISTRY;

/// A named field of a scanned struct.
struct Field {
    /// The wire key (the field name: renames are refused by the scanner).
    name:    String,
    /// The Cargo feature its `#[cfg(feature = "…")]` names, if any.
    feature: Option<String>,
    /// Whether `#[serde(skip)]` keeps it off the wire.
    skipped: bool,
}

/// Every named field of `struct_name` in `file`.
///
/// A `cfg` this scanner cannot read as a single `feature = "…"`, or a serde attribute
/// that changes the wire key, fails the test rather than being skipped: the scan exists
/// to leave nothing out, and a field it silently passed over would be exactly the miss
/// it is meant to catch.
fn scan_struct(file: &Path, struct_name: &str) -> Vec<Field> {
    let src =
        std::fs::read_to_string(file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
    let parsed = syn::parse_file(&src).unwrap_or_else(|e| panic!("parse {}: {e}", file.display()));
    let item = parsed
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Struct(s) if s.ident == struct_name => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no `struct {struct_name}` in {}", file.display()));
    for attr in &item.attrs {
        serde_keys(attr, struct_name);
    }
    let syn::Fields::Named(fields) = &item.fields else {
        panic!("`{struct_name}` has no named fields");
    };
    fields
        .named
        .iter()
        .map(|field| {
            let name = field.ident.as_ref().expect("named field").to_string();
            let at = format!("{struct_name}.{name}");
            let mut feature = None;
            let mut skipped = false;
            for attr in &field.attrs {
                if attr.path().is_ident("cfg") {
                    feature = Some(cfg_feature(attr, &at));
                }
                skipped |= serde_keys(attr, &at).iter().any(|k| k == "skip");
            }
            Field {
                name,
                feature,
                skipped,
            }
        })
        .collect()
}

/// The feature a `#[cfg(feature = "…")]` names; anything else fails the test.
fn cfg_feature(attr: &syn::Attribute, at: &str) -> String {
    let meta: syn::MetaNameValue = attr
        .parse_args()
        .unwrap_or_else(|e| panic!("`{at}`: cfg is not `feature = \"…\"`: {e}"));
    assert!(meta.path.is_ident("feature"), "`{at}`: cfg is not `feature = \"…\"`");
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(s),
        ..
    }) = meta.value
    else {
        panic!("`{at}`: cfg feature is not a string literal");
    };
    s.value()
}

/// The top-level names inside a `#[serde(…)]` attribute (empty for any other
/// attribute). One that changes the wire key fails the test.
fn serde_keys(attr: &syn::Attribute, at: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if !attr.path().is_ident("serde") {
        return keys;
    }
    attr.parse_nested_meta(|m| {
        let key = m.path.get_ident().map(ToString::to_string).unwrap_or_default();
        assert!(
            !matches!(key.as_str(), "rename" | "rename_all" | "flatten" | "alias"),
            "`{at}`: serde `{key}` changes the wire key; teach this scanner to read it"
        );
        if m.input.peek(syn::Token![=]) {
            m.value()?.parse::<syn::Expr>()?;
        }
        keys.push(key);
        Ok(())
    })
    .unwrap_or_else(|e| panic!("`{at}`: unreadable serde attribute: {e}"));
    keys
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)
}

/// Compiled-schema sections every build serves, and why no feature gates them.
///
/// "It needs no gate" is a claim about where the code runs, so the reason is recorded
/// for a later reader to re-check. A section in neither this list nor a registration
/// fails [`every_compiled_schema_section_is_classified`]: whether a section's consumers
/// are all feature-gated is a question about call graphs and `cfg` arms, and answering
/// it wrongly in the permissive direction is the #1326 defect (a lean build loads the
/// section, validates it and silently drops it).
const SERVED_BY_EVERY_BUILD: &[(&str, &str)] = &[
    ("types", "the GraphQL type system, the executor's core"),
    ("enums", "type system"),
    ("input_types", "type system"),
    ("interfaces", "type system"),
    ("unions", "type system"),
    ("queries", "read dispatch"),
    ("mutations", "write dispatch"),
    ("subscriptions", "the subscription manager is built in every server"),
    ("directives", "type system"),
    ("fact_tables", "aggregate/window dispatch in fraiseql-core"),
    (
        "observers",
        "observer DEFINITIONS are matched by the change-log reader in core; the `observers` \
         feature gates the action transports, which `observers_config` covers",
    ),
    ("subscribable", "read by the subscription manager"),
    ("operation_cost_weights", "the #379 cost estimator in fraiseql-core"),
    ("security", "field filters, RLS, role and actor gates all live in fraiseql-core"),
    (
        "locale",
        "request-locale resolution, the read session setting, collation and localized-field \
         projection live in fraiseql-core and every transport (#1512, #1513)",
    ),
    (
        "auth",
        "`auth` is a default feature; a build without it mounts no auth surface at all",
    ),
    ("subscriptions_config", "read by the subscription manager"),
    ("validation_config", "the #379 executor gate in fraiseql-core"),
    ("debug_config", "read by AppState in every build"),
    ("changelog", "the Change-Spine outbox write lives in fraiseql-core"),
    ("session_variables", "resolved in fraiseql-core on every request"),
    ("hierarchies_config", "hierarchy resolution in fraiseql-core"),
    ("naming_convention", "casing resolution in fraiseql-core"),
    ("naming_acronyms", "casing resolution in fraiseql-core"),
    ("fraiseql_version", "the #1304 build-identity stamp, checked in every build"),
    ("schema_sdl", "served by introspection"),
    (
        "storage",
        "refused outright (#1008): configured in the server config file, not here",
    ),
    ("realtime", "warned and ignored (#605): the subsystem was removed"),
];

/// Keys `load_extended` reads from `schema.compiled.json` that are not
/// `CompiledSchema` fields.
const LOADER_ONLY_KEYS: &[&str] = &["functions", "storage", "realtime"];

/// Every `ServerConfig` key that exists only under a Cargo feature is a registered
/// runtime key, under that feature, and nothing else is.
///
/// A key missing from the registry still refuses to parse in a lean build
/// (`deny_unknown_fields`), but with serde's bare "unknown field" and no word about the
/// build feature that would accept it.
#[test]
fn every_feature_gated_server_config_key_is_registered() {
    let scanned: BTreeSet<(String, String)> =
        scan_struct(&repo_path("crates/fraiseql-server/src/server_config/mod.rs"), "ServerConfig")
            .into_iter()
            .filter_map(|f| f.feature.map(|feature| (f.name, feature)))
            .collect();
    let registered: BTreeSet<(String, String)> = super::feature_gated_keys()
        .iter()
        .map(|(key, feature, _)| ((*key).to_string(), (*feature).to_string()))
        .collect();

    let missing: Vec<_> = scanned.difference(&registered).collect();
    let stale: Vec<_> = registered.difference(&scanned).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "the capability registry disagrees with ServerConfig.\n  gated in ServerConfig but \
         not registered: {missing:?}\n  registered but not a gated ServerConfig field: {stale:?}"
    );
}

/// Every wire-visible compiled-schema section is either registered to exactly one
/// capability or listed in [`SERVED_BY_EVERY_BUILD`], never both, and neither list
/// names a section that no longer exists.
#[test]
fn every_compiled_schema_section_is_classified() {
    let discovered: BTreeSet<String> = scan_struct(
        &repo_path("crates/fraiseql-core/src/schema/compiled/schema.rs"),
        "CompiledSchema",
    )
    .into_iter()
    .filter(|f| !f.skipped)
    .map(|f| f.name)
    .chain(LOADER_ONLY_KEYS.iter().map(ToString::to_string))
    .collect();

    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for r in REGISTRY {
        for section in r.meta.compiled_sections {
            owners.entry(section.key).or_default().push(r.meta.name);
        }
    }
    for (key, reason) in SERVED_BY_EVERY_BUILD {
        owners.entry(key).or_default().push(reason);
    }

    let unclassified: Vec<_> =
        discovered.iter().filter(|k| !owners.contains_key(k.as_str())).collect();
    let stale: Vec<_> = owners.keys().filter(|k| !discovered.contains(**k)).collect();
    let ambiguous: Vec<_> = owners.iter().filter(|(_, o)| o.len() > 1).collect();
    assert!(
        unclassified.is_empty() && stale.is_empty() && ambiguous.is_empty(),
        "compiled-schema sections and the capability registry disagree.\n  unclassified \
         (register it, or add it to SERVED_BY_EVERY_BUILD with the reason): \
         {unclassified:?}\n  classified but not a compiled-schema section: {stale:?}\n  \
         classified more than once: {ambiguous:?}"
    );
}

/// A runtime key belongs to one capability: two registrations claiming it would give
/// a parse error two contradictory build-feature hints.
#[test]
fn every_runtime_key_has_one_registration() {
    let mut seen = BTreeMap::new();
    for r in REGISTRY {
        for key in r.meta.runtime_keys {
            if let Some(other) = seen.insert(*key, r.meta.name) {
                panic!("`{key}` is registered by both `{other}` and `{}`", r.meta.name);
            }
        }
    }
}

/// Capability names are unique: `fraiseql capabilities` and every consumer key on them.
#[test]
fn every_capability_name_is_unique() {
    let mut seen = BTreeSet::new();
    for r in REGISTRY {
        assert!(seen.insert(r.meta.name), "`{}` is registered twice", r.meta.name);
    }
}

/// Every registration names a feature `fraiseql-server/Cargo.toml` declares, or its
/// remedy tells an operator to pass a flag that does nothing.
#[test]
fn every_registration_names_a_real_feature() {
    let manifest: toml::Table = toml::from_str(include_str!("../../Cargo.toml"))
        .expect("fraiseql-server/Cargo.toml parses");
    let features = manifest["features"].as_table().expect("[features] table");
    for r in REGISTRY {
        assert!(
            features.contains_key(r.meta.feature),
            "`{}` names feature `{}`, which fraiseql-server/Cargo.toml does not declare",
            r.meta.name,
            r.meta.feature
        );
    }
}

/// Every documentation page a registration names exists.
#[test]
fn every_registered_docs_page_exists() {
    for r in REGISTRY {
        if let Some(docs) = r.meta.docs {
            assert!(
                repo_path(docs).is_file(),
                "`{}` names docs page `{docs}`, which does not exist",
                r.meta.name
            );
        }
    }
}

/// Stable means a published artifact compiles it in; Preview means none does.
#[test]
fn maturity_agrees_with_the_release_artifact() {
    use fraiseql_core::capability::{Maturity, Release};
    for r in REGISTRY {
        let source_only = r.meta.release == Release::Source;
        assert_eq!(
            r.meta.maturity == Maturity::Preview,
            source_only,
            "`{}` is {:?} but released as {:?}",
            r.meta.name,
            r.meta.maturity,
            r.meta.release
        );
    }
}

/// The two lists derived from the registry carry each registration's own feature,
/// `compiled_in`, remedy and activity, for every key it registers and no other.
#[test]
fn derived_lists_carry_each_registration_unchanged() {
    let mut expected_keys = Vec::new();
    let mut expected_sections = Vec::new();
    for r in REGISTRY {
        for key in r.meta.runtime_keys {
            expected_keys.push((*key, r.meta.feature, r.compiled_in));
        }
        for s in r.meta.compiled_sections {
            expected_sections.push(crate::schema::loader::GatedSection {
                name:        s.key,
                feature:     r.meta.feature,
                compiled_in: r.compiled_in,
                remedy:      r.meta.remedy(),
                activity:    s.activity,
            });
        }
    }
    assert_eq!(super::feature_gated_keys(), expected_keys);
    assert_eq!(crate::schema::loader::gated_sections(), expected_sections);
}

/// `ServerConfig::from_file` hands the registry's keys to the parse-error hint. Only a
/// build without the feature can see the hint, so this arm runs in a lean build
/// (`cargo test -p fraiseql-server --lib capability::`), not the all-features leg.
#[cfg(not(feature = "arrow"))]
#[test]
fn a_config_naming_a_compiled_out_key_gets_the_build_feature_hint() {
    let file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(file.path(), "flight_views = []\n").expect("write config");
    let err = crate::ServerConfig::from_file(file.path()).expect_err("unknown in this build");
    assert!(err.contains("`arrow` feature"), "the hint must name the feature: {err}");
}

/// A minimal declaration of `key` that its registration's activity rule reads as
/// asking for something to run.
fn active_declaration(activity: fraiseql_core::capability::SectionActivity) -> serde_json::Value {
    use fraiseql_core::capability::SectionActivity;
    match activity {
        SectionActivity::EnabledFlag => serde_json::json!({ "enabled": true }),
        SectionActivity::AnyEntryEnabled => serde_json::json!([
            { "name": "s", "schedule": "*/5 * * * *", "function": "f" }
        ]),
        SectionActivity::NonEmptyDefinitions => serde_json::json!({ "definitions": [{}] }),
        _ => panic!("no fixture for {activity:?}"),
    }
}

/// A capability's compiled section is read once, by the subsystem it starts at boot, so
/// a hot reload changing it must be refused rather than half-applied. Every registered
/// section that is a `CompiledSchema` field is classified boot-frozen by the reload gate.
#[test]
fn every_registered_compiled_section_is_boot_frozen() {
    use fraiseql_core::schema::CompiledSchema;
    let empty = serde_json::to_value(CompiledSchema::default()).expect("serialize");
    for r in REGISTRY {
        for section in r.meta.compiled_sections {
            if LOADER_ONLY_KEYS.contains(&section.key) {
                continue; // not a `CompiledSchema` field: the reload gate never sees it
            }
            let mut next = empty.clone();
            next[section.key] = active_declaration(section.activity);
            let next: CompiledSchema = serde_json::from_value(next)
                .unwrap_or_else(|e| panic!("`{}` fixture does not parse: {e}", section.key));
            let drift = crate::routes::graphql::reload_gate::boot_frozen_drift(
                &CompiledSchema::default(),
                &next,
            );
            assert!(
                drift.contains(&section.key),
                "`{}` ({}) changed across a reload, but the reload gate let it through: \
                 {drift:?}",
                section.key,
                r.meta.name
            );
        }
    }
}

/// Every registered compiled section can be authored: the multi-file loader and merger
/// carry exactly the sections `seam.rs` lists, so one missing there would be dropped on
/// the TOML workflow paths before the capability ever saw it.
#[test]
fn every_registered_compiled_section_is_authorable() {
    use fraiseql_cli::schema::seam::{AUTHORABLE_ARRAY_SECTIONS, AUTHORABLE_SINGLETON_SECTIONS};
    for r in REGISTRY {
        for section in r.meta.compiled_sections {
            assert!(
                AUTHORABLE_ARRAY_SECTIONS.contains(&section.key)
                    || AUTHORABLE_SINGLETON_SECTIONS.contains(&section.key),
                "`{}` ({}) is not an authorable section in fraiseql-cli's seam.rs",
                section.key,
                r.meta.name
            );
        }
    }
}

/// The `fraiseql-server` features a build compiles, given the features it is asked for:
/// the closure through the manifest's `[features]` table, plus the default set.
fn server_feature_closure(requested: &[String]) -> BTreeSet<String> {
    let manifest: toml::Table = toml::from_str(include_str!("../../Cargo.toml"))
        .expect("fraiseql-server/Cargo.toml parses");
    let table = manifest["features"].as_table().expect("[features] table");
    let mut closure = BTreeSet::new();
    let mut pending: Vec<String> = requested.to_vec();
    pending.push("default".to_string());
    while let Some(feature) = pending.pop() {
        if !closure.insert(feature.clone()) {
            continue;
        }
        let entries = table
            .get(&feature)
            .and_then(toml::Value::as_array)
            .unwrap_or_else(|| panic!("`{feature}` is not a fraiseql-server feature"));
        pending.extend(
            entries
                .iter()
                .filter_map(toml::Value::as_str)
                .filter(|e| !e.starts_with("dep:") && !e.contains('/'))
                .map(ToString::to_string),
        );
    }
    closure
}

/// The `fraiseql-server` features an umbrella (`fraiseql`) feature forwards, followed
/// through the umbrella's own feature table.
fn umbrella_forwards(feature: &str) -> Vec<String> {
    let manifest: toml::Table = toml::from_str(
        &std::fs::read_to_string(repo_path("crates/fraiseql/Cargo.toml")).expect("read"),
    )
    .expect("crates/fraiseql/Cargo.toml parses");
    let table = manifest["features"].as_table().expect("[features] table");
    let mut seen = BTreeSet::new();
    let mut forwarded = Vec::new();
    let mut pending = vec![feature.to_string()];
    while let Some(f) = pending.pop() {
        if !seen.insert(f.clone()) {
            continue;
        }
        for entry in table[&f]
            .as_array()
            .expect("feature list")
            .iter()
            .filter_map(toml::Value::as_str)
        {
            if let Some(server) = entry.strip_prefix("fraiseql-server?/") {
                forwarded.push(server.to_string());
            } else if !entry.starts_with("dep:") && !entry.contains('/') {
                pending.push(entry.to_string());
            }
        }
    }
    forwarded
}

/// The `CARGO_FEATURES` build argument of an image variant in `.dagger/image.go`, the
/// table `check-image-parity.py` holds `docker-build.yml` to.
fn image_features(variant: &str) -> Vec<String> {
    let src = std::fs::read_to_string(repo_path(".dagger/image.go")).expect("read image.go");
    let line = src
        .lines()
        .find(|l| l.contains(&format!("name: \"{variant}\"")))
        .unwrap_or_else(|| panic!("no `{variant}` row in .dagger/image.go"));
    let args = line
        .split("buildArgs: \"")
        .nth(1)
        .expect("buildArgs")
        .split('"')
        .next()
        .expect("value");
    args.strip_prefix("CARGO_FEATURES=")
        .map(|list| list.split(',').map(ToString::to_string).collect())
        .unwrap_or_default()
}

/// Each registration's `release` is the smallest published artifact that compiles it,
/// measured against the recipes, in both directions: no artifact below it has it, and
/// every artifact from it up does. The default image builds the default features, `-full`
/// is the `fraiseql-server-full` image row, and `-platform` is the umbrella's
/// `release-full` bundle, which `check-artifact-features.py` holds the platform image and
/// the release tarball builds to.
#[test]
fn every_registration_ships_in_the_artifacts_its_release_names() {
    use fraiseql_core::capability::Release;
    let default = server_feature_closure(&image_features("fraiseql-server"));
    let full = server_feature_closure(&image_features("fraiseql-server-full"));
    let platform = server_feature_closure(&umbrella_forwards("release-full"));
    for r in REGISTRY {
        let shipped = [&default, &full, &platform].map(|set| set.contains(r.meta.feature));
        let expected = match r.meta.release {
            Release::Default => [true, true, true],
            Release::Full => [false, true, true],
            Release::Platform => [false, false, true],
            Release::Source => [false, false, false],
            _ => panic!("no artifact mapping for {:?}", r.meta.release),
        };
        assert_eq!(
            shipped, expected,
            "`{}` ({}) is registered as {:?}, but [default, -full, -platform] compile it as \
             {shipped:?}",
            r.meta.name, r.meta.feature, r.meta.release
        );
    }
}

/// A Stable capability above the default set is reachable from the `fraiseql` facade
/// crate: some umbrella feature forwards its server feature. Without it, an embedder
/// depending on `fraiseql` cannot turn the capability on.
#[test]
fn every_stable_registration_is_forwarded_by_the_facade() {
    use fraiseql_core::capability::{Maturity, Release};
    let manifest: toml::Table = toml::from_str(
        &std::fs::read_to_string(repo_path("crates/fraiseql/Cargo.toml")).expect("read"),
    )
    .expect("crates/fraiseql/Cargo.toml parses");
    let forwarded: BTreeSet<String> = manifest["features"]
        .as_table()
        .expect("[features] table")
        .keys()
        .flat_map(|f| umbrella_forwards(f))
        .collect();
    for r in REGISTRY {
        if r.meta.maturity == Maturity::Stable && r.meta.release != Release::Default {
            assert!(
                forwarded.contains(r.meta.feature),
                "`{}` is Stable but no `fraiseql` feature forwards `fraiseql-server?/{}`",
                r.meta.name,
                r.meta.feature
            );
        }
    }
}

const MATRIX_DOC: &str = "docs/features-compatibility-matrix.md";
const MATRIX_BEGIN: &str = "<!-- capabilities:begin -->";
const MATRIX_END: &str = "<!-- capabilities:end -->";

/// The capability table of the features matrix, rendered from the registry.
fn render_matrix() -> String {
    use std::fmt::Write as _;

    use fraiseql_core::capability::Release;
    let code = |items: &mut dyn Iterator<Item = &str>| {
        let joined = items.map(|i| format!("`{i}`")).collect::<Vec<_>>().join(", ");
        if joined.is_empty() {
            "—".to_string()
        } else {
            joined
        }
    };
    let mut out = String::from(
        "| Capability | Feature | Maturity | Published in | Config keys | Compiled-schema sections | Docs |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for r in REGISTRY {
        let m = &r.meta;
        let published = match m.release {
            Release::Default => "every image and tarball",
            Release::Full => "`-full`, `-platform`",
            Release::Platform => "`-platform`",
            Release::Source => "none: build from source",
            _ => panic!("no matrix wording for {:?}", m.release),
        };
        let maturity = serde_json::to_value(m.maturity).expect("serialize");
        let docs = m.docs.map_or_else(
            || "—".to_string(),
            |d| format!("[{}]({})", d.trim_start_matches("docs/"), d.trim_start_matches("docs/")),
        );
        let _ = writeln!(
            out,
            "| `{}` | `{}` | {} | {published} | {} | {} | {docs} |",
            m.name,
            m.feature,
            maturity.as_str().expect("string"),
            code(&mut m.runtime_keys.iter().copied()),
            code(&mut m.compiled_sections.iter().map(|s| s.key)),
        );
    }
    out
}

/// The capability table in `docs/features-compatibility-matrix.md` is the registry,
/// byte for byte. `make gen-matrix` rewrites it (this test, with `FRAISEQL_GEN_MATRIX=1`).
#[test]
fn the_capability_matrix_is_generated_from_the_registry() {
    let path = repo_path(MATRIX_DOC);
    let doc = std::fs::read_to_string(&path).expect("read the matrix");
    let begin = doc.find(MATRIX_BEGIN).expect("the matrix has a capabilities:begin marker")
        + MATRIX_BEGIN.len();
    let end = doc.find(MATRIX_END).expect("the matrix has a capabilities:end marker");
    let expected = format!("\n{}", render_matrix());
    if std::env::var_os("FRAISEQL_GEN_MATRIX").is_some() {
        std::fs::write(&path, format!("{}{expected}{}", &doc[..begin], &doc[end..]))
            .expect("write the matrix");
        return;
    }
    assert!(
        doc[begin..end] == expected,
        "{MATRIX_DOC} is stale against the capability registry; run `make gen-matrix`"
    );
}
