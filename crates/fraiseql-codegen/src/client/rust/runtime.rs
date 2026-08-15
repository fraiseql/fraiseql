//! The `client.rs` runtime template.
//!
//! Identical for every generated client. Rust's standard library has no HTTP
//! client, so — unlike the Python (`urllib`) and Go (`net/http`) runtimes — the
//! transport is a trait the consumer supplies, with a blanket implementation for
//! any closure. That keeps the generated crate's dependencies to `serde` and
//! `serde_json` and leaves the choice of `reqwest`/`ureq`/blocking/async to the
//! caller, where it belongs.

/// Contents of the generated `client.rs` (without the auto-generated header).
pub(super) const CLIENT_RS: &str = r#"//! Minimal GraphQL client. The transport is supplied by the caller: Rust has no
//! HTTP client in `std`, and a generated crate is the wrong place to choose one.
//!
//! ```ignore
//! let client = FraiseqlClient::new(|body: &str| {
//!     ureq::post("https://api.example.com/graphql")
//!         .set("content-type", "application/json")
//!         .send_string(body)
//!         .map_err(|e| Error::transport(e.to_string()))?
//!         .into_string()
//!         .map_err(|e| Error::transport(e.to_string()))
//! });
//! ```

use serde::{Serialize, de::DeserializeOwned};

/// One entry of a GraphQL response's `errors` array.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResponseError {
    /// Human-readable error message.
    pub message: String,
    /// Response path the error applies to, when the server reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<serde_json::Value>,
    /// Source locations in the document, when the server reports them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<serde_json::Value>,
    /// Server-defined extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,
}

/// Everything that can go wrong executing an operation.
#[derive(Debug)]
pub enum Error {
    /// The transport itself failed (connection, TLS, non-2xx status).
    Transport(String),
    /// A request or response could not be encoded/decoded.
    Serde(serde_json::Error),
    /// The response carried a GraphQL `errors` array.
    GraphQL(Vec<ResponseError>),
    /// The response carried no `data`, or no field for this operation.
    NoData(String),
}

impl Error {
    /// Build a [`Error::Transport`] from any message.
    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport(message.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(message) => write!(f, "transport error: {message}"),
            Self::Serde(error) => write!(f, "serialization error: {error}"),
            Self::GraphQL(errors) => match errors.first() {
                Some(first) => write!(f, "GraphQL error: {}", first.message),
                None => write!(f, "GraphQL error"),
            },
            Self::NoData(what) => write!(f, "GraphQL response contained no {what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serde(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Serde(error)
    }
}

/// Sends a JSON request body to the GraphQL endpoint and returns the raw
/// response body.
///
/// Implemented for any `Fn(&str) -> Result<String, Error>`, so a closure is
/// usually all a caller needs.
pub trait Transport {
    /// Execute one request. `body` is a complete `{"query":…,"variables":…}`
    /// JSON document; the returned string must be the raw response body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] when the request cannot be completed or the
    /// server answers with a non-success status.
    fn execute(&self, body: &str) -> Result<String, Error>;
}

impl<F> Transport for F
where
    F: Fn(&str) -> Result<String, Error>,
{
    fn execute(&self, body: &str) -> Result<String, Error> {
        self(body)
    }
}

/// Executes GraphQL documents against a FraiseQL endpoint. The generated
/// operation functions wrap [`FraiseqlClient::request`] and unwrap their single
/// root field.
#[derive(Debug, Clone)]
pub struct FraiseqlClient<T> {
    transport: T,
}

#[derive(serde::Deserialize)]
struct GraphQLResponse {
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    errors: Vec<ResponseError>,
}

impl<T: Transport> FraiseqlClient<T> {
    /// Build a client over `transport`.
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Execute `document` and deserialize `data[root_field]` into `R`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphQL`] when the response carries errors,
    /// [`Error::NoData`] when it carries no `data` or no such root field,
    /// [`Error::Transport`] when the request fails, and [`Error::Serde`] when a
    /// payload cannot be encoded or decoded.
    pub fn request<R: DeserializeOwned>(
        &self,
        document: &str,
        variables: serde_json::Map<String, serde_json::Value>,
        root_field: &str,
    ) -> Result<R, Error> {
        let body = serde_json::to_string(&serde_json::json!({
            "query": document,
            "variables": serde_json::Value::Object(variables),
        }))?;

        let raw = self.transport.execute(&body)?;
        let response: GraphQLResponse = serde_json::from_str(&raw)?;

        if !response.errors.is_empty() {
            return Err(Error::GraphQL(response.errors));
        }
        let data = response.data.ok_or_else(|| Error::NoData("data".to_string()))?;
        let field = match data {
            serde_json::Value::Object(mut map) => map
                .remove(root_field)
                .ok_or_else(|| Error::NoData(format!("field `{root_field}`")))?,
            other => other,
        };
        Ok(serde_json::from_value(field)?)
    }
}

/// Insert a required operation argument into the variables map.
///
/// # Errors
///
/// Returns [`Error::Serde`] if the value cannot be encoded as JSON.
pub fn var<V: Serialize>(
    variables: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: &V,
) -> Result<(), Error> {
    variables.insert(name.to_string(), serde_json::to_value(value)?);
    Ok(())
}

/// Insert an optional operation argument, omitting it entirely when `None`.
///
/// The server then applies its own default. Sending an explicit JSON `null` for
/// an optional argument is not supported by the generated wrappers — call
/// [`FraiseqlClient::request`] directly for that.
///
/// # Errors
///
/// Returns [`Error::Serde`] if the value cannot be encoded as JSON.
pub fn var_opt<V: Serialize>(
    variables: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: Option<&V>,
) -> Result<(), Error> {
    if let Some(value) = value {
        variables.insert(name.to_string(), serde_json::to_value(value)?);
    }
    Ok(())
}
"#;
