//! Click events flowing browser → `POST /events` → host mailbox (M2).
//!
//! The wire shape is flat (`{"type":"select","screen":1,"option_id":"a"}`);
//! `seq` and `at_ms` are server-assigned on arrival and persisted in `events`.

use serde::{Deserialize, Serialize};

/// What the user did on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventAction {
    /// Click an option. Multiselect screens toggle; single-select commits at
    /// the host boundary (M2b — synthesis lives in the host, not here).
    Select { option_id: String },
    /// Multiselect "confirm" button.
    Confirm,
    /// "Skip / pick nothing" button. Always present when options exist.
    Skip,
}

/// A persisted click record (one JSONL line in `events`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasEvent {
    pub seq: u64,
    pub screen: u64,
    pub at_ms: u64,
    pub action: EventAction,
}

/// Body accepted by `POST /events`, before server-side bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EventInput {
    pub screen: u64,
    #[serde(flatten)]
    pub action: EventAction,
}

impl EventInput {
    /// Turn an accepted wire event into a persisted record.
    pub fn into_event(self, seq: u64, at_ms: u64) -> CanvasEvent {
        CanvasEvent {
            seq,
            screen: self.screen,
            at_ms,
            action: self.action,
        }
    }
}
