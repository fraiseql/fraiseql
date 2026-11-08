//! Capturing what a test's code logs, to check what reaches the log.

use std::sync::{Arc, Mutex};

/// A writer the capturing subscriber appends to.
#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    /// Everything logged so far.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
            .into_owned()
    }
}

/// Capture every event at TRACE and above on this thread until the guard drops.
pub fn capture() -> (Captured, tracing::subscriber::DefaultGuard) {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let guard = fraiseql_test_support::tracing_capture::set_default(subscriber);
    (captured, guard)
}

/// `url` with `user:password@` userinfo inserted after its scheme.
#[cfg(any(
    feature = "observers-cache",
    feature = "redis-pkce",
    feature = "redis-rate-limiting"
))]
pub fn with_userinfo(url: &str, user: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("a URL with a scheme");
    format!("{scheme}://{user}:{password}@{rest}")
}
