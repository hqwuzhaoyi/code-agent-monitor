//! 具体渠道实现

pub mod telegram;
pub mod dashboard;
pub mod whatsapp;
pub mod openclaw_message;

pub use telegram::TelegramChannel;
pub use dashboard::DashboardChannel;
pub use whatsapp::WhatsAppChannel;
pub use openclaw_message::OpenclawMessageChannel;
