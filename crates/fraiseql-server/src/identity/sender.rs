//! Send-path consumer (consumer B): the DB-backed [`SenderIdentityResolver`].
//!
//! Resolves `sub → verified from-address + mailbox` on the **shared** identity
//! primitive (the same `IdentityResolver` the read path uses), cached and
//! fail-closed. This replaces the login-email assumption where the sending
//! mailbox differs from the connected user's login email (DESIGN §4) — the pure
//! `LoginEmailSender` policy is the degenerate case this subsumes.
//!
//! This lands the resolver and the seam; the `send_email` host op that injects
//! and calls it — and the SMTP transport — are the hardening train's, not #539's.

use std::{collections::HashMap, future::Future, pin::Pin};

use fraiseql_functions::{SendPolicyError, SenderIdentity, SenderIdentityResolver};
use serde_json::Value;

use super::{failure::IdentityResolution, resolver::IdentityResolver};

/// The sender profile's DB-backed resolver: `sub → { verified from-address,
/// display name }` on the shared identity primitive.
pub struct DbSenderIdentityResolver {
    resolver:           IdentityResolver,
    /// The mapped field holding the verified from-address.
    address_field:      String,
    /// The mapped field holding the sender display name, if any.
    display_name_field: Option<String>,
}

impl DbSenderIdentityResolver {
    /// Wrap a sender-profile resolver with the field names it maps its verified
    /// sending identity onto.
    pub fn new(
        resolver: IdentityResolver,
        address_field: impl Into<String>,
        display_name_field: Option<String>,
    ) -> Self {
        Self {
            resolver,
            address_field: address_field.into(),
            display_name_field,
        }
    }
}

impl SenderIdentityResolver for DbSenderIdentityResolver {
    fn resolve_sender<'a>(
        &'a self,
        auth_context: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<SenderIdentity, SendPolicyError>> + Send + 'a>> {
        Box::pin(async move {
            let Some(sub) = auth_context.get("sub").and_then(Value::as_str) else {
                return Err(SendPolicyError::new(
                    "refusing to send: no subject in the authenticated context",
                ));
            };
            // The host auth context is the claim set — bind the sender query from it.
            let claims: HashMap<String, Value> =
                auth_context.as_object().map_or_else(HashMap::new, |map| {
                    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                });

            match self.resolver.resolve(sub, &claims).await {
                IdentityResolution::Resolved(fields) => {
                    let address = fields
                        .get(&self.address_field)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|v| v.contains('@') && !v.contains(char::is_whitespace))
                        .ok_or_else(|| {
                            SendPolicyError::new(
                                "refusing to send: the resolved sender identity has no usable \
                                 verified address",
                            )
                        })?
                        .to_owned();
                    let display_name = self
                        .display_name_field
                        .as_ref()
                        .and_then(|field| fields.get(field))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(ToOwned::to_owned);
                    Ok(SenderIdentity {
                        address,
                        display_name,
                    })
                },
                // Fail-closed, uniform with the read path (DESIGN §5): never fall
                // back to a shared mailbox.
                IdentityResolution::Denied(_) => Err(SendPolicyError::new(
                    "refusing to send: no verified sending identity for the connected user",
                )),
                IdentityResolution::Unavailable(_) => {
                    Err(SendPolicyError::new("sender identity resolution temporarily unavailable"))
                },
            }
        })
    }
}
