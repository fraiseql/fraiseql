//! The one authorization decision every Flight write path must make.
//!
//! A Flight upload names its own target table and inserts rows that never pass through
//! the mutation pipeline — no `SecurityContext` filtering, no cache invalidation, no
//! `core.tb_entity_change_log` outbox row. #953 gated that by requiring an operator to
//! allow-list each writable table, with `None` (the default) meaning "Upload is off".
//!
//! That gate went into `do_exchange` and not into `do_put`, which reached the same
//! capability by a different RPC: any caller with a valid Flight session could open
//! `DoPut` with `FlightDescriptor { path: ["tb_user"] }` and stream a batch straight into
//! an `INSERT`. Two doors to one capability, one of them guarded (#1028).
//!
//! So the check lives here, in a module neither handler owns, and both call it. A future
//! third write path that forgets to call it is a missing call to a function that visibly
//! exists — not an absence nobody can see.

use std::collections::HashSet;

use tracing::warn;

/// Decide whether `table` may be written by `user_id`.
///
/// `Err` carries the message to return to the client. The refusal deliberately names the
/// table and the fix: an operator who has not configured `with_upload_tables()` is far
/// more likely than an attacker probing table names, and the allow-list's contents are
/// operator configuration rather than a secret.
///
/// # Errors
///
/// Returns the refusal message when uploads are disabled entirely (`allowed_tables` is
/// `None`) or when `table` is not on the list.
pub(super) fn authorize_upload(
    allowed_tables: Option<&HashSet<String>>,
    user_id: &str,
    table: &str,
) -> Result<(), String> {
    let refusal = match allowed_tables {
        None => format!(
            "Upload is disabled, so table '{table}' cannot be written. Allow-list specific \
             tables with with_upload_tables()."
        ),
        Some(allowed) if !allowed.contains(table) => {
            format!("Upload is not permitted for table '{table}'.")
        },
        Some(_) => return Ok(()),
    };
    warn!(user_id, table = %table, "Refused Flight Upload: {}", refusal);
    Err(refusal)
}

#[cfg(test)]
mod tests;
