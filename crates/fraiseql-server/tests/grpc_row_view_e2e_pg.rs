//! A gRPC read serves a temporal value as GraphQL and REST serve it, or refuses it in the
//! same words.
//!
//! A gRPC read decodes each cell of the type's `vr_*` view by the column's own PostgreSQL
//! type: a `timestamptz` is written with its offset, a `timestamp` without one, a `time` as
//! PostgreSQL writes it (`24:00:00` included, which a `Time` refuses), and a text column as it
//! is stored. A `DateTime`, `LocalDateTime`, `Date` or `Time` field is a proto
//! `string` carrying its scalar's format. A value outside that format refuses the read
//! (`INTERNAL`) with the message GraphQL completion gives for the same field and stored
//! value, on a unary and a server-streaming call alike: a `DateTime` without an offset is not
//! read in any time zone. A `LocalDateTime` field is read like the others, and a missing one
//! is refused when it is published non-null.
//!
//! A column of a type that is not decoded, a `bytea` say, refuses the read, naming the column
//! and its type, though its field is nullable.
//!
//! An `ID` field is a proto `string`: a string is served as stored, an integer as its
//! decimal text, as GraphQL serves them; any other kind is refused in GraphQL's words. A
//! value of a kind its proto field cannot carry refuses the read, naming the field.
//!
//! A descriptor declaring a temporal or `ID` field as anything but a `string` is refused
//! when the service is built, saying what to declare instead.
//!
//! Each case is its own type over its own one-row view, so one refusal does not hide
//! another. Its document view (read by GraphQL) is built from that same row.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `v_grpc_row_view_*` views → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{CompiledSchema, FieldType, GrpcConfig},
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

/// A served row: an instant, a local date-time, a date and a time, each in its format.
const SERVED: [&str; 4] = [
    "'2024-01-15 10:00:00+00'::timestamptz",
    "'2024-01-15 10:00:00'::timestamp",
    "'2024-01-15'::date",
    "'10:00:00'::text",
];

/// A stored value of a type no rung decodes, distinctive enough that a message quoting it
/// would show.
const UNDECODED: &str = "'\\xfeedc0de'::bytea";

/// Each case: its name (`Pascal`, `snake`), its row's `at`, `local`, `day` and `clock`
/// columns, and the field it is refused at (`None`: served).
struct Case {
    pascal:  &'static str,
    snake:   &'static str,
    columns: [&'static str; 4],
    refused: Option<&'static str>,
}

const fn with(at: usize, column: &'static str) -> [&'static str; 4] {
    let mut columns = SERVED;
    columns[at] = column;
    columns
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            pascal:  "Served",
            snake:   "served",
            columns: SERVED,
            refused: None,
        },
        Case {
            pascal:  "OffsetlessText",
            snake:   "offsetless_text",
            columns: with(0, "'2024-01-15T10:00:00'::text"),
            refused: Some("at"),
        },
        Case {
            pascal:  "OffsetlessColumn",
            snake:   "offsetless_column",
            columns: with(0, "'2024-01-15 10:00:00'::timestamp"),
            refused: Some("at"),
        },
        Case {
            pascal:  "NumberInstant",
            snake:   "number_instant",
            columns: with(0, "7"),
            refused: Some("at"),
        },
        Case {
            pascal:  "OffsetLocal",
            snake:   "offset_local",
            columns: with(1, "'2024-01-15 10:00:00+00'::timestamptz"),
            refused: Some("local"),
        },
        Case {
            pascal:  "MissingLocal",
            snake:   "missing_local",
            columns: with(1, "NULL::timestamp"),
            refused: Some("local"),
        },
        Case {
            pascal:  "DateTimeDay",
            snake:   "date_time_day",
            columns: with(2, "'2024-01-15T10:00:00'::text"),
            refused: Some("day"),
        },
        Case {
            pascal:  "TimeColumn",
            snake:   "time_column",
            columns: with(3, "'10:00:00.5'::time"),
            refused: None,
        },
        Case {
            pascal:  "HourTwentyFour",
            snake:   "hour_twenty_four",
            columns: with(3, "'24:00:00'::time"),
            refused: Some("clock"),
        },
        Case {
            pascal:  "UndecodedClock",
            snake:   "undecoded_clock",
            columns: with(3, UNDECODED),
            refused: Some("clock"),
        },
        Case {
            pascal:  "WordClock",
            snake:   "word_clock",
            columns: with(3, "'noon'::text"),
            refused: Some("clock"),
        },
    ]
}

fn field(name: &str, number: i32, ty: field_descriptor_proto::Type) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.into()),
        number: Some(number),
        r#type: Some(ty.into()),
        label: Some(field_descriptor_proto::Label::Optional.into()),
        ..Default::default()
    }
}

/// Each `ID` case: its name (`Pascal`, `snake`), its row's `id` and `n` columns, what its
/// `id` is served as, and the field its read is refused at (`None`: served).
struct KeyCase {
    pascal:  &'static str,
    snake:   &'static str,
    id:      &'static str,
    n:       &'static str,
    served:  &'static str,
    refused: Option<&'static str>,
}

const UUID: &str = "00000000-0000-4000-8000-000000000007";

fn key_cases() -> Vec<KeyCase> {
    vec![
        KeyCase {
            pascal:  "Integer",
            snake:   "integer",
            id:      "7::int",
            n:       "NULL::int",
            served:  "7",
            refused: None,
        },
        KeyCase {
            pascal:  "Text",
            snake:   "text",
            id:      "'k-1'::text",
            n:       "NULL::int",
            served:  "k-1",
            refused: None,
        },
        KeyCase {
            pascal:  "Uuid",
            snake:   "uuid",
            id:      "'00000000-0000-4000-8000-000000000007'::uuid",
            n:       "NULL::int",
            served:  UUID,
            refused: None,
        },
        KeyCase {
            pascal:  "Fraction",
            snake:   "fraction",
            id:      "7.5::float8",
            n:       "NULL::int",
            served:  "",
            refused: Some("id"),
        },
        KeyCase {
            pascal:  "Uncarried",
            snake:   "uncarried",
            id:      "'k-1'::text",
            n:       "3::int",
            served:  "",
            refused: Some("n"),
        },
    ]
}

/// `Slot { id, at, local, day, clock }`, with `local` replaced by `local`; per case, the
/// unary `List<Case>Slots` (`ListSlotsResponse { repeated items }`) and the server-streaming
/// `List<Case>SlotStream` (`Slot`). `Stamp` is an instant message, for the refused shape.
/// `Key { id, n }`, with `id` replaced by `key_id` and `n` declared a `string` although the
/// schema's `n` is an `Int`; per `ID` case, `List<Case>Keys` and `List<Case>KeyStream`.
fn descriptor_set(local: FieldDescriptorProto, key_id: FieldDescriptorProto) -> FileDescriptorSet {
    use field_descriptor_proto::Type;
    let slot = DescriptorProto {
        name: Some("Slot".into()),
        field: vec![
            field("id", 1, Type::Int64),
            field("at", 2, Type::String),
            local,
            field("day", 4, Type::String),
            field("clock", 5, Type::String),
        ],
        ..Default::default()
    };
    let stamp = DescriptorProto {
        name: Some("Stamp".into()),
        field: vec![field("seconds", 1, Type::Int64)],
        ..Default::default()
    };
    let request = DescriptorProto {
        name: Some("ListRequest".into()),
        field: vec![field("limit", 1, Type::Int32)],
        ..Default::default()
    };
    let response = DescriptorProto {
        name: Some("ListSlotsResponse".into()),
        field: vec![FieldDescriptorProto {
            name: Some("items".into()),
            number: Some(1),
            r#type: Some(Type::Message.into()),
            label: Some(field_descriptor_proto::Label::Repeated.into()),
            type_name: Some(format!(".{PACKAGE}.Slot")),
            ..Default::default()
        }],
        ..Default::default()
    };
    let method = |name: String, output: &str, streaming: bool| MethodDescriptorProto {
        name: Some(name),
        input_type: Some(format!(".{PACKAGE}.ListRequest")),
        output_type: Some(format!(".{PACKAGE}.{output}")),
        server_streaming: Some(streaming),
        ..Default::default()
    };
    let key = DescriptorProto {
        name: Some("Key".into()),
        field: vec![key_id, field("n", 2, Type::String)],
        ..Default::default()
    };
    let keys = DescriptorProto {
        name: Some("ListKeysResponse".into()),
        field: vec![FieldDescriptorProto {
            type_name: Some(format!(".{PACKAGE}.Key")),
            ..response.field[0].clone()
        }],
        ..Default::default()
    };
    let service = ServiceDescriptorProto {
        name: Some("FraiseqlService".into()),
        method: cases()
            .iter()
            .flat_map(|c| {
                [
                    method(format!("List{}Slots", c.pascal), "ListSlotsResponse", false),
                    method(format!("List{}SlotStream", c.pascal), "Slot", true),
                ]
            })
            .chain(key_cases().iter().flat_map(|c| {
                [
                    method(format!("List{}Keys", c.pascal), "ListKeysResponse", false),
                    method(format!("List{}KeyStream", c.pascal), "Key", true),
                ]
            }))
            .collect(),
        ..Default::default()
    };
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("service.proto".into()),
            package: Some(PACKAGE.into()),
            syntax: Some("proto3".into()),
            message_type: vec![slot, stamp, request, response, key, keys],
            service: vec![service],
            ..Default::default()
        }],
    }
}

/// The descriptor every read runs over: each temporal and `ID` field a `string`.
fn strings() -> FileDescriptorSet {
    descriptor_set(field("local", 3, field_descriptor_proto::Type::String), string_id())
}

fn string_id() -> FieldDescriptorProto {
    field("id", 1, field_descriptor_proto::Type::String)
}

fn schema(descriptor_path: &str) -> CompiledSchema {
    let mut builder = TestSchemaBuilder::new();
    for case in cases() {
        let view = format!("v_grpc_row_view_{}", case.snake);
        builder = builder
            .with_type(
                TestTypeBuilder::new(&format!("Slot{}", case.pascal), &view)
                    .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                    .with_nullable_field("at", FieldType::DateTime)
                    .with_field(TestFieldBuilder::new("local", FieldType::LocalDateTime).build())
                    .with_nullable_field("day", FieldType::Date)
                    .with_nullable_field("clock", FieldType::Time)
                    .build(),
            )
            .with_query(
                TestQueryBuilder::new(
                    &format!("{}_slots", case.snake),
                    &format!("Slot{}", case.pascal),
                )
                .returns_list(true)
                .with_sql_source(&view)
                .build(),
            )
            .with_query(
                TestQueryBuilder::new(
                    &format!("{}_slot_stream", case.snake),
                    &format!("Slot{}", case.pascal),
                )
                .returns_list(true)
                .with_sql_source(&view)
                .build(),
            );
    }
    for case in key_cases() {
        let view = format!("v_grpc_row_view_key_{}", case.snake);
        let name = format!("Key{}", case.pascal);
        builder = builder
            .with_type(
                TestTypeBuilder::new(&name, &view)
                    .with_field(TestFieldBuilder::new("id", FieldType::Id).build())
                    .with_nullable_field("n", FieldType::Int)
                    .build(),
            )
            .with_query(
                TestQueryBuilder::new(&format!("{}_keys", case.snake), &name)
                    .returns_list(true)
                    .with_sql_source(&view)
                    .build(),
            )
            .with_query(
                TestQueryBuilder::new(&format!("{}_key_stream", case.snake), &name)
                    .returns_list(true)
                    .with_sql_source(&view)
                    .build(),
            );
    }
    let mut schema = builder.build();
    schema.grpc_config = Some(GrpcConfig {
        enabled: true,
        descriptor_path: descriptor_path.to_string(),
        ..GrpcConfig::default()
    });
    schema.build_indexes();
    schema
}

async fn seed(adapter: &PostgresAdapter) {
    for case in cases() {
        let view = format!("v_grpc_row_view_{}", case.snake);
        let [at, local, day, clock] = case.columns;
        for ddl in [
            format!("DROP VIEW IF EXISTS {view}"),
            format!("DROP VIEW IF EXISTS vr_{view}"),
            format!(
                "CREATE VIEW vr_{view} AS SELECT 1 AS id, {at} AS at, {local} AS local, {day} AS \
                 day, {clock} AS clock"
            ),
            format!(
                "CREATE VIEW {view} AS SELECT id, jsonb_build_object('id', id, 'at', at, \
                 'local', local, 'day', day, 'clock', clock) AS data FROM vr_{view}"
            ),
        ] {
            let _: Vec<std::collections::HashMap<String, serde_json::Value>> =
                adapter.execute_raw_query(&ddl).await.unwrap();
        }
    }
    for case in key_cases() {
        let view = format!("v_grpc_row_view_key_{}", case.snake);
        let (id, n) = (case.id, case.n);
        for ddl in [
            format!("DROP VIEW IF EXISTS {view}"),
            format!("DROP VIEW IF EXISTS vr_{view}"),
            format!("CREATE VIEW vr_{view} AS SELECT {id} AS id, {n} AS n"),
            format!(
                "CREATE VIEW {view} AS SELECT jsonb_build_object('id', id, 'n', n) AS data FROM \
                 vr_{view}"
            ),
        ] {
            let _: Vec<std::collections::HashMap<String, serde_json::Value>> =
                adapter.execute_raw_query(&ddl).await.unwrap();
        }
    }
}

/// The executor, and the gRPC service built over `descriptor` (its build error, if refused).
async fn build(
    dir: &std::path::Path,
    descriptor: &FileDescriptorSet,
) -> Option<(Arc<Executor>, Result<DynamicGrpcService, String>)> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    seed(&adapter).await;
    let path = dir.join("descriptor.binpb");
    std::fs::write(&path, descriptor.encode_to_vec()).unwrap();
    let schema = schema(path.to_str().unwrap());
    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let services = grpc::build_grpc_service(
        Arc::new(schema),
        Arc::clone(&executor),
        None,
        None,
        #[cfg(feature = "auth")]
        fraiseql_server::identity::RequestIdentity::default(),
        None,
        Arc::new(fraiseql_server::config::ErrorSanitizer::disabled()),
    )
    .map(|services| services.expect("gRPC is enabled").service)
    .map_err(|e| e.to_string());
    Some((executor, services))
}

/// Call `method` with an empty request: the gRPC status, its message, and the body's
/// frames. The status is in the headers for a refused unary call, else in the trailers.
async fn call(svc: &DynamicGrpcService, method: &str) -> (String, String, Vec<Vec<u8>>) {
    let request = http::Request::builder()
        .method("POST")
        .uri(format!("/{SERVICE}/{method}"))
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .body(tonic::body::Body::new(axum::body::Body::from(vec![0, 0, 0, 0, 0])))
        .unwrap();
    let response = svc.clone().oneshot(request).await.unwrap();
    let header = |h: &http::HeaderMap, name: &str| {
        h.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
    };
    let headers = response.headers().clone();
    let collected = response.into_body().collect().await.unwrap();
    let trailers = collected.trailers().cloned().unwrap_or_default();
    let body = collected.to_bytes().to_vec();
    let status = header(&headers, "grpc-status")
        .or_else(|| header(&trailers, "grpc-status"))
        .unwrap_or_default();
    let message = header(&headers, "grpc-message")
        .or_else(|| header(&trailers, "grpc-message"))
        .unwrap_or_default();
    let mut frames = Vec::new();
    let mut at = 0;
    while at + 5 <= body.len() {
        let len =
            u32::from_be_bytes([body[at + 1], body[at + 2], body[at + 3], body[at + 4]]) as usize;
        frames.push(body[at + 5..at + 5 + len].to_vec());
        at += 5 + len;
    }
    (status, message, frames)
}

fn decode(bytes: &[u8], message: &str) -> prost_reflect::DynamicMessage {
    let pool = prost_reflect::DescriptorPool::decode(strings().encode_to_vec().as_slice()).unwrap();
    let desc = pool.get_message_by_name(&format!("{PACKAGE}.{message}")).unwrap();
    prost_reflect::DynamicMessage::decode(desc, bytes).unwrap()
}

/// A slot's temporal fields, as served.
fn temporals(slot: &prost_reflect::DynamicMessage) -> [String; 4] {
    ["at", "local", "day", "clock"]
        .map(|name| slot.get_field_by_name(name).unwrap().as_str().unwrap_or_default().to_string())
}

/// GraphQL's read of `case`: the message of its first error, if any.
async fn graphql_refusal(executor: &Executor, case: &Case) -> Option<String> {
    let query = format!("{{ {}_slots {{ id at local day clock }} }}", case.snake);
    let response = executor.execute(&query, None).await.unwrap();
    response["errors"][0]["message"].as_str().map(str::to_string)
}

#[tokio::test]
async fn a_grpc_read_serves_each_temporal_value_in_its_format() {
    let dir = tempfile::tempdir().unwrap();
    let Some((_, services)) = build(dir.path(), &strings()).await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let svc = services.expect("an all-`string` descriptor builds");
    let expected = [
        "2024-01-15T10:00:00+00:00",
        "2024-01-15T10:00:00",
        "2024-01-15",
        "10:00:00",
    ]
    .map(str::to_string);

    let (status, message, frames) = call(&svc, "ListServedSlots").await;
    assert_eq!(status, "0", "unary: {message}");
    let response = decode(&frames[0], "ListSlotsResponse");
    let items = response.get_field_by_name("items").unwrap();
    assert_eq!(temporals(items.as_list().unwrap()[0].as_message().unwrap()), expected, "unary");

    let (status, message, frames) = call(&svc, "ListServedSlotStream").await;
    assert_eq!(status, "0", "server-streaming: {message}");
    assert_eq!(frames.len(), 1, "server-streaming: one row");
    assert_eq!(temporals(&decode(&frames[0], "Slot")), expected, "server-streaming");
}

/// A `time` column is served as PostgreSQL writes it: its fraction trimmed, as the document
/// GraphQL reads carries it.
#[tokio::test]
async fn a_grpc_read_serves_a_time_column_as_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let Some((executor, services)) = build(dir.path(), &strings()).await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let svc = services.expect("an all-`string` descriptor builds");
    let graphql = executor.execute("{ time_column_slots { clock } }", None).await.unwrap();
    assert_eq!(graphql["data"]["time_column_slots"][0]["clock"], "10:00:00.5", "{graphql}");

    let (status, message, frames) = call(&svc, "ListTimeColumnSlots").await;
    assert_eq!(status, "0", "unary: {message}");
    let response = decode(&frames[0], "ListSlotsResponse");
    let items = response.get_field_by_name("items").unwrap();
    let slot = items.as_list().unwrap()[0].as_message().unwrap().clone();
    assert_eq!(temporals(&slot)[3], "10:00:00.5", "unary");

    let (status, message, frames) = call(&svc, "ListTimeColumnSlotStream").await;
    assert_eq!(status, "0", "server-streaming: {message}");
    assert_eq!(temporals(&decode(&frames[0], "Slot"))[3], "10:00:00.5", "server-streaming");
}

/// A column whose PostgreSQL type is not decoded refuses the read (`INTERNAL`), naming the
/// column and its type, never the value, though its field is nullable: it is not served unset.
#[tokio::test]
async fn a_grpc_read_refuses_a_column_whose_type_is_not_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let Some((_executor, services)) = build(dir.path(), &strings()).await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let svc = services.expect("an all-`string` descriptor builds");
    for method in ["ListUndecodedClockSlots", "ListUndecodedClockSlotStream"] {
        let (status, message, frames) = call(&svc, method).await;
        assert_eq!(status, "13", "{method}: INTERNAL, not served ({message})");
        assert!(
            message.contains("column `clock` of PostgreSQL type `bytea` is not decoded"),
            "{method}: {message}"
        );
        assert!(!message.contains("feedc0de"), "{method}: {message}");
        assert!(frames.is_empty(), "{method}: no row is served");
    }
}

#[tokio::test]
async fn a_grpc_read_refuses_a_temporal_value_as_graphql_does() {
    let dir = tempfile::tempdir().unwrap();
    let Some((executor, services)) = build(dir.path(), &strings()).await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let svc = services.expect("an all-`string` descriptor builds");
    for case in cases() {
        if case.columns.contains(&UNDECODED) {
            // Refused before any field is read, in its own words: see
            // `a_grpc_read_refuses_a_column_whose_type_is_not_decoded`.
            continue;
        }
        let Some(field) = case.refused else {
            assert_eq!(graphql_refusal(&executor, &case).await, None, "{}", case.pascal);
            continue;
        };
        let graphql = graphql_refusal(&executor, &case)
            .await
            .unwrap_or_else(|| panic!("{}: GraphQL refuses the row", case.pascal));
        assert!(graphql.contains(&format!("Slot{}.{field}", case.pascal)), "{graphql}");
        for method in [
            format!("List{}Slots", case.pascal),
            format!("List{}SlotStream", case.pascal),
        ] {
            let (status, message, frames) = call(&svc, &method).await;
            assert_eq!(status, "13", "{method}: INTERNAL, not served ({message})");
            if case.columns[1].starts_with("NULL") {
                // A missing value: gRPC's non-null refusal, naming the field as GraphQL's.
                let sentence = graphql.trim_end_matches('.');
                assert!(message.contains(sentence), "{method}: {message} / {graphql}");
            } else {
                // GraphQL's message, as the gRPC transport renders every engine error.
                let words =
                    fraiseql_core::error::FraiseQLError::internal(graphql.clone()).to_string();
                assert_eq!(message, words, "{method}: GraphQL's words for the same value");
            }
            assert!(frames.is_empty(), "{method}: no row is served");
        }
    }
}

#[tokio::test]
async fn a_descriptor_declaring_a_temporal_field_as_no_string_is_refused() {
    use field_descriptor_proto::Type;
    let dir = tempfile::tempdir().unwrap();
    let stamp = FieldDescriptorProto {
        type_name: Some(format!(".{PACKAGE}.Stamp")),
        ..field("local", 3, Type::Message)
    };
    let repeated = FieldDescriptorProto {
        label: Some(field_descriptor_proto::Label::Repeated.into()),
        ..field("local", 3, Type::String)
    };
    for (shape, local) in [
        ("an instant message", stamp),
        ("an integer", field("local", 3, Type::Int64)),
        ("a repeated string", repeated),
    ] {
        let Some((_, services)) = build(dir.path(), &descriptor_set(local, string_id())).await
        else {
            eprintln!("SKIP: DATABASE_URL not set");
            return;
        };
        let Err(error) = services else {
            panic!("{shape}: a temporal field declared as {shape} is refused");
        };
        assert!(error.contains("fraiseql.v1.Slot.local"), "{shape}: names the field: {error}");
        assert!(error.contains("LocalDateTime"), "{shape}: names the scalar: {error}");
        assert!(error.contains("Declare it `string local`"), "{shape}: says how: {error}");
    }
}

/// A gRPC read serves an `ID` as GraphQL serves it: a string as stored, an integer as its
/// decimal text, a UUID column as its text. A fraction is refused in GraphQL's words. A
/// value of a kind its proto field cannot carry (an `Int` declared `string`) refuses the
/// read, naming the field, on a unary and a streaming call alike.
#[tokio::test]
async fn a_grpc_read_serves_an_id_as_graphql_does() {
    let dir = tempfile::tempdir().unwrap();
    let Some((executor, services)) = build(dir.path(), &strings()).await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let svc = services.expect("an all-`string` descriptor builds");
    let id_of = |key: &prost_reflect::DynamicMessage| {
        key.get_field_by_name("id").unwrap().as_str().unwrap_or_default().to_string()
    };
    for case in key_cases() {
        let unary = format!("List{}Keys", case.pascal);
        let stream = format!("List{}KeyStream", case.pascal);
        let query = format!("{{ {}_keys {{ id n }} }}", case.snake);
        let graphql = executor.execute(&query, None).await.unwrap();
        match case.refused {
            None => {
                assert_eq!(graphql["data"][format!("{}_keys", case.snake)][0]["id"], case.served);
                let (status, message, frames) = call(&svc, &unary).await;
                assert_eq!(status, "0", "{unary}: {message}");
                let response = decode(&frames[0], "ListKeysResponse");
                let items = response.get_field_by_name("items").unwrap();
                assert_eq!(id_of(items.as_list().unwrap()[0].as_message().unwrap()), case.served);
                let (status, message, frames) = call(&svc, &stream).await;
                assert_eq!(status, "0", "{stream}: {message}");
                assert_eq!(id_of(&decode(&frames[0], "Key")), case.served, "{stream}");
            },
            Some("id") => {
                let refusal = graphql["errors"][0]["message"].as_str().unwrap().to_string();
                let words = fraiseql_core::error::FraiseQLError::internal(refusal).to_string();
                for method in [&unary, &stream] {
                    let (status, message, frames) = call(&svc, method).await;
                    assert_eq!(status, "13", "{method}: INTERNAL ({message})");
                    assert_eq!(message, words, "{method}: GraphQL's words");
                    assert!(frames.is_empty(), "{method}: no row is served");
                }
            },
            Some(field) => {
                assert!(graphql["errors"].is_null(), "GraphQL serves it: {graphql}");
                for method in [&unary, &stream] {
                    let (status, message, frames) = call(&svc, method).await;
                    assert_eq!(status, "13", "{method}: INTERNAL ({message})");
                    assert!(
                        message
                            .contains(&format!("`fraiseql.v1.Key.{field}` is declared `string`")),
                        "{method}: names the field and its kind: {message}"
                    );
                    assert!(message.contains("a number"), "{method}: the held kind: {message}");
                    assert!(frames.is_empty(), "{method}: no row is served");
                }
            },
        }
    }
}

/// A descriptor declaring an `ID` field as anything but a `string` is refused when the
/// service is built, naming the field and what to declare.
#[tokio::test]
async fn a_descriptor_declaring_an_id_as_no_string_is_refused() {
    use field_descriptor_proto::Type;
    let dir = tempfile::tempdir().unwrap();
    for id in [field("id", 1, Type::Int64), field("id", 1, Type::Bytes)] {
        let local = field("local", 3, Type::String);
        let Some((_, services)) = build(dir.path(), &descriptor_set(local, id)).await else {
            eprintln!("SKIP: DATABASE_URL not set");
            return;
        };
        let Err(error) = services else {
            panic!("an `ID` declared as no `string` is refused");
        };
        assert!(error.contains("fraiseql.v1.Key.id"), "names the field: {error}");
        assert!(error.contains("Declare it `string id`"), "says how: {error}");
    }
}
