//! Installing a `tracing` subscriber to capture what a test emits.
//!
//! Use these rather than `tracing::subscriber::{with_default, set_default}`: in a test
//! binary that runs tests in parallel, those can drop the very event a test captures.
//!
//! `tracing` caches each callsite's interest process-wide, computed when the callsite is
//! first reached. While exactly one dispatcher is registered, that computation asks the
//! *reaching thread's* default instead of the registered dispatchers. So with a test's
//! scoped subscriber the only one alive, another test's thread with no subscriber can reach
//! the same `warn!` first, cache `Interest::never` for it, and the capturing test then sees
//! nothing. Keeping a second dispatcher alive for the life of the process means the
//! registered dispatchers, the capture among them, are always the ones asked.

use std::sync::OnceLock;

use tracing::{
    Dispatch, Subscriber,
    subscriber::{DefaultGuard, NoSubscriber},
};

/// Run `f` with `subscriber` as this thread's default.
pub fn with_default<T>(
    subscriber: impl Subscriber + Send + Sync + 'static,
    f: impl FnOnce() -> T,
) -> T {
    keep_a_second_dispatcher_alive();
    #[allow(clippy::disallowed_methods)] // Reason: this is the wrapper the ban points to
    tracing::subscriber::with_default(subscriber, f)
}

/// Make `subscriber` this thread's default until the guard drops.
#[must_use = "the subscriber is uninstalled when the guard drops"]
pub fn set_default(subscriber: impl Subscriber + Send + Sync + 'static) -> DefaultGuard {
    keep_a_second_dispatcher_alive();
    #[allow(clippy::disallowed_methods)] // Reason: this is the wrapper the ban points to
    tracing::subscriber::set_default(subscriber)
}

/// Register a dispatcher that is never dropped, so a capture is never the only one.
fn keep_a_second_dispatcher_alive() {
    static KEEPALIVE: OnceLock<Dispatch> = OnceLock::new();
    KEEPALIVE.get_or_init(|| Dispatch::new(NoSubscriber::default()));
}
