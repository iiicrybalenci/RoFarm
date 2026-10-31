#![deny(unsafe_op_in_unsafe_fn)]

pub mod error;
pub mod input;
pub mod job;
pub mod model;
pub mod platform;
pub mod process;
pub mod system;
pub mod watcher;
pub mod window;

pub use error::{PlatformError, PlatformErrorPulse, PlatformResult};
pub use job::{apply_job_limits, Job};
pub use model::*;
pub use platform::{MockPlatform, Platform, WindowsPlatform};
pub use process::{build_command_line, quote_arg};
pub use watcher::{AdaptivePoller, SharedPoller};
pub use window::DwmThumbnails;

pub fn default_poller() -> SharedPoller {
    std::sync::Arc::new(AdaptivePoller::new(
        std::time::Duration::from_secs(4),
        std::time::Duration::from_millis(500),
    ))
}
