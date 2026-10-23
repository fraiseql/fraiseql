//! `fraiseql capabilities` — list every optional capability and whether this build has it.
//!
//! Reads `fraiseql_server::capability::REGISTRY`, so the answer is about the server this
//! binary links: the one `fraiseql run` starts.

use anyhow::Result;
use serde_json::{Value, json};

use crate::output::{CommandResult, OutputFormatter};

/// The registry as the command reports it.
///
/// One entry per capability, with its maturity, whether this build compiled it in, its
/// feature, config keys, compiled sections, the smallest published artifact that ships
/// it, and its docs page.
#[must_use]
pub fn report() -> Value {
    json!({ "capabilities": fraiseql_server::capability::REGISTRY })
}

/// Print the report: the `--json` envelope, or one line per capability.
///
/// # Errors
///
/// Infallible today; `Result` keeps the runner's command signature.
pub fn run(json_mode: bool, quiet: bool) -> Result<()> {
    if json_mode {
        let result = CommandResult::success("capabilities", report());
        println!("{}", OutputFormatter::new(true, quiet).format(&result));
        return Ok(());
    }
    if quiet {
        return Ok(());
    }
    println!(
        "{:<20} {:<8} {:<9} {:<20} DOCS",
        "CAPABILITY", "MATURITY", "COMPILED", "FEATURE"
    );
    for r in fraiseql_server::capability::REGISTRY {
        let maturity = serde_json::to_value(r.meta.maturity)?;
        println!(
            "{:<20} {:<8} {:<9} {:<20} {}",
            r.meta.name,
            maturity.as_str().unwrap_or_default(),
            if r.compiled_in { "yes" } else { "no" },
            r.meta.feature,
            r.meta.docs.unwrap_or("-"),
        );
    }
    Ok(())
}
