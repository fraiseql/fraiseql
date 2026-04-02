//! Federation gateway command
//!
//! `fraiseql federation gateway <config.toml>` starts a federation gateway
//! that routes GraphQL queries across multiple FraiseQL subgraph instances.

pub mod config;
pub mod merger;
pub mod planner;
pub mod server;

use std::path::Path;

use anyhow::Result;
use serde_json::json;

use self::{config::GatewayConfig, planner::FieldOwnership};
use crate::output::CommandResult;

/// Run the gateway command: validate config, compose schema, start server.
///
/// # Errors
///
/// Returns an error if configuration is invalid or the server fails to start.
pub async fn run(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);
    let base_dir = path.parent().unwrap_or(Path::new("."));

    // Load and validate config
    eprintln!("Loading gateway configuration from {config_path}...");
    let config = config::load_config(path)?;

    if let Err(errors) = config::validate_config(&config, base_dir) {
        for e in &errors {
            eprintln!("  Config error: {e}");
        }
        anyhow::bail!("Gateway configuration has {} error(s)", errors.len());
    }

    // Print startup summary
    print_startup_summary(&config);

    // Build field ownership from subgraph schemas
    // For now, this is built from a simple mapping. In a full implementation,
    // the gateway would introspect each subgraph's schema (via _service query
    // or local SDL files) and build the ownership map from the composed schema.
    let ownership = build_field_ownership(&config).await?;

    // Start the HTTP server
    server::serve(&config, ownership).await
}

/// Validate gateway configuration and return a `CommandResult`.
///
/// Used by `fraiseql federation gateway --check` to validate without starting.
///
/// # Errors
///
/// Returns an error if the config file cannot be read or parsed.
pub fn validate(config_path: &str) -> Result<CommandResult> {
    let path = Path::new(config_path);
    let base_dir = path.parent().unwrap_or(Path::new("."));

    let config = config::load_config(path)?;

    if let Err(errors) = config::validate_config(&config, base_dir) {
        let error_strings: Vec<String> = errors.iter().map(ToString::to_string).collect();
        return Ok(CommandResult {
            status: "validation-failed".to_string(),
            command: "federation/gateway".to_string(),
            data: None,
            message: Some(format!("{} validation error(s)", error_strings.len())),
            code: Some("GATEWAY_CONFIG_INVALID".to_string()),
            errors: error_strings,
            warnings: Vec::new(),
        });
    }

    let subgraph_names: Vec<String> = config.subgraphs.keys().cloned().collect();
    Ok(CommandResult::success(
        "federation/gateway",
        json!({
            "valid": true,
            "listen": config.listen,
            "subgraphs": subgraph_names,
            "timeouts": {
                "subgraph_request_ms": config.timeouts.subgraph_request_ms,
                "total_request_ms": config.timeouts.total_request_ms,
            },
        }),
    ))
}

/// Build field ownership by introspecting subgraphs.
///
/// For the MVP, each subgraph is assumed to own root fields matching its name.
/// A full implementation would fetch the schema from each subgraph and parse
/// the root Query/Mutation types to build an accurate mapping.
async fn build_field_ownership(config: &GatewayConfig) -> Result<FieldOwnership> {
    let mut ownership = FieldOwnership::default();

    for (name, sg_config) in &config.subgraphs {
        // Try to fetch schema from subgraph if no local SDL provided
        let fields = if let Some(schema_path) = &sg_config.schema {
            extract_fields_from_sdl(schema_path)?
        } else {
            match fetch_schema_from_subgraph(&sg_config.url).await {
                Ok(fields) => fields,
                Err(e) => {
                    eprintln!("  Warning: Could not fetch schema from subgraph '{name}': {e}");
                    eprintln!("  Falling back to subgraph name as root field");
                    vec![String::from(name.as_str())]
                },
            }
        };

        for field in fields {
            ownership.insert(field, name.clone());
        }
    }

    Ok(ownership)
}

/// Extract root Query field names from a local SDL file.
fn extract_fields_from_sdl(path: &std::path::Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
    let mut fields = Vec::new();
    let mut in_query_type = false;
    let mut brace_depth: i32 = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("type Query") {
            in_query_type = true;
            if trimmed.contains('{') {
                brace_depth += 1;
            }
            continue;
        }

        if in_query_type {
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            in_query_type = false;
                        }
                    },
                    _ => {},
                }
            }

            if brace_depth == 1 {
                // Parse field name: `fieldName(args): Type`
                let field_name = trimmed
                    .split(['(', ':'])
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && !s.starts_with('#') && *s != "{" && *s != "}");
                if let Some(name) = field_name {
                    fields.push(name.to_string());
                }
            }
        }
    }

    Ok(fields)
}

/// Fetch the schema from a running subgraph via the `_service` SDL query.
async fn fetch_schema_from_subgraph(url: &str) -> Result<Vec<String>> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build()?;

    let resp = client
        .post(url)
        .json(&json!({
            "query": "{ _service { sdl } }"
        }))
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;

    let sdl = body["data"]["_service"]["sdl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No SDL in _service response"))?;

    // Parse field names from SDL string
    let mut fields = Vec::new();
    let mut in_query = false;
    let mut depth: i32 = 0;

    for line in sdl.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("type Query") {
            in_query = true;
            if trimmed.contains('{') {
                depth += 1;
            }
            continue;
        }
        if in_query {
            for ch in trimmed.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            in_query = false;
                        }
                    },
                    _ => {},
                }
            }
            if depth == 1 {
                let field_name = trimmed
                    .split(['(', ':'])
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && !s.starts_with('#') && *s != "{" && *s != "}");
                if let Some(name) = field_name {
                    fields.push(name.to_string());
                }
            }
        }
    }

    if fields.is_empty() {
        anyhow::bail!("No Query fields found in SDL");
    }

    Ok(fields)
}

/// Print a startup summary to stderr.
fn print_startup_summary(config: &GatewayConfig) {
    eprintln!("FraiseQL Federation Gateway");
    eprintln!("  Listen: {}", config.listen);
    eprintln!("  Subgraphs: {}", config.subgraphs.len());
    for (name, sg) in &config.subgraphs {
        let schema_info = sg
            .schema
            .as_ref()
            .map_or("(introspect at startup)".to_string(), |p: &std::path::PathBuf| {
                format!("({})", p.display())
            });
        eprintln!("    - {name}: {} {schema_info}", sg.url);
    }
    eprintln!(
        "  Timeouts: subgraph={}ms, total={}ms",
        config.timeouts.subgraph_request_ms, config.timeouts.total_request_ms
    );
    eprintln!(
        "  Circuit breaker: threshold={}, recovery={}ms",
        config.circuit_breaker.failure_threshold, config.circuit_breaker.recovery_timeout_ms
    );
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("gateway.toml");
        std::fs::write(
            &config_path,
            r#"
[gateway]
listen = "127.0.0.1:4000"

[gateway.subgraphs.users]
url = "http://localhost:4001/graphql"
"#,
        )
        .unwrap();

        let result = validate(config_path.to_str().unwrap()).unwrap();
        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_validate_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("gateway.toml");
        std::fs::write(
            &config_path,
            r#"
[gateway]
listen = "127.0.0.1:4000"
"#,
        )
        .unwrap();

        let result = validate(config_path.to_str().unwrap()).unwrap();
        assert_eq!(result.status, "validation-failed");
    }

    #[test]
    fn test_extract_fields_from_sdl() {
        let dir = tempfile::tempdir().unwrap();
        let sdl_path = dir.path().join("schema.graphql");
        std::fs::write(
            &sdl_path,
            r#"
type Query {
    users: [User!]!
    user(id: ID!): User
    products: [Product!]!
}

type User @key(fields: "id") {
    id: ID!
    name: String!
}
"#,
        )
        .unwrap();

        let fields = extract_fields_from_sdl(&sdl_path).unwrap();
        assert_eq!(fields, vec!["users", "user", "products"]);
    }

    #[test]
    fn test_extract_fields_no_query_type() {
        let dir = tempfile::tempdir().unwrap();
        let sdl_path = dir.path().join("schema.graphql");
        std::fs::write(
            &sdl_path,
            r"
type User {
    id: ID!
    name: String!
}
",
        )
        .unwrap();

        let fields = extract_fields_from_sdl(&sdl_path).unwrap();
        assert!(fields.is_empty());
    }
}
