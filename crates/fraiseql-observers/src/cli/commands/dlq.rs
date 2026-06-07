//! Dead letter queue management commands.
//!
//! These subcommands operate against a running server's observer admin API
//! under `/api/observers/dlq`. They never fabricate data: a non-2xx response or
//! an unreachable server surfaces as an error (non-zero exit), never as a
//! synthetic success (#341).

use std::fmt::Write as _;

use colored::Colorize;
use reqwest::{Client, Method};
use serde_json::Value;

use crate::{
    cli::{DlqSubcommand, OutputFormat},
    error::{ObserverError, Result},
};

/// HTTP client for the server's observer DLQ admin API.
struct ApiClient {
    base_url: String,
    token:    Option<String>,
    http:     Client,
}

impl ApiClient {
    fn new(base_url: &str, token: Option<&str>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token:    token.map(ToString::to_string),
            http:     Client::new(),
        }
    }

    /// Send a request to `path`, attaching the bearer token, and return the
    /// parsed JSON body.
    ///
    /// A transport failure (server unreachable) or a non-2xx status is an
    /// error — never swallowed as success (#341).
    async fn send(&self, method: Method, path: &str) -> Result<Value> {
        let url = format!("{}{path}", self.base_url);
        let mut request = self.http.request(method, &url);
        if let Some(ref token) = self.token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(|e| ObserverError::DlqError {
            reason: format!("request to {url} failed: {e}"),
        })?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(ObserverError::DlqError {
                reason: format!(
                    "server returned HTTP {} for {url}: {}",
                    status.as_u16(),
                    body.trim()
                ),
            });
        }

        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|e| ObserverError::DlqError {
            reason: format!("invalid JSON in response from {url}: {e}"),
        })
    }
}

/// Execute DLQ subcommands against the server admin API at `base_url`.
///
/// # Errors
///
/// Returns [`ObserverError::DlqError`] if the server is unreachable, returns a
/// non-2xx status, or returns an unparseable body — failures are surfaced, never
/// reported as success.
pub async fn execute(
    format: OutputFormat,
    base_url: &str,
    admin_token: Option<&str>,
    subcommand: DlqSubcommand,
) -> Result<()> {
    let client = ApiClient::new(base_url, admin_token);

    match subcommand {
        DlqSubcommand::List {
            limit,
            offset,
            observer,
            after,
        } => {
            warn_unsupported(
                "dlq list",
                &[
                    ("--observer", observer.is_some()),
                    ("--after", after.is_some()),
                ],
            );
            let mut path = format!("/api/observers/dlq?limit={limit}");
            if let Some(offset) = offset {
                let _ = write!(path, "&offset={offset}");
            }
            let value = client.send(Method::GET, &path).await?;
            print_response(format, "Dead Letter Queue Items", &value);
        },
        DlqSubcommand::Show { item_id } => {
            let value = client.send(Method::GET, &format!("/api/observers/dlq/{item_id}")).await?;
            print_response(format, "DLQ Item", &value);
        },
        DlqSubcommand::Retry { item_id, force: _ } => {
            let value = client
                .send(Method::POST, &format!("/api/observers/dlq/{item_id}/retry"))
                .await?;
            print_response(format, "Retry Result", &value);
        },
        DlqSubcommand::RetryAll {
            observer,
            after,
            dry_run,
        } => {
            warn_unsupported(
                "dlq retry-all",
                &[
                    ("--observer", observer.is_some()),
                    ("--after", after.is_some()),
                    ("--dry-run", dry_run),
                ],
            );
            let value = client.send(Method::POST, "/api/observers/dlq/retry-all").await?;
            print_response(format, "Batch Retry Result", &value);
        },
        DlqSubcommand::Remove { item_id, force: _ } => {
            let value =
                client.send(Method::DELETE, &format!("/api/observers/dlq/{item_id}")).await?;
            print_response(format, "Remove Result", &value);
        },
        DlqSubcommand::Stats {
            by_observer,
            by_error,
        } => {
            warn_unsupported(
                "dlq stats",
                &[("--by-observer", by_observer), ("--by-error", by_error)],
            );
            let value = client.send(Method::GET, "/api/observers/dlq/stats").await?;
            print_response(format, "DLQ Statistics", &value);
        },
    }

    Ok(())
}

/// Emit a stderr warning for any flag the server DLQ API does not support, so a
/// requested filter is never *silently* ignored.
fn warn_unsupported(command: &str, flags: &[(&str, bool)]) {
    for (flag, set) in flags {
        if *set {
            eprintln!(
                "{}: `{command} {flag}` is not supported by the server DLQ API and is ignored",
                "warning".yellow().bold()
            );
        }
    }
}

/// Print a server response. In text mode a bold title precedes the JSON; in JSON
/// mode the body is printed as-is (pretty). The data is the server's real
/// response — these commands never fabricate.
fn print_response(format: OutputFormat, title: &str, value: &Value) {
    if format == OutputFormat::Text {
        println!("{}", title.bold().underline());
    }
    let rendered = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    println!("{rendered}");
}

#[cfg(test)]
mod tests;
