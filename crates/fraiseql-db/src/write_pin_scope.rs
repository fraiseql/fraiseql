//! The principal a read or a write is made for, so read-your-writes pins can be kept per
//! principal: a write pins its writer's reads to the primary, not everyone's.

use std::{
    future::Future,
    hash::{DefaultHasher, Hash, Hasher},
};

tokio::task_local! {
    static WRITE_PIN_PRINCIPAL: u64;
}

/// Run `future` as the work of `principal`, so its writes pin only `principal`'s reads.
///
/// With read replicas, the writes `future` makes pin `principal`'s reads to the primary
/// for the pin window, and its reads are pinned only by `principal`'s writes and by
/// writes made outside any principal's scope. Outside a scope, every write pins every
/// read, as without this.
///
/// `principal` names who is reading and writing, the same for every request of theirs,
/// for example a tenant and a subject. The hash is keyed the same in every process, so
/// two principals that hash alike always do; they share a pin, which only sends more
/// reads to the primary. Work spawned onto another task leaves the scope,
/// and is pinned by any write.
pub async fn with_write_pin_principal<F: Future>(principal: &str, future: F) -> F::Output {
    let mut hasher = DefaultHasher::new();
    principal.hash(&mut hasher);
    WRITE_PIN_PRINCIPAL.scope(hasher.finish(), future).await
}

/// The key of the principal whose scope the current task runs in; `None` outside one.
///
/// The scope is opened by [`with_write_pin_principal`]; an adapter that keeps its own
/// read-your-writes pins reads the principal here.
#[must_use]
pub fn scoped_write_pin_principal() -> Option<u64> {
    WRITE_PIN_PRINCIPAL.try_with(|principal| *principal).ok()
}
