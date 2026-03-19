//! Linux AT-SPI2 accessibility extractor.
//!
//! Extracts the currently focused UI element via the AT-SPI2 protocol over
//! D-Bus. AT-SPI2 (Assistive Technology Service Provider Interface) is the
//! standard accessibility framework on Linux desktops (GNOME, KDE, XFCE).
//!
//! ## Architecture
//!
//! The intended implementation flow:
//!
//! 1. Connect to the D-Bus session bus via the `atspi` crate
//!    (`atspi::AccessibilityBus::open()`)
//! 2. Query the accessibility registry for the currently focused application
//!    (`atspi::Registry::get_focused_accessible()`)
//! 3. From the focused `Accessible` object, extract:
//!    - **Role**: `accessible.get_role()` -> map `atspi::Role` to string
//!    - **Name**: `accessible.name()` -> accessibility label
//!    - **Bounding rect**: `accessible.get_extents(CoordType::Screen)` via the
//!      `Component` interface -> `ElementRect`
//!    - **Text value**: `accessible.get_text(0, -1)` via the `Text` interface
//!      -> filtered by PII level using `Zeroizing<String>`
//! 4. Apply PII level gating identical to the macOS/Windows implementations
//!
//! ## Dependencies (not yet added)
//!
//! When implementing the full version, add to `Cargo.toml`:
//! ```toml
//! [target.'cfg(target_os = "linux")'.dependencies]
//! atspi = "0.25"      # AT-SPI2 D-Bus bindings
//! zbus = "5"           # D-Bus connection (used by atspi internally)
//! ```
//!
//! ## Permissions
//!
//! Unlike macOS (which requires Accessibility permission) and some Windows
//! configurations, AT-SPI2 does not require special permissions. Any user
//! process can connect to the AT-SPI2 D-Bus service as long as:
//! - The `at-spi2-core` package is installed (default on most desktop distros)
//! - The AT-SPI2 bus is running (started by the desktop session manager)
//! - The `ATSPI_BUS_ADDRESS` or `DBUS_SESSION_BUS_ADDRESS` env var is set
//!
//! ## Current Status
//!
//! This is a structural stub that returns `Ok(None)`. The architecture is
//! documented above for the full implementation in a future phase.

#[cfg(target_os = "linux")]
mod inner {
    use std::sync::atomic::{AtomicU32, Ordering};

    use async_trait::async_trait;
    use tracing::{debug, warn};

    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::focused_element::FocusedElementInfo;
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    // ── Circuit breaker (same pattern as macOS/Windows) ──────────────

    /// Consecutive AT-SPI2 failures before the circuit breaker opens.
    static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);
    const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
    const CIRCUIT_BREAKER_RETRY_INTERVAL: u32 = 60;

    /// Linux AT-SPI2 accessibility extractor.
    ///
    /// Currently a structural stub. When implemented, it will connect to the
    /// AT-SPI2 D-Bus service to extract the focused UI element.
    pub struct LinuxAccessibility;

    impl Default for LinuxAccessibility {
        fn default() -> Self {
            Self
        }
    }

    impl LinuxAccessibility {
        pub fn new() -> Self {
            Self
        }

        // ── Circuit breaker ──────────────────────────────────────────

        fn circuit_allows() -> bool {
            let failures = CONSECUTIVE_FAILURES.load(Ordering::Relaxed);
            if failures >= CIRCUIT_BREAKER_THRESHOLD {
                if failures % CIRCUIT_BREAKER_RETRY_INTERVAL != 0 {
                    CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
                warn!(
                    "LinuxAccessibility: circuit breaker retry after {} skipped",
                    failures - CIRCUIT_BREAKER_THRESHOLD
                );
            }
            true
        }

        fn record_success() {
            CONSECUTIVE_FAILURES.store(0, Ordering::Relaxed);
        }

        fn record_failure() {
            CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed);
        }

        /// Check if the AT-SPI2 D-Bus service is reachable.
        ///
        /// In the full implementation this would attempt a lightweight D-Bus
        /// ping to `org.a11y.Bus`. For now, returns `true` since AT-SPI2
        /// does not require special permissions.
        fn check_atspi_available() -> bool {
            // TODO: Probe the AT-SPI2 bus via D-Bus
            // let connection = zbus::Connection::session().ok()?;
            // connection.call_method("org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus", "GetAddress", &())
            true
        }

        /// Extract focused element via AT-SPI2 (stub).
        ///
        /// Full implementation outline:
        /// ```ignore
        /// async fn extract_atspi() -> Option<RawFocusedElement> {
        ///     let bus = atspi::AccessibilityBus::open().await.ok()?;
        ///     let focused = bus.get_focused_accessible().await.ok()?;
        ///
        ///     let role = focused.get_role().await.ok()?;
        ///     let name = focused.name().await.ok()?;
        ///     let extents = focused.get_extents(CoordType::Screen).await.ok()?;
        ///
        ///     // Text interface (if supported by the element)
        ///     let text = if focused.supports_text() {
        ///         focused.get_text(0, -1).await.ok()
        ///     } else {
        ///         None
        ///     };
        ///
        ///     Some(RawFocusedElement { role, name, text, extents })
        /// }
        /// ```
        fn extract_raw() -> Option<FocusedElementInfo> {
            // Stub: AT-SPI2 D-Bus calls not yet implemented
            None
        }
    }

    #[async_trait]
    impl AccessibilityExtractor for LinuxAccessibility {
        async fn extract_focused_element(
            &self,
            _pii_level: PiiFilterLevel,
            _has_full_text_consent: bool,
        ) -> Result<Option<FocusedElementInfo>, CoreError> {
            if !Self::circuit_allows() {
                debug!("LinuxAccessibility: circuit breaker open");
                return Ok(None);
            }

            // Consent gating will be applied here once extract_raw is implemented:
            // let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
            //     PiiFilterLevel::Standard
            // } else {
            //     pii_level
            // };

            let result = tokio::task::spawn_blocking(Self::extract_raw)
                .await
                .map_err(|e| CoreError::Internal(format!("AT-SPI2 blocking task failed: {e}")))?;

            match result {
                Some(info) => {
                    Self::record_success();
                    debug!(role = %info.role, "AT-SPI2 focused element extracted");
                    Ok(Some(info))
                }
                None => {
                    Self::record_failure();
                    debug!("LinuxAccessibility: AT-SPI2 stub -- returning None");
                    Ok(None)
                }
            }
        }

        fn has_permission(&self) -> bool {
            // AT-SPI2 does not require special permissions on Linux.
            // Any user-session process can connect to the accessibility bus.
            Self::check_atspi_available()
        }

        fn name(&self) -> &str {
            "linux-atspi2-accessibility"
        }
    }
}

#[cfg(target_os = "linux")]
pub use inner::LinuxAccessibility;

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::inner::*;
    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    #[test]
    fn has_permission_true() {
        let extractor = LinuxAccessibility::new();
        assert!(extractor.has_permission());
    }

    #[test]
    fn name_is_correct() {
        let extractor = LinuxAccessibility::new();
        assert_eq!(extractor.name(), "linux-atspi2-accessibility");
    }

    #[tokio::test]
    async fn stub_returns_none() {
        let extractor = LinuxAccessibility::new();
        let result = extractor
            .extract_focused_element(PiiFilterLevel::Standard, false)
            .await
            .unwrap();
        // Stub always returns None until AT-SPI2 is wired
        assert!(result.is_none());
    }
}
