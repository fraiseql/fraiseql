//! Authors the cross-SDK conformance fixture with the Rust SDK's public API.
//!
//! Driven by `sdks/official/conformance/run.py`; see
//! `sdks/official/conformance/README.md`.
//!
//! The one rule for every SDK's copy of this file: author through the SDK, never
//! hand-assemble the JSON.
//!
//! This SDK is field-level-RBAC focused: it registers types and their fields and has no
//! query, mutation, enum or input-object builders. Those gaps are declared in
//! `conformance/manifest.json` with their reasons, and the harness holds it to exactly
//! what it claims to support.

use std::{env, fs};

use fraiseql_rust::{Field, SchemaRegistry, VectorConfig, VectorIndex, VectorMetric};

fn minimal() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register_type_with_source(
        "User",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("email", "String").with_nullable(false),
        ],
        "v_user",
    );
    registry
}

fn full() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register_type_with_source(
        "User",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("email", "String").with_nullable(false),
            // The quote characters are deliberate: this description is what used to make
            // the export unparseable JSON (#855).
            Field::new("name", "String")
                .with_nullable(true)
                .with_description(Some("The user's \"display\" name".to_string()))
                .with_deprecated(Some("use displayName".to_string())),
            Field::new("salary", "Float")
                .with_nullable(true)
                .with_requires_scope(Some("read:User.salary".to_string())),
            // Two words and a digit segment (#1249). The Rust SDK's author writes the wire
            // name, so these are already camelCase; the translation is exercised by the
            // SDKs whose identifiers are idiomatic instead (Python, Ruby, Elixir, C#, F#).
            Field::new("lastLoginAt", "String").with_nullable(true),
            Field::new("phone1", "String").with_nullable(true),
        ],
        "v_user",
    );
    registry.register_type_with_source(
        "Order",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("total", "Float").with_nullable(false),
            Field::new("status", "String").with_nullable(false),
        ],
        "v_order",
    );
    registry.register_type_with_source(
        "UserNotFound",
        vec![
            Field::new("message", "String").with_nullable(false),
            Field::new("code", "String").with_nullable(false),
        ],
        "v_user_not_found",
    );
    registry.register_type_with_source(
        "Document",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("embedding", "Vector").with_nullable(false).with_vector_config(Some(
                VectorConfig::new(1536)
                    .with_index(VectorIndex::IvfFlat)
                    .with_metric(VectorMetric::L2),
            )),
            Field::new("fingerprint", "BitVector")
                .with_nullable(false)
                .with_vector_config(Some(
                    VectorConfig::new(768).with_metric(VectorMetric::Hamming),
                )),
            Field::new("compact", "HalfVector").with_nullable(true).with_vector_config(Some(
                VectorConfig::new(1536).with_metric(VectorMetric::InnerProduct),
            )),
            Field::new("terms", "SparseVector")
                .with_nullable(true)
                .with_vector_config(Some(VectorConfig::new(30000).with_index(VectorIndex::None))),
            Field::new("similarity", "Float")
                .with_nullable(false)
                .with_vector_distance(Some("embedding".to_string())),
        ],
        "v_document",
    );
    registry
}

fn main() {
    let fixture =
        env::var("FRAISEQL_CONFORMANCE_FIXTURE").expect("FRAISEQL_CONFORMANCE_FIXTURE must be set");
    let out = env::var("FRAISEQL_CONFORMANCE_OUT").expect("FRAISEQL_CONFORMANCE_OUT must be set");

    let registry = match fixture.as_str() {
        "minimal" => minimal(),
        "full" => full(),
        other => panic!("unknown fixture {other}"),
    };

    fs::write(out, registry.export_to_json()).expect("failed to write schema.json");
}
