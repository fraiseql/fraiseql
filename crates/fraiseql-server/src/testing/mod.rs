//! Testing utilities for fraiseql-server

#[cfg(test)]
pub mod idp;
#[cfg(test)]
pub mod pg_schema;

/// The server's mount ledger, for the transport refusal matrix.
#[cfg(feature = "testing")]
#[doc(hidden)]
pub mod mount_ledger {
    pub use crate::server::mount_ledger::{Credential, MountLedger, MountRecord};
}
