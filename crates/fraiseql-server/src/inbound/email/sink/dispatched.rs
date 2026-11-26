//! The post-commit dispatch tasks a sink spawned. Outside tests they run detached; a test
//! build keeps them, so a test can wait for a dispatch to settle rather than poll for it.

use tokio::task::JoinHandle;

/// Drops each handle it is given, leaving its task running detached.
#[cfg(not(test))]
pub(super) struct DispatchedTasks;

#[cfg(not(test))]
impl DispatchedTasks {
    pub(super) const fn new() -> Self {
        Self
    }

    // Reason: the same call shape as the test build's, which keeps the handles.
    #[allow(clippy::unused_self)]
    pub(super) fn keep(&self, tasks: Vec<JoinHandle<()>>) {
        drop(tasks);
    }
}

/// Keeps every handle it is given until a test takes them.
#[cfg(test)]
#[derive(Default)]
pub(super) struct DispatchedTasks(std::sync::Mutex<Vec<JoinHandle<()>>>);

#[cfg(test)]
impl DispatchedTasks {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn keep(&self, tasks: Vec<JoinHandle<()>>) {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).extend(tasks);
    }

    pub(super) fn take(&self) -> Vec<JoinHandle<()>> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}
