//! A `reqwest` resolver that runs the resolve-and-check on the client's own lookups.
//!
//! Checking a URL's host and then letting the client resolve it again connects to
//! whatever the second answer names: a record can change between the two. A client
//! built with [`PinnedResolver`] has no second answer. Every lookup it makes is
//! [`resolve_and_check`](super::resolve::resolve_and_check)'s, and it connects only to
//! the addresses that passed. A URL holding a literal address is not looked up; the
//! caller's URL check covers it.
//!
//! The pin holds only for a direct connection. Through a proxy the client looks up the
//! proxy and the proxy resolves the destination, so a pinned client is built with
//! `no_proxy()`.

use std::{future::Future, io, net::SocketAddr, pin::Pin, sync::Arc};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::resolve::{resolve_and_check_with, system_lookup};

/// A lookup: the host and port to the addresses it resolves to.
pub type Lookup = Arc<
    dyn Fn(String, u16) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send>>
        + Send
        + Sync,
>;

/// A [`Resolve`] whose every answer has passed the resolve-and-check.
///
/// ```
/// use std::sync::Arc;
///
/// use fraiseql_guard::net::pinned::PinnedResolver;
///
/// let client = reqwest::Client::builder()
///     .dns_resolver(Arc::new(PinnedResolver::new()))
///     .redirect(reqwest::redirect::Policy::none())
///     .no_proxy()
///     .build();
/// assert!(client.is_ok());
/// ```
pub struct PinnedResolver {
    lookup:    Option<Lookup>,
    unchecked: fn() -> bool,
}

impl PinnedResolver {
    /// A resolver that looks names up with the system resolver and checks every answer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lookup:    None,
            unchecked: || false,
        }
    }

    /// Skip the check, answering as the lookup does, whenever `unchecked` returns `true`
    /// at lookup time: for a development escape hatch decided per request.
    #[must_use]
    pub fn unchecked_when(mut self, unchecked: fn() -> bool) -> Self {
        self.unchecked = unchecked;
        self
    }

    /// Look names up with `lookup` instead of the system resolver: a test's stand-in for
    /// a DNS answer.
    #[must_use]
    pub fn with_lookup(mut self, lookup: Lookup) -> Self {
        self.lookup = Some(lookup);
        self
    }
}

impl Default for PinnedResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PinnedResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedResolver")
            .field("custom_lookup", &self.lookup.is_some())
            .field("unchecked", &self.unchecked)
            .finish()
    }
}

impl Resolve for PinnedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let lookup = self.lookup.clone();
        let unchecked = (self.unchecked)();
        Box::pin(async move {
            let host = name.as_str().to_owned();
            // The connector sets the URL's port on every address; the lookup's is unused.
            let looked_up = |host: String, port: u16| async move {
                match lookup {
                    Some(lookup) => lookup(host, port).await,
                    None => system_lookup(host, port).await,
                }
            };
            let addrs = if unchecked {
                looked_up(host, 0).await?
            } else {
                resolve_and_check_with(&host, 0, looked_up).await?
            };
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests;
