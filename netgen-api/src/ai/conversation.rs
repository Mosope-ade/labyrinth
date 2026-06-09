use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai::client::{ChatMessage, MessageRole};

// ─── Phase ───────────────────────────────────────────────────────────────────

/// The three phases of a Labyrinth design conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignPhase {
    /// AI asks questions; intent accumulates across turns. AI decides when to transition.
    Gathering,
    /// Intent is complete — engine is running, topology is appearing.
    Building,
    /// Design exists — user changes are parsed as delta intents and regenerate outputs.
    Refining,
}

// ─── DesignFlag ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagSeverity {
    Info,
    Warning,
    Error,
}

/// An issue or observation the AI has flagged about the design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignFlag {
    pub code: String,
    pub message: String,
    pub severity: FlagSeverity,
}

// ─── ConversationState ────────────────────────────────────────────────────────

/// Full state of one design conversation — persisted per session.
/// Accumulated across all turns; passed into every AI call as message history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub session_id: Uuid,
    pub phase: DesignPhase,
    pub messages: Vec<ChatMessage>,
    /// Raw partial intent JSON accumulated across Gathering turns.
    pub partial_intent: Option<serde_json::Value>,
    pub flags: Vec<DesignFlag>,
    /// Number of intent-parse retries attempted for the current building attempt.
    /// Resets to 0 after a successful parse. Max 1 per spec.
    pub retry_count: u8,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ConversationState {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            session_id: Uuid::new_v4(),
            phase: DesignPhase::Gathering,
            messages: Vec::new(),
            partial_intent: None,
            flags: Vec::new(),
            retry_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Appends a message and bumps the updated timestamp.
    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.updated_at = Utc::now();
    }

    /// Advances to the next phase. Returns `false` if already at `Refining`.
    pub fn advance_phase(&mut self) -> bool {
        match self.phase {
            DesignPhase::Gathering => {
                self.phase = DesignPhase::Building;
                self.updated_at = Utc::now();
                true
            }
            DesignPhase::Building => {
                self.phase = DesignPhase::Refining;
                self.updated_at = Utc::now();
                true
            }
            DesignPhase::Refining => false,
        }
    }

    /// Returns `true` if the one-retry budget has been consumed.
    pub fn retry_exhausted(&self) -> bool {
        self.retry_count >= 1
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
        self.updated_at = Utc::now();
    }

    pub fn reset_retry(&mut self) {
        self.retry_count = 0;
    }

    /// Adds a design flag (issue the AI has noticed during conversation).
    pub fn add_flag(
        &mut self,
        code: impl Into<String>,
        message: impl Into<String>,
        severity: FlagSeverity,
    ) {
        self.flags.push(DesignFlag {
            code: code.into(),
            message: message.into(),
            severity,
        });
        self.updated_at = Utc::now();
    }

    /// All turns excluding the system prompt — the user-visible conversation.
    pub fn user_turns(&self) -> Vec<&ChatMessage> {
        self.messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .collect()
    }

    /// Concatenated text of all user messages — used for industry detection.
    pub fn user_text(&self) -> String {
        self.messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn turn_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .count()
    }
}

impl Default for ConversationState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::client::ChatMessage;

    #[test]
    fn new_state_starts_in_gathering() {
        let s = ConversationState::new();
        assert_eq!(s.phase, DesignPhase::Gathering);
        assert_eq!(s.retry_count, 0);
        assert!(s.messages.is_empty());
    }

    #[test]
    fn advance_phase_gathering_to_building() {
        let mut s = ConversationState::new();
        assert!(s.advance_phase());
        assert_eq!(s.phase, DesignPhase::Building);
    }

    #[test]
    fn advance_phase_building_to_refining() {
        let mut s = ConversationState::new();
        s.advance_phase();
        assert!(s.advance_phase());
        assert_eq!(s.phase, DesignPhase::Refining);
    }

    #[test]
    fn advance_phase_refining_returns_false() {
        let mut s = ConversationState::new();
        s.advance_phase();
        s.advance_phase();
        assert!(!s.advance_phase());
        assert_eq!(s.phase, DesignPhase::Refining);
    }

    #[test]
    fn retry_exhausted_after_one_increment() {
        let mut s = ConversationState::new();
        assert!(!s.retry_exhausted());
        s.increment_retry();
        assert!(s.retry_exhausted());
    }

    #[test]
    fn reset_retry_clears_counter() {
        let mut s = ConversationState::new();
        s.increment_retry();
        s.reset_retry();
        assert!(!s.retry_exhausted());
    }

    #[test]
    fn push_message_appends_and_bumps_timestamp() {
        let mut s = ConversationState::new();
        let before = s.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        s.push_message(ChatMessage::user("hello"));
        assert_eq!(s.messages.len(), 1);
        assert!(s.updated_at >= before);
    }

    #[test]
    fn user_turns_excludes_system_messages() {
        let mut s = ConversationState::new();
        s.push_message(ChatMessage::system("You are..."));
        s.push_message(ChatMessage::user("design a network"));
        s.push_message(ChatMessage::assistant("Sure!"));
        s.push_message(ChatMessage::user("add DMZ"));
        let turns = s.user_turns();
        // system excluded → 1 user + 1 assistant + 1 user = 3 non-system messages
        assert_eq!(turns.len(), 3);
        assert!(turns.iter().all(|m| m.role == MessageRole::User || m.role == MessageRole::Assistant));
    }

    #[test]
    fn user_text_joins_user_messages() {
        let mut s = ConversationState::new();
        s.push_message(ChatMessage::system("ignored"));
        s.push_message(ChatMessage::user("hospital"));
        s.push_message(ChatMessage::assistant("ok"));
        s.push_message(ChatMessage::user("ICU network"));
        let text = s.user_text();
        assert!(text.contains("hospital"));
        assert!(text.contains("ICU network"));
        assert!(!text.contains("ignored"));
    }

    #[test]
    fn turn_count_counts_only_user_messages() {
        let mut s = ConversationState::new();
        s.push_message(ChatMessage::system("sys"));
        s.push_message(ChatMessage::user("msg1"));
        s.push_message(ChatMessage::assistant("reply"));
        s.push_message(ChatMessage::user("msg2"));
        assert_eq!(s.turn_count(), 2);
    }

    #[test]
    fn add_flag_appends_to_flags() {
        let mut s = ConversationState::new();
        s.add_flag("SINGLE_POINT", "No redundancy detected", FlagSeverity::Warning);
        assert_eq!(s.flags.len(), 1);
        assert_eq!(s.flags[0].code, "SINGLE_POINT");
    }
}
