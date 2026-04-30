//! Room-based presence tracking for realtime member awareness.
//!
//! Clients join a "room" with an initial state payload.  The server tracks
//! membership and emits `PRESENCE_STATE` (full roster on join) and
//! `PRESENCE_DIFF` (join/leave/update deltas) events.
//!
//! All state is in-memory — lost on server restart (acceptable for v1).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;

// ──────────────────────── Configuration ────────────────────────

/// Configuration for the presence subsystem.
#[derive(Debug, Clone)]
pub struct PresenceConfig {
    /// Maximum members per room (prevents memory abuse).
    pub max_members_per_room: usize,

    /// Maximum number of rooms that can exist simultaneously.
    pub max_rooms: usize,

    /// Heartbeat timeout — members are evicted after this duration without a ping.
    pub heartbeat_timeout: Duration,
}

impl PresenceConfig {
    /// Create config with production defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_members_per_room: 500,
            max_rooms: 10_000,
            heartbeat_timeout: Duration::from_secs(30),
        }
    }
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────── Types ────────────────────────

/// A member's presence in a room.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceMember {
    /// Unique identifier for this member (typically connection ID or user ID).
    pub id: String,

    /// Arbitrary JSON state (e.g., cursor position, status, avatar).
    pub state: serde_json::Value,

    /// When this member last sent a heartbeat.
    #[serde(skip, default = "Instant::now")]
    pub last_seen: Instant,
}

/// Full room state — sent to a client on join (`PRESENCE_STATE`).
#[derive(Debug, Clone, Serialize)]
pub struct PresenceState {
    /// Room name.
    pub room: String,

    /// All current members.
    pub members: Vec<PresenceMember>,
}

/// Delta event — sent when members join, leave, or update (`PRESENCE_DIFF`).
#[derive(Debug, Clone, Serialize)]
pub struct PresenceDiff {
    /// Room name.
    pub room: String,

    /// Members who joined.
    pub joins: Vec<PresenceMember>,

    /// Member IDs who left (or were evicted).
    pub leaves: Vec<String>,
}

// ──────────────────────── Room ────────────────────────

/// A single presence room.
#[derive(Debug)]
struct PresenceRoom {
    /// Members indexed by their ID.
    members: HashMap<String, PresenceMember>,
}

impl PresenceRoom {
    fn new() -> Self {
        Self {
            members: HashMap::new(),
        }
    }
}

// ──────────────────────── Manager ────────────────────────

/// Statistics for the presence subsystem.
#[derive(Debug, Clone)]
pub struct PresenceStats {
    /// Total rooms currently tracked.
    pub active_rooms: usize,

    /// Total members across all rooms.
    pub total_members: usize,

    /// Total join events processed.
    pub joins_total: u64,

    /// Total leave events processed.
    pub leaves_total: u64,

    /// Total heartbeat evictions.
    pub evictions_total: u64,
}

/// Manages room-based presence state.
///
/// Thread-safe via `RwLock` for the room map and atomics for counters.
#[derive(Debug)]
pub struct PresenceManager {
    rooms: RwLock<HashMap<String, PresenceRoom>>,
    config: PresenceConfig,
    joins_total: AtomicU64,
    leaves_total: AtomicU64,
    evictions_total: AtomicU64,
}

impl PresenceManager {
    /// Create a new presence manager.
    #[must_use]
    pub fn new(config: PresenceConfig) -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
            config,
            joins_total: AtomicU64::new(0),
            leaves_total: AtomicU64::new(0),
            evictions_total: AtomicU64::new(0),
        }
    }

    /// Join a room with initial state.
    ///
    /// Returns `PresenceState` (current members including the new one) and
    /// a `PresenceDiff` (announcing the join) for broadcasting.
    ///
    /// # Errors
    ///
    /// Returns error if the room is full or room limit is exceeded.
    pub async fn join(
        &self,
        room: &str,
        member_id: &str,
        state: serde_json::Value,
    ) -> Result<(PresenceState, PresenceDiff), PresenceError> {
        let mut rooms = self.rooms.write().await;

        // Create room if needed
        if !rooms.contains_key(room) {
            if rooms.len() >= self.config.max_rooms {
                return Err(PresenceError::TooManyRooms {
                    max: self.config.max_rooms,
                });
            }
            rooms.insert(room.to_string(), PresenceRoom::new());
        }

        let Some(presence_room) = rooms.get_mut(room) else {
            // Unreachable: we just inserted the room above if it was missing.
            return Err(PresenceError::TooManyRooms {
                max: self.config.max_rooms,
            });
        };

        // Check room capacity (only if this is a new member, not a rejoin)
        if !presence_room.members.contains_key(member_id)
            && presence_room.members.len() >= self.config.max_members_per_room
        {
            return Err(PresenceError::RoomFull {
                room: room.to_string(),
                max: self.config.max_members_per_room,
            });
        }

        let member = PresenceMember {
            id: member_id.to_string(),
            state,
            last_seen: Instant::now(),
        };

        presence_room.members.insert(member_id.to_string(), member.clone());
        self.joins_total.fetch_add(1, Ordering::Relaxed);

        let presence_state = PresenceState {
            room: room.to_string(),
            members: presence_room.members.values().cloned().collect(),
        };

        let diff = PresenceDiff {
            room: room.to_string(),
            joins: vec![member],
            leaves: vec![],
        };

        debug!(room, member_id, members = presence_room.members.len(), "presence: member joined");
        Ok((presence_state, diff))
    }

    /// Leave a room.
    ///
    /// Returns a `PresenceDiff` for broadcasting, or `None` if the member
    /// wasn't in the room.
    pub async fn leave(&self, room: &str, member_id: &str) -> Option<PresenceDiff> {
        let mut rooms = self.rooms.write().await;

        let presence_room = rooms.get_mut(room)?;
        presence_room.members.remove(member_id)?;
        self.leaves_total.fetch_add(1, Ordering::Relaxed);

        debug!(room, member_id, members = presence_room.members.len(), "presence: member left");

        let diff = PresenceDiff {
            room: room.to_string(),
            joins: vec![],
            leaves: vec![member_id.to_string()],
        };

        // Clean up empty rooms
        if presence_room.members.is_empty() {
            rooms.remove(room);
            debug!(room, "presence: room removed (empty)");
        }

        Some(diff)
    }

    /// Record a heartbeat for a member, resetting their eviction timer.
    ///
    /// Returns `true` if the heartbeat was accepted (member exists in room).
    pub async fn heartbeat(&self, room: &str, member_id: &str) -> bool {
        let mut rooms = self.rooms.write().await;

        if let Some(presence_room) = rooms.get_mut(room) {
            if let Some(member) = presence_room.members.get_mut(member_id) {
                member.last_seen = Instant::now();
                return true;
            }
        }

        false
    }

    /// Update a member's state payload.
    ///
    /// Returns a `PresenceDiff` with the updated member in `joins` (same
    /// semantics as Supabase Realtime — updates appear as joins).
    pub async fn update_state(
        &self,
        room: &str,
        member_id: &str,
        new_state: serde_json::Value,
    ) -> Option<PresenceDiff> {
        let mut rooms = self.rooms.write().await;
        let presence_room = rooms.get_mut(room)?;
        let member = presence_room.members.get_mut(member_id)?;

        member.state = new_state;
        member.last_seen = Instant::now();

        Some(PresenceDiff {
            room: room.to_string(),
            joins: vec![member.clone()],
            leaves: vec![],
        })
    }

    /// Evict members whose heartbeat has expired.
    ///
    /// Returns `PresenceDiff` events for each room that had evictions.
    pub async fn evict_stale(&self) -> Vec<PresenceDiff> {
        let timeout = self.config.heartbeat_timeout;
        let mut rooms = self.rooms.write().await;
        let mut diffs = Vec::new();
        let mut empty_rooms = Vec::new();

        for (room_name, room) in rooms.iter_mut() {
            let mut evicted = Vec::new();

            room.members.retain(|id, member| {
                if member.last_seen.elapsed() > timeout {
                    evicted.push(id.clone());
                    false
                } else {
                    true
                }
            });

            if !evicted.is_empty() {
                let count = evicted.len();
                self.evictions_total
                    .fetch_add(count as u64, Ordering::Relaxed);
                debug!(room = %room_name, evicted = count, "presence: evicted stale members");

                diffs.push(PresenceDiff {
                    room: room_name.clone(),
                    joins: vec![],
                    leaves: evicted,
                });
            }

            if room.members.is_empty() {
                empty_rooms.push(room_name.clone());
            }
        }

        for room_name in empty_rooms {
            rooms.remove(&room_name);
        }

        diffs
    }

    /// Get current members of a room.
    pub async fn get_room(&self, room: &str) -> Option<PresenceState> {
        let rooms = self.rooms.read().await;
        let presence_room = rooms.get(room)?;

        Some(PresenceState {
            room: room.to_string(),
            members: presence_room.members.values().cloned().collect(),
        })
    }

    /// Get statistics.
    pub async fn stats(&self) -> PresenceStats {
        let rooms = self.rooms.read().await;
        let total_members: usize = rooms.values().map(|r| r.members.len()).sum();

        PresenceStats {
            active_rooms: rooms.len(),
            total_members,
            joins_total: self.joins_total.load(Ordering::Relaxed),
            leaves_total: self.leaves_total.load(Ordering::Relaxed),
            evictions_total: self.evictions_total.load(Ordering::Relaxed),
        }
    }
}

/// Errors from presence operations.
#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    /// Room has reached its member cap.
    #[error("room '{room}' is full: max {max} members")]
    RoomFull {
        /// Room name.
        room: String,
        /// Maximum members.
        max: usize,
    },

    /// Too many rooms exist.
    #[error("room limit exceeded: max {max} rooms")]
    TooManyRooms {
        /// Maximum rooms.
        max: usize,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    fn default_manager() -> PresenceManager {
        PresenceManager::new(PresenceConfig::new())
    }

    #[tokio::test]
    async fn test_join_returns_state_and_diff() {
        let mgr = default_manager();

        let (state, diff) = mgr
            .join("room1", "alice", serde_json::json!({"status": "online"}))
            .await
            .unwrap();

        assert_eq!(state.room, "room1");
        assert_eq!(state.members.len(), 1);
        assert_eq!(state.members[0].id, "alice");
        assert_eq!(diff.joins.len(), 1);
        assert!(diff.leaves.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_members_in_room() {
        let mgr = default_manager();

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        let (state, diff) = mgr
            .join("room1", "bob", serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(state.members.len(), 2);
        assert_eq!(diff.joins.len(), 1);
        assert_eq!(diff.joins[0].id, "bob");
    }

    #[tokio::test]
    async fn test_leave_returns_diff() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();

        let diff = mgr.leave("room1", "alice").await.unwrap();
        assert_eq!(diff.leaves, vec!["alice"]);
        assert!(diff.joins.is_empty());

        // Room should be cleaned up
        assert!(mgr.get_room("room1").await.is_none());
    }

    #[tokio::test]
    async fn test_leave_nonexistent_returns_none() {
        let mgr = default_manager();
        assert!(mgr.leave("room1", "alice").await.is_none());
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();

        assert!(mgr.heartbeat("room1", "alice").await);
        assert!(!mgr.heartbeat("room1", "nobody").await);
        assert!(!mgr.heartbeat("noroom", "alice").await);
    }

    #[tokio::test]
    async fn test_update_state() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({"status": "online"}))
            .await
            .unwrap();

        let diff = mgr
            .update_state("room1", "alice", serde_json::json!({"status": "away"}))
            .await
            .unwrap();

        assert_eq!(diff.joins.len(), 1);
        assert_eq!(diff.joins[0].state["status"], "away");

        let state = mgr.get_room("room1").await.unwrap();
        assert_eq!(state.members[0].state["status"], "away");
    }

    #[tokio::test]
    async fn test_evict_stale_members() {
        let config = PresenceConfig {
            heartbeat_timeout: Duration::from_millis(1),
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

        // Wait for heartbeat to expire
        tokio::time::sleep(Duration::from_millis(10)).await;

        let diffs = mgr.evict_stale().await;
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].leaves.len(), 2);

        // Room should be cleaned up
        assert!(mgr.get_room("room1").await.is_none());
    }

    #[tokio::test]
    async fn test_heartbeat_prevents_eviction() {
        let config = PresenceConfig {
            heartbeat_timeout: Duration::from_millis(50),
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

        // Wait a bit, then heartbeat only alice
        tokio::time::sleep(Duration::from_millis(30)).await;
        mgr.heartbeat("room1", "alice").await;

        // Wait for bob to expire but not alice
        tokio::time::sleep(Duration::from_millis(30)).await;

        let diffs = mgr.evict_stale().await;
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].leaves, vec!["bob"]);

        // Alice should still be in the room
        let state = mgr.get_room("room1").await.unwrap();
        assert_eq!(state.members.len(), 1);
        assert_eq!(state.members[0].id, "alice");
    }

    #[tokio::test]
    async fn test_room_full() {
        let config = PresenceConfig {
            max_members_per_room: 2,
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();

        let result = mgr.join("room1", "charlie", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_too_many_rooms() {
        let config = PresenceConfig {
            max_rooms: 2,
            ..PresenceConfig::new()
        };
        let mgr = PresenceManager::new(config);

        mgr.join("room1", "a", serde_json::json!({})).await.unwrap();
        mgr.join("room2", "b", serde_json::json!({})).await.unwrap();

        let result = mgr.join("room3", "c", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejoin_same_member_updates_state() {
        let mgr = default_manager();

        mgr.join("room1", "alice", serde_json::json!({"v": 1})).await.unwrap();
        let (state, _) = mgr
            .join("room1", "alice", serde_json::json!({"v": 2}))
            .await
            .unwrap();

        // Should still be 1 member, not 2
        assert_eq!(state.members.len(), 1);
        assert_eq!(state.members[0].state["v"], 2);
    }

    #[tokio::test]
    async fn test_stats() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room1", "bob", serde_json::json!({})).await.unwrap();
        mgr.join("room2", "charlie", serde_json::json!({})).await.unwrap();
        mgr.leave("room1", "alice").await;

        let stats = mgr.stats().await;
        assert_eq!(stats.active_rooms, 2);
        assert_eq!(stats.total_members, 2);
        assert_eq!(stats.joins_total, 3);
        assert_eq!(stats.leaves_total, 1);
    }

    #[tokio::test]
    async fn test_room_isolation() {
        let mgr = default_manager();
        mgr.join("room1", "alice", serde_json::json!({})).await.unwrap();
        mgr.join("room2", "bob", serde_json::json!({})).await.unwrap();

        let state1 = mgr.get_room("room1").await.unwrap();
        let state2 = mgr.get_room("room2").await.unwrap();

        assert_eq!(state1.members.len(), 1);
        assert_eq!(state1.members[0].id, "alice");
        assert_eq!(state2.members.len(), 1);
        assert_eq!(state2.members[0].id, "bob");
    }
}
