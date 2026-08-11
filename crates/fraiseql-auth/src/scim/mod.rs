//! SCIM 2.0 provisioning storage and credentials (#946).
//!
//! `#381`'s SAML slice covers *authentication*. Provisioning is the other half of what an
//! enterprise IdP integration means, and its security-load-bearing part is the end of the
//! lifecycle: without it an offboarded employee's account stays active for every credential
//! that is not SAML — a local password, a social link, an API key. SAML stops them signing
//! in *through the IdP* and nothing else does.
//!
//! # Where the deactivation actually bites
//!
//! `active = false` does two things, and both are necessary:
//!
//! 1. **Existing sessions are revoked** — otherwise someone offboarded at 09:00 keeps working until
//!    their refresh token expires.
//! 2. **New sessions are refused** — enforced in
//!    [`PostgresSessionStore::create_session`](crate::PostgresSessionStore), the one point every
//!    credential path converges on. Password login, the MFA second factor, a social callback, email
//!    or phone OTP, and the SAML ACS all end there, so none of them can quietly stay open.
//!
//! That is why SCIM users are `core.tb_user` rows rather than a parallel directory: the
//! account an IdP deactivates has to be the account a password would authenticate.
//!
//! The HTTP surface (`/scim/v2/...`, its schemas, filtering, pagination and `ETag`
//! concurrency) lives in `fraiseql-server`, which is where it can reach both this store and
//! the RBAC roles SCIM groups map onto.

mod store;
mod token;

pub use store::{
    PG_SCIM_SCHEMA_SQL, PgScimStore, ScimGroup, ScimPage, ScimStore, ScimUser, ScimUserWrite,
};
pub use token::{MintedScimToken, PgScimTokenStore, ScimPrincipal, ScimTokenRecord};
