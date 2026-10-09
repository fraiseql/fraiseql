//! #1512 on the gRPC transport: a unary read and a server-streaming read run in the request
//! locale, resolved from the request metadata (`accept-language`). #1520 likewise: a
//! `source = "header"` session variable is read from the metadata (`x-region`).
//!
//! gRPC reads native columns, so the probe view carries the setting as a column. The service is
//! built with `build_grpc_service` over a real `PostgresAdapter` and driven with
//! `tower::ServiceExt::oneshot`; the streaming read opens its statement in the handler and its
//! frames are encoded as the body is pulled.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `vr_v_locale_grpc_probe` view.
#![cfg(feature = "grpc")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{CompiledSchema, FieldType, GrpcConfig, LocaleConfig},
};
use fraiseql_server::routes::grpc::{self, DynamicGrpcService};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use http_body_util::BodyExt as _;
use prost::Message as _;
use prost_reflect::prost_types::{
    DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    MethodDescriptorProto, ServiceDescriptorProto, field_descriptor_proto,
};
use tower::ServiceExt as _;

const PACKAGE: &str = "fraiseql.v1";
const SERVICE: &str = "fraiseql.v1.FraiseqlService";
const VIEW: &str = "v_locale_grpc_probe";

fn field(name: &str, number: i32, ty: field_descriptor_proto::Type) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.into()),
        number: Some(number),
        r#type: Some(ty.into()),
        label: Some(field_descriptor_proto::Label::Optional.into()),
        ..Default::default()
    }
}

/// `Probe { id, locale }`; `ListProbes` (unary, `ListProbesResponse { repeated items }`) and
/// `ListProbeStream` (server-streaming `Probe`).
fn descriptor_set() -> FileDescriptorSet {
    use field_descriptor_proto::Type;
    let probe = DescriptorProto {
        name: Some("Probe".into()),
        field: vec![
            field("id", 1, Type::Int64),
            field("locale", 2, Type::String),
            field("label", 3, Type::String),
            field("region", 4, Type::String),
        ],
        ..Default::default()
    };
    let request = |name: &str| DescriptorProto {
        name: Some(name.into()),
        field: vec![
            field("limit", 1, Type::Int32),
            field("offset", 2, Type::Int32),
        ],
        ..Default::default()
    };
    let response = DescriptorProto {
        name: Some("ListProbesResponse".into()),
        field: vec![FieldDescriptorProto {
            name: Some("items".into()),
            number: Some(1),
            r#type: Some(Type::Message.into()),
            label: Some(field_descriptor_proto::Label::Repeated.into()),
            type_name: Some(format!(".{PACKAGE}.Probe")),
            ..Default::default()
        }],
        ..Default::default()
    };
    let service = ServiceDescriptorProto {
        name: Some("FraiseqlService".into()),
        method: vec![
            MethodDescriptorProto {
                name: Some("ListProbes".into()),
                input_type: Some(format!(".{PACKAGE}.ListProbesRequest")),
                output_type: Some(format!(".{PACKAGE}.ListProbesResponse")),
                ..Default::default()
            },
            MethodDescriptorProto {
                name: Some("ListProbeStream".into()),
                input_type: Some(format!(".{PACKAGE}.ListProbeStreamRequest")),
                output_type: Some(format!(".{PACKAGE}.Probe")),
                server_streaming: Some(true),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("service.proto".into()),
            package: Some(PACKAGE.into()),
            syntax: Some("proto3".into()),
            message_type: vec![
                probe,
                request("ListProbesRequest"),
                request("ListProbeStreamRequest"),
                response,
            ],
            service: vec![service],
            ..Default::default()
        }],
    }
}

fn schema(descriptor_path: &str) -> CompiledSchema {
    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("Probe", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::nullable("locale", FieldType::String).build())
                .with_field({
                    let mut label = TestFieldBuilder::nullable("label", FieldType::String).build();
                    label.localized = true;
                    label
                })
                .with_field(TestFieldBuilder::nullable("region", FieldType::String).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("probes", "Probe")
                .returns_list(true)
                .with_sql_source(VIEW)
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("probe_stream", "Probe")
                .returns_list(true)
                .with_sql_source(VIEW)
                .build(),
        )
        .build();
    schema.grpc_config = Some(GrpcConfig {
        enabled: true,
        descriptor_path: descriptor_path.to_string(),
        ..GrpcConfig::default()
    });
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".into(), "fr".into(), "fr-FR".into(), "de-DE".into()],
            BTreeMap::from([("fr-CA".to_string(), "fr-FR".to_string())]),
            vec![fraiseql_core::schema::LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema.session_variables = fraiseql_core::schema::SessionVariablesConfig {
        variables:         vec![fraiseql_core::schema::SessionVariableMapping {
            name:   "app.region".to_string(),
            source: fraiseql_core::schema::SessionVariableSource::Header {
                header: "x-region".to_string(),
            },
        }],
        inject_started_at: false,
    };
    schema.build_indexes();
    schema
}

async fn service(dir: &std::path::Path) -> Option<DynamicGrpcService> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    // gRPC reads the row-shaped `vr_` view of the type's source.
    for ddl in [
        format!("DROP VIEW IF EXISTS vr_{VIEW}"),
        format!(
            "CREATE VIEW vr_{VIEW} AS SELECT 1 AS id, current_setting('fraiseql.locale', true) \
             AS locale, '{{\"fr-FR\": \"Pomme\", \"en-US\": \"Apple\"}}'::jsonb AS label, \
             current_setting('app.region', true) AS region, jsonb_build_object('id', 1) AS data"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let path = dir.join("descriptor.binpb");
    std::fs::write(&path, descriptor_set().encode_to_vec()).unwrap();
    let schema = Arc::new(schema(path.to_str().unwrap()));
    let services = grpc::build_grpc_service(
        Arc::clone(&schema),
        Arc::new(Executor::new((*schema).clone(), adapter)),
        None,
        None,
        #[cfg(feature = "auth")]
        None,
    )
    .unwrap()
    .expect("gRPC is enabled");
    Some(services.service)
}

/// Call `method` with an empty request and `accept-language`; the response body's frames.
async fn call(svc: &DynamicGrpcService, method: &str, accept_language: &str) -> Vec<Vec<u8>> {
    call_with(svc, method, &[("accept-language", accept_language)]).await
}

/// Call `method` with an empty request and `metadata`; the response body's frames.
async fn call_with(
    svc: &DynamicGrpcService,
    method: &str,
    metadata: &[(&str, &str)],
) -> Vec<Vec<u8>> {
    let mut request = http::Request::builder()
        .method("POST")
        .uri(format!("/{SERVICE}/{method}"))
        .header("content-type", "application/grpc")
        .header("te", "trailers");
    for (name, value) in metadata {
        request = request.header(*name, *value);
    }
    let request = request
        .body(tonic::body::Body::new(axum::body::Body::from(vec![0, 0, 0, 0, 0])))
        .unwrap();
    let response = svc.clone().oneshot(request).await.unwrap();
    let status = format!(
        "{:?} {:?}",
        response.headers().get("grpc-status"),
        response.headers().get("grpc-message")
    );
    let body = response.into_body().collect().await.unwrap().to_bytes().to_vec();
    assert!(!body.is_empty(), "{method}: no frames ({status})");
    let mut frames = Vec::new();
    let mut at = 0;
    while at + 5 <= body.len() {
        let len =
            u32::from_be_bytes([body[at + 1], body[at + 2], body[at + 3], body[at + 4]]) as usize;
        frames.push(body[at + 5..at + 5 + len].to_vec());
        at += 5 + len;
    }
    frames
}

fn decode(bytes: &[u8], message: &str) -> prost_reflect::DynamicMessage {
    let pool =
        prost_reflect::DescriptorPool::decode(descriptor_set().encode_to_vec().as_slice()).unwrap();
    let desc = pool.get_message_by_name(&format!("{PACKAGE}.{message}")).unwrap();
    prost_reflect::DynamicMessage::decode(desc, bytes).unwrap()
}

fn locale_of(probe: &prost_reflect::DynamicMessage) -> String {
    probe
        .get_field_by_name("locale")
        .unwrap()
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn grpc_reads_run_in_the_request_locale() {
    let dir = tempfile::tempdir().unwrap();
    let Some(svc) = service(dir.path()).await else {
        eprintln!("skipping #1512 gRPC locale: DATABASE_URL not set");
        return;
    };

    let frames = call(&svc, "ListProbes", "fr-CA").await;
    let response = decode(frames.first().expect("a unary response frame"), "ListProbesResponse");
    let items = response.get_field_by_name("items").unwrap();
    let items = items.as_list().unwrap();
    let probe = items[0].as_message().unwrap();
    assert_eq!(locale_of(probe), "fr-FR", "unary read");

    let frames = call(&svc, "ListProbeStream", "de-DE").await;
    let locales: Vec<String> = frames.iter().map(|f| locale_of(&decode(f, "Probe"))).collect();
    assert_eq!(locales, vec!["de-DE".to_string()], "server-streaming read");
}

fn label_of(probe: &prost_reflect::DynamicMessage) -> String {
    probe
        .get_field_by_name("label")
        .unwrap()
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// #1513: a localized column of the row view holds the locale map; both gRPC reads return the
/// request locale's label.
#[tokio::test]
async fn grpc_reads_return_localized_labels() {
    let dir = tempfile::tempdir().unwrap();
    let Some(svc) = service(dir.path()).await else {
        return;
    };
    let frames = call(&svc, "ListProbes", "fr-CA").await;
    let response = decode(frames.first().unwrap(), "ListProbesResponse");
    let items = response.get_field_by_name("items").unwrap();
    assert_eq!(label_of(items.as_list().unwrap()[0].as_message().unwrap()), "Pomme", "unary");

    let frames = call(&svc, "ListProbeStream", "en-US").await;
    let labels: Vec<String> = frames.iter().map(|f| label_of(&decode(f, "Probe"))).collect();
    assert_eq!(labels, vec!["Apple".to_string()], "server-streaming");
}

fn region_of(probe: &prost_reflect::DynamicMessage) -> String {
    probe
        .get_field_by_name("region")
        .unwrap()
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// #1520: a `source = "header"` session variable is read from the request metadata, for the
/// unary read and for the server-streaming read (whose statement opens in the handler).
#[tokio::test]
async fn grpc_reads_see_a_header_session_variable() {
    let dir = tempfile::tempdir().unwrap();
    let Some(svc) = service(dir.path()).await else {
        return;
    };
    let frames = call_with(&svc, "ListProbes", &[("x-region", "eu")]).await;
    let response = decode(frames.first().unwrap(), "ListProbesResponse");
    let items = response.get_field_by_name("items").unwrap();
    assert_eq!(region_of(items.as_list().unwrap()[0].as_message().unwrap()), "eu", "unary");

    let frames = call_with(&svc, "ListProbeStream", &[("x-region", "eu")]).await;
    let regions: Vec<String> = frames.iter().map(|f| region_of(&decode(f, "Probe"))).collect();
    assert_eq!(regions, vec!["eu".to_string()], "server-streaming");
}
