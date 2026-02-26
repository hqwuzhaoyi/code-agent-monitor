//! 会话管理 - Claude Code 会话和对话状态

pub mod manager;
pub mod state;

pub use manager::{SessionFilter, SessionManager};
pub use state::{
    AgentContext, BatchFilter, BatchReplyResult, ConfirmationType, ConversationState,
    ConversationStateManager, PendingConfirmation, ReplyResult,
};
