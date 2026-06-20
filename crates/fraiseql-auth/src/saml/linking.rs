//! Tenant-bounded SAML email-trust policy and account-store key derivation (#381).
//!
//! These pure functions decide how a verified SAML assertion maps onto the existing
//! [`crate::account_linking::AccountStore`].

use super::SamlIdpConfig;

/// The account-store provider key for a SAML IdP: `"saml:<idp_name>"`.
///
/// SAML identities live in their own provider namespace. When email auto-linking is *not*
/// honored (the default), the store keys the identity on `(this, NameID)`, so a SAML login
/// never collapses into another provider's account.
#[must_use]
pub fn saml_provider_key(idp_name: &str) -> String {
    format!("saml:{idp_name}")
}

/// Whether a verified assertion's email may be used as a cross-provider auto-linking key.
///
/// This is the `email_verified` bool passed to
/// [`crate::account_linking::AccountStore::link_or_create_user`] for a SAML identity.
///
/// # Policy (fail-closed, #381 — opt-in per IdP, default OFF)
///
/// Returns `true` only when **both** hold:
///
/// 1. the operator opted this IdP in (`trust_asserted_email = true`), and
/// 2. the merge is provably bounded to a single tenant — i.e. no `tenant_id` is configured on the
///    IdP.
///
/// # Why the tenant clause is load-bearing
///
/// A SAML IdP only has authority over *its own* tenant's users; "Okta is trusted" is
/// meaningless globally because every Okta tenant asserts whatever its admin configured.
/// The v1 account store keys verified email **globally** (`email:<normalized>`) and cannot
/// restrict a merge to one tenant. So if an IdP is bound to a tenant (multi-tenant intent),
/// honoring its email here could merge a verified assertion for `victim@x.com` into a
/// *different* tenant's Google/Apple account that shares that address — cross-tenant
/// account takeover (the nOAuth class) reintroduced behind an opt-in switch. We therefore
/// fail closed for tenant-bound IdPs until per-tenant account scoping lands (the open #381
/// umbrella). In a single-tenant deployment (`tenant_id = None`) the global store *is* the
/// one tenant, so the opt-in merge is within-tenant and safe.
///
/// This function never registers the IdP into the global
/// [`crate::account_linking::TrustedEmailProviders`] set; SAML trust is computed here and
/// nowhere else.
///
/// # Both-sides safety
///
/// When this returns `true` the store merges on the `email:` key space, which by
/// construction holds only verified-trusted identities (local-password and phone sign-ups
/// pass `email_verified = false` and live in the `(provider, provider_id)` space — see
/// [`crate::account_linking`]). So an attacker-seeded unverified local account under the
/// victim's email is never absorbed by a later trusted SAML sign-in.
#[must_use]
pub const fn effective_saml_email_verified(idp: &SamlIdpConfig) -> bool {
    idp.trust_asserted_email && idp.tenant_id.is_none()
}
