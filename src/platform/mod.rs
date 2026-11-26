pub mod traits;

#[cfg(target_os = "windows")]
pub mod windows;

pub use traits::{HotkeyId, HotkeyManager, InputSender, MockPlatform, PlatformError};
