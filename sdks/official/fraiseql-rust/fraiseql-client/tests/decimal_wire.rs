//! A `Decimal` travels as the string of its exact digits.
//!
//! The server takes a `Decimal` argument as a string such as `"1.50"`, or an exact
//! integer, and refuses a number with a fraction. The client sends the caller's
//! `serde_json::Value` as it is and decodes the response into the caller's type, so a
//! `String` keeps every digit both ways; an `f64` is sent as a JSON number, which the
//! server refuses for a `Decimal`.

use fraiseql_client::FraiseQLClientBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const DIGITS: [&str; 5] = [
    "1.50",
    "100.00",
    "12345678901234567890.12",
    "123456789012345678901234567890",
    "-0.00",
];

#[derive(Deserialize)]
struct Echo {
    echo: Amount,
}

#[derive(Deserialize)]
struct Amount {
    a: String,
}

/// A server that answers with the variables it received.
async fn echo_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "echo": body["variables"] } }))
        })
        .mount(&server)
        .await;
    server
}

/// The raw text of the `variables` the server received in its only request.
async fn sent_variables(server: &MockServer) -> String {
    let requests = server.received_requests().await.unwrap_or_default();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap_or(Value::Null);
    body["variables"].to_string()
}

#[tokio::test]
async fn a_decimal_string_is_sent_and_read_as_its_exact_digits() {
    for digits in DIGITS {
        let server = echo_server().await;
        let client = FraiseQLClientBuilder::new(format!("{}/graphql", server.uri())).build();

        let data: Echo = client
            .query("query($a: Decimal) { echo(a: $a) }", Some(&json!({ "a": digits })))
            .await
            .unwrap();

        assert_eq!(sent_variables(&server).await, format!(r#"{{"a":"{digits}"}}"#));
        assert_eq!(data.echo.a, digits);
    }
}

#[tokio::test]
async fn an_f64_is_sent_as_a_number_which_a_decimal_argument_refuses() {
    let server = echo_server().await;
    let client = FraiseQLClientBuilder::new(format!("{}/graphql", server.uri())).build();

    let _: Value = client
        .query("query($a: Decimal) { echo(a: $a) }", Some(&json!({ "a": 1.5 })))
        .await
        .unwrap();

    assert_eq!(sent_variables(&server).await, r#"{"a":1.5}"#);
}
