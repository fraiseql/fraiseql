//! Tests for the DLQ CLI subcommands (#341).
//!
//! The CLI must never fabricate data: until the HTTP client lands every
//! subcommand fails loud, and once wired a missing endpoint / unreachable
//! server surfaces as an error rather than being reported as success.
#![allow(clippy::unwrap_used)] // Reason: test code; failures should panic to surface bugs.

use super::execute;
use crate::cli::{DlqSubcommand, OutputFormat};

#[tokio::test]
async fn list_does_not_fabricate() {
    let sub = DlqSubcommand::List {
        limit:    10,
        offset:   None,
        observer: None,
        after:    None,
    };
    assert!(
        execute(OutputFormat::Json, sub).await.is_err(),
        "dlq list must not return fabricated data"
    );
}

#[tokio::test]
async fn show_does_not_fabricate() {
    let sub = DlqSubcommand::Show {
        item_id: "dlq-001".to_string(),
    };
    assert!(execute(OutputFormat::Json, sub).await.is_err());
}

#[tokio::test]
async fn retry_does_not_fabricate() {
    let sub = DlqSubcommand::Retry {
        item_id: "dlq-001".to_string(),
        force:   false,
    };
    assert!(execute(OutputFormat::Json, sub).await.is_err());
}

#[tokio::test]
async fn retry_all_does_not_fabricate() {
    let sub = DlqSubcommand::RetryAll {
        observer: None,
        after:    None,
        dry_run:  false,
    };
    assert!(execute(OutputFormat::Json, sub).await.is_err());
}

#[tokio::test]
async fn remove_does_not_fabricate() {
    let sub = DlqSubcommand::Remove {
        item_id: "dlq-001".to_string(),
        force:   true,
    };
    assert!(execute(OutputFormat::Json, sub).await.is_err());
}

#[tokio::test]
async fn stats_does_not_fabricate() {
    let sub = DlqSubcommand::Stats {
        by_observer: true,
        by_error:    true,
    };
    assert!(execute(OutputFormat::Json, sub).await.is_err());
}
