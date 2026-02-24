//! 具体渠道实现

pub mod dashboard;
pub mod local_file;

pub use dashboard::DashboardChannel;
pub use local_file::LocalFileChannel;
