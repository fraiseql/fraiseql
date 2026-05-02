//! `fraiseql schema metadata` — display field-level security metadata from a running server.
//!
//! Fetches `GET /api/v1/schema/metadata` and renders the result as an aligned table:
//!
//! ```text
//! Field       Encrypted  Scope     On Deny
//! ----------  ---------  --------  -------
//! User.email  true       -         -
//! User.ssn    -          read:pii  mask
//! ```

use anyhow::Result;

/// Fetch schema metadata from `server_url` and print as a formatted table.
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the server responds with a non-2xx status,
/// or the response body cannot be parsed as the expected JSON shape.
pub async fn run(server_url: &str, token: Option<&str>) -> Result<()> {
    let url = format!(
        "{}/api/v1/schema/metadata",
        server_url.trim_end_matches('/')
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let mut req = client.get(&url);
    if let Some(tok) = token {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to server at {url}: {e}"))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "Server returned HTTP {}",
            resp.status()
        ));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse server response: {e}"))?;

    let metadata = body
        .pointer("/data/metadata")
        .ok_or_else(|| anyhow::anyhow!("Unexpected response shape — missing /data/metadata"))?;

    print!("{}", format_table(metadata));
    Ok(())
}

/// Render field security metadata as an aligned plain-text table.
///
/// Each entry in `metadata` corresponds to one row. Optional columns (`Encrypted`, `Scope`,
/// `On Deny`) show `"-"` when not set.
pub fn format_table(metadata: &serde_json::Value) -> String {
    let Some(obj) = metadata.as_object() else {
        return "No metadata entries found.\n".to_string();
    };

    if obj.is_empty() {
        return "No metadata entries found.\n".to_string();
    }

    // Build rows: (field, encrypted, scope, on_deny)
    let mut rows: Vec<(String, String, String, String)> = obj
        .iter()
        .map(|(field, meta)| {
            let encrypted =
                if meta.get("encrypted").and_then(|v| v.as_bool()).unwrap_or(false) {
                    "true".to_string()
                } else {
                    "-".to_string()
                };
            let scope = meta
                .get("requires_scope")
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();
            let on_deny = meta
                .get("on_deny")
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();
            (field.clone(), encrypted, scope, on_deny)
        })
        .collect();

    rows.sort_by(|a, b| a.0.cmp(&b.0));

    // Column widths: max of header and all row values
    let w0 = rows
        .iter()
        .map(|r| r.0.len())
        .max()
        .unwrap_or(0)
        .max("Field".len());
    let w1 = rows
        .iter()
        .map(|r| r.1.len())
        .max()
        .unwrap_or(0)
        .max("Encrypted".len());
    let w2 = rows
        .iter()
        .map(|r| r.2.len())
        .max()
        .unwrap_or(0)
        .max("Scope".len());
    let w3 = rows
        .iter()
        .map(|r| r.3.len())
        .max()
        .unwrap_or(0)
        .max("On Deny".len());

    let mut out = String::new();
    out.push_str(&format!(
        "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}\n",
        "Field", "Encrypted", "Scope", "On Deny"
    ));
    out.push_str(&format!(
        "{:-<w0$}  {:-<w1$}  {:-<w2$}  {:-<w3$}\n",
        "", "", "", ""
    ));
    for (field, enc, scope, on_deny) in &rows {
        out.push_str(&format!(
            "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}\n",
            field, enc, scope, on_deny
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    #[test]
    fn test_format_table_contains_expected_values() {
        let metadata = json!({
            "User.email": {"encrypted": true},
            "User.ssn": {"requires_scope": "read:pii", "on_deny": "mask"}
        });

        let table = format_table(&metadata);

        assert!(table.contains("User.email"), "Missing User.email:\n{table}");
        assert!(table.contains("true"), "Missing encrypted=true:\n{table}");
        assert!(table.contains("User.ssn"), "Missing User.ssn:\n{table}");
        assert!(table.contains("read:pii"), "Missing scope read:pii:\n{table}");
        assert!(table.contains("mask"), "Missing on_deny=mask:\n{table}");
    }

    #[test]
    fn test_format_table_headers_present() {
        let metadata = json!({"User.email": {"encrypted": true}});
        let table = format_table(&metadata);

        assert!(table.contains("Field"), "Missing Field header");
        assert!(table.contains("Encrypted"), "Missing Encrypted header");
        assert!(table.contains("Scope"), "Missing Scope header");
        assert!(table.contains("On Deny"), "Missing On Deny header");
    }

    #[test]
    fn test_format_table_empty_metadata() {
        let table = format_table(&json!({}));
        assert!(table.contains("No metadata"), "Empty metadata should report no entries:\n{table}");
    }

    #[test]
    fn test_format_table_missing_optional_fields_show_dash() {
        let metadata = json!({"User.name": {}});
        let table = format_table(&metadata);

        assert!(table.contains("User.name"), "Missing field name:\n{table}");
        // All optional columns should default to "-"
        let data_line = table
            .lines()
            .find(|l| l.contains("User.name"))
            .unwrap();
        assert!(data_line.contains('-'), "Missing dash for unset columns: {data_line}");
    }

    #[test]
    fn test_format_table_rows_sorted_alphabetically() {
        let metadata = json!({
            "User.ssn": {"requires_scope": "read:pii"},
            "User.email": {"encrypted": true}
        });
        let table = format_table(&metadata);
        let email_pos = table.find("User.email").unwrap();
        let ssn_pos = table.find("User.ssn").unwrap();
        assert!(email_pos < ssn_pos, "Rows should be sorted: email before ssn");
    }

    #[test]
    fn test_format_table_separator_line_present() {
        let metadata = json!({"User.email": {"encrypted": true}});
        let table = format_table(&metadata);
        // The separator line consists only of dashes and spaces
        let has_separator = table
            .lines()
            .any(|l| !l.is_empty() && l.chars().all(|c| c == '-' || c == ' '));
        assert!(has_separator, "Missing separator line:\n{table}");
    }
}
