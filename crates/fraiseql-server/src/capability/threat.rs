//! What each capability guards, and the inputs it must refuse.
//!
//! Every [`Registration`](super::Registration) carries a [`ThreatModel`]: the assets it
//! touches, the trust boundaries its inputs cross, and each input it refuses. A refusal is
//! not prose: it names the hostile fixture under `tests/capabilities/` that sends the input
//! and shows it refused with nothing following, and the definition-of-shipped gate
//! (`threat_tests`) holds every refusal and every hostile fixture to each other.

/// What a capability guards, and what it must refuse.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))] // Reason: read by the definition-of-shipped gate, a test
pub struct ThreatModel {
    /// What it reads, writes or holds that a caller it does not trust must not reach.
    pub assets:  &'static [&'static str],
    /// The trust boundaries its inputs cross on the way in.
    pub crosses: &'static [&'static str],
    /// Each input it must refuse, with the hostile fixture that sends it.
    pub refuses: &'static [Refusal],
}

/// One input a capability must refuse, and the fixture that shows it refused.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))] // Reason: read by the definition-of-shipped gate, a test
pub struct Refusal {
    /// The input, in a few words.
    pub input:   &'static str,
    /// The directory under `tests/capabilities/` whose fixture sends it, with
    /// `role = "hostile"`.
    pub fixture: &'static str,
}
