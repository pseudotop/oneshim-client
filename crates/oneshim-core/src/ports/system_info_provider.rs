//! Static system information provider port.

use crate::models::system::StaticSystemInfo;

/// Provides static system hardware/software information.
pub trait SystemInfoProvider: Send + Sync {
    /// Collect current system information.
    fn system_info(&self) -> StaticSystemInfo;
}
