//! Process-wide id sequences. Restoring a workspace reserves the saved ids so
//! new rows, requests, and groups never collide with them.

use std::sync::atomic::{AtomicU64, Ordering};

pub struct Sequence(AtomicU64);

impl Sequence {
    pub const fn new() -> Self {
        Sequence(AtomicU64::new(0))
    }

    pub fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Make later ids larger than `id`.
    pub fn reserve(&self, id: u64) {
        self.0.fetch_max(id, Ordering::Relaxed);
    }
}

impl Default for Sequence {
    fn default() -> Self {
        Sequence::new()
    }
}

/// Query, header, and form rows.
pub static PAIRS: Sequence = Sequence::new();
/// Requests (sessions).
pub static SESSIONS: Sequence = Sequence::new();
/// Browser groups.
pub static GROUPS: Sequence = Sequence::new();
/// Group environments.
pub static ENVIRONMENTS: Sequence = Sequence::new();
/// Assertions and captures.
pub static CHECKS: Sequence = Sequence::new();
/// History entries.
pub static HISTORY: Sequence = Sequence::new();
/// WebSocket log messages.
pub static SOCKET_MESSAGES: Sequence = Sequence::new();
