//! Dead letter queue management commands.
//!
//! These subcommands operate against a running server's observer admin API
//! (`/api/observers/dlq`). They never fabricate data: until the HTTP client is
//! wired in they fail loud, and once wired a call to a missing endpoint or an
//! unreachable server surfaces as an error rather than being reported as
//! success (#341).

use crate::{
    cli::DlqSubcommand,
    error::{ObserverError, Result},
};

/// Execute DLQ subcommands.
///
/// # Errors
///
/// Returns an error describing how to reach the real data — these commands do
/// not (and must not) return fabricated results.
#[allow(clippy::unused_async)] // Reason: the HTTP client wired in #341 Cycle 3 makes this genuinely async.
pub async fn execute(_format: crate::cli::OutputFormat, subcommand: DlqSubcommand) -> Result<()> {
    let op = match subcommand {
        DlqSubcommand::List { .. } => "list",
        DlqSubcommand::Show { .. } => "show",
        DlqSubcommand::Retry { .. } => "retry",
        DlqSubcommand::RetryAll { .. } => "retry-all",
        DlqSubcommand::Remove { .. } => "remove",
        DlqSubcommand::Stats { .. } => "stats",
    };
    Err(ObserverError::DlqError {
        reason: format!(
            "`dlq {op}` is not yet wired to the server admin API. Query the running server's \
             observer DLQ endpoints under /api/observers/dlq instead — this command will not \
             return fabricated data"
        ),
    })
}

#[cfg(test)]
mod tests;
