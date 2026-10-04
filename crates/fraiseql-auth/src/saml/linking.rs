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
/// Returns `true` only when the operator opted this IdP in (`trust_asserted_email = true`).
///
/// # Why a tenant-bound IdP may merge (#1088)
///
/// A SAML IdP only has authority over *its own* tenant's users; "Okta is trusted" is
/// meaningless globally, because every Okta tenant asserts whatever its admin configured.
/// The merge is safe because it cannot leave that authority: the ACS passes the IdP's
/// `tenant_id` as the account space, and the account store confines every lookup to it.
/// A verified assertion for `victim@x.com` from tenant A's IdP can reach only tenant A's
/// accounts, never a platform account or another tenant's (the nOAuth class). An untenanted
/// IdP's space is the platform, as before.
///
/// Until #1088 the store keyed email globally. This function then refused every
/// tenant-bound IdP, and the flag was inert for the whole population #947's IdP store
/// serves.
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
    idp.trust_asserted_email
}
