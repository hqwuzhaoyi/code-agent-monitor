//! 会话管理 - Claude Code 会话和对话状态

pub mod manager;
pub mod state;

pub use manager::{SessionManager, SessionFilter};
pub use state::{ConversationStateManager, ConversationState, PendingConfirmation, ConfirmationType, AgentContext, ReplyResult, BatchFilter, BatchReplyResult};
