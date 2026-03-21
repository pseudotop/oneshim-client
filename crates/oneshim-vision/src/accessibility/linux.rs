//! Linux AT-SPI2 accessibility extractor.
//!
//! Extracts the currently focused UI element via the AT-SPI2 protocol over
//! D-Bus. AT-SPI2 (Assistive Technology Service Provider Interface) is the
//! standard accessibility framework on Linux desktops (GNOME, KDE, XFCE).
//!
//! ## Architecture
//!
//! The implementation flow:
//!
//! 1. Connect to the D-Bus session bus via `atspi::AccessibilityConnection::new()`
//! 2. Walk the accessibility registry: root → applications → frames
//! 3. Find the active window by checking `State::Active` on frame nodes
//! 4. For the active frame, recursively traverse children up to `max_depth`
//!    and `max_elements`, extracting role, name, and bounding box via
//!    `ComponentProxy::get_extents(CoordType::Screen)`
//! 5. Apply PII-level gating: Strict suppresses labels, Standard/Basic/Off
//!    include them.
//!
//! ## Permissions
//!
//! Unlike macOS (which requires Accessibility permission) and some Windows
//! configurations, AT-SPI2 does not require special permissions. Any user
//! process can connect to the AT-SPI2 D-Bus service as long as:
//! - The `at-spi2-core` package is installed (default on most desktop distros)
//! - The AT-SPI2 bus is running (started by the desktop session manager)
//! - The `ATSPI_BUS_ADDRESS` or `DBUS_SESSION_BUS_ADDRESS` env var is set

#[cfg(target_os = "linux")]
mod inner {
    use std::sync::atomic::{AtomicU32, Ordering};

    use async_trait::async_trait;
    use tracing::{debug, warn};

    use oneshim_core::config::PiiFilterLevel;
    use oneshim_core::error::CoreError;
    #[cfg(feature = "linux-atspi")]
    use oneshim_core::models::focused_element::AccessibilityElement;
    #[cfg(feature = "linux-atspi")]
    use oneshim_core::models::focused_element::ElementRect;
    use oneshim_core::models::focused_element::FocusedElementInfo;
    use oneshim_core::ports::accessibility::AccessibilityExtractor;

    // ── Circuit breaker (same pattern as macOS/Windows) ──────────────

    /// Consecutive AT-SPI2 failures before the circuit breaker opens.
    static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);
    const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
    const CIRCUIT_BREAKER_RETRY_INTERVAL: u32 = 60;

    /// Linux AT-SPI2 accessibility extractor.
    ///
    /// Connects to the AT-SPI2 D-Bus service and traverses the accessibility
    /// tree for the active window to extract UI element information.
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
        fn check_atspi_available() -> bool {
            #[cfg(feature = "linux-atspi")]
            {
                // Check if the AT-SPI2 bus address environment variable is set
                std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
                    || std::env::var("ATSPI_BUS_ADDRESS").is_ok()
            }
            #[cfg(not(feature = "linux-atspi"))]
            {
                true // Stub mode, claim available
            }
        }

        /// Extract focused element via AT-SPI2 (stub).
        ///
        /// The `extract_window_elements` method provides the full tree traversal.
        /// This single-element extraction remains a stub pending AT-SPI focus
        /// tracking integration.
        fn extract_raw() -> Option<FocusedElementInfo> {
            // Stub: single-element focus tracking not yet implemented.
            // Use extract_window_elements() for full tree traversal.
            None
        }

        // ── AT-SPI tree traversal helpers (linux-atspi feature) ──────

        /// Recursively traverse the AT-SPI accessibility tree starting from
        /// `proxy`, collecting elements up to `max_depth` levels deep and
        /// `remaining` total elements.
        ///
        /// Each node's role, name, and bounding box (via ComponentProxy) are
        /// extracted and converted to `AccessibilityElement`. Individual
        /// element failures are skipped silently.
        #[cfg(feature = "linux-atspi")]
        async fn traverse_tree(
            conn: &atspi::connection::AccessibilityConnection,
            proxy: &atspi::proxy::accessible::AccessibleProxy<'_>,
            depth: u32,
            max_depth: u32,
            remaining: &mut usize,
            pii_level: PiiFilterLevel,
        ) -> Vec<AccessibilityElement> {
            if depth > max_depth || *remaining == 0 {
                return Vec::new();
            }

            let mut results = Vec::new();

            // Extract role as a string
            let role_str = match proxy.get_role().await {
                Ok(role) => format!("{role:?}"),
                Err(_) => "Unknown".to_string(),
            };

            // Extract name/label (suppress at Strict PII level)
            let label = if pii_level != PiiFilterLevel::Strict {
                proxy.name().await.unwrap_or_default()
            } else {
                String::new()
            };

            // Extract bounding box via ComponentProxy
            let bounds = Self::get_element_bounds(conn, proxy).await;

            results.push(AccessibilityElement {
                role: role_str,
                label,
                bounds,
            });
            *remaining = remaining.saturating_sub(1);

            // Recurse into children
            if depth < max_depth && *remaining > 0 {
                // get_children() returns Vec<(destination, object_path)>
                // representing child accessible objects on the D-Bus.
                let children = match proxy.get_children().await {
                    Ok(c) => c,
                    Err(_) => return results,
                };

                for child_ref in &children {
                    if *remaining == 0 {
                        break;
                    }

                    // Build an AccessibleProxy for the child.
                    // child_ref has .name() (bus destination) and .path()
                    // (D-Bus object path). Use .ok() chaining since we
                    // are not in a Result-returning fn.
                    let child_proxy =
                        match atspi::proxy::accessible::AccessibleProxy::builder(conn.connection())
                            .destination(child_ref.name())
                            .ok()
                            .and_then(|b| b.path(child_ref.path()).ok())
                        {
                            Some(builder) => match builder.build().await {
                                Ok(p) => p,
                                Err(_) => continue, // Skip inaccessible children
                            },
                            None => continue, // Skip if dest/path invalid
                        };

                    let child_elements = Box::pin(Self::traverse_tree(
                        conn,
                        &child_proxy,
                        depth + 1,
                        max_depth,
                        remaining,
                        pii_level,
                    ))
                    .await;
                    results.extend(child_elements);
                }
            }

            results
        }

        /// Extract the bounding rectangle for an element via `ComponentProxy`.
        ///
        /// Returns `None` if the element does not support the Component
        /// interface or if the extents query fails.
        #[cfg(feature = "linux-atspi")]
        async fn get_element_bounds(
            conn: &atspi::connection::AccessibilityConnection,
            proxy: &atspi::proxy::accessible::AccessibleProxy<'_>,
        ) -> Option<ElementRect> {
            use atspi_common::CoordType;

            // Query the Component interface for extents.
            // AccessibleProxy wraps a zbus Proxy; we extract its
            // destination and path to build a ComponentProxy for the same
            // D-Bus object.
            let inner_proxy = proxy.inner();
            let dest = inner_proxy.destination().to_string();
            let path = inner_proxy.path().to_string();

            let component = atspi::proxy::component::ComponentProxy::builder(conn.connection())
                .destination(dest.as_str())
                .ok()?
                .path(path.as_str())
                .ok()?
                .build()
                .await
                .ok()?;

            let (x, y, w, h) = component.get_extents(CoordType::Screen).await.ok()?;

            // Filter out zero-sized or off-screen elements
            if w <= 0 || h <= 0 {
                return None;
            }

            Some(ElementRect {
                x: x as f32,
                y: y as f32,
                width: w as f32,
                height: h as f32,
            })
        }

        /// Find the active window frame across all AT-SPI applications.
        ///
        /// Walks: registry root → applications → children (frames/windows),
        /// checking each frame for `State::Active`. Returns the first active
        /// frame's AccessibleProxy, or `None` if no active window is found.
        #[cfg(feature = "linux-atspi")]
        async fn find_active_window<'a>(
            conn: &'a atspi::connection::AccessibilityConnection,
        ) -> Option<atspi::proxy::accessible::AccessibleProxy<'a>> {
            use atspi_common::Role;
            use atspi_common::State;

            let root = conn.root_accessible_on_registry().await.ok()?;
            let apps = root.get_children().await.ok()?;

            for app_ref in &apps {
                // Build AccessibleProxy for the application
                let app_proxy =
                    atspi::proxy::accessible::AccessibleProxy::builder(conn.connection())
                        .destination(app_ref.name())
                        .ok()?
                        .path(app_ref.path())
                        .ok()?
                        .build()
                        .await
                        .ok()?;

                let children = match app_proxy.get_children().await {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                for child_ref in &children {
                    // Build AccessibleProxy for each child (potential frame)
                    let child_proxy =
                        match atspi::proxy::accessible::AccessibleProxy::builder(conn.connection())
                            .destination(child_ref.name())
                            .ok()
                            .and_then(|b| b.path(child_ref.path()).ok())
                        {
                            Some(builder) => match builder.build().await {
                                Ok(p) => p,
                                Err(_) => continue,
                            },
                            None => continue,
                        };

                    // Check if this is a frame/window with Active state
                    let role = match child_proxy.get_role().await {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    if !matches!(role, Role::Frame | Role::Window | Role::Dialog) {
                        continue;
                    }

                    // Check the state set for Active
                    let states = match child_proxy.get_state().await {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    if states.contains(State::Active) {
                        return Some(child_proxy);
                    }
                }
            }

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

        #[cfg(feature = "linux-atspi")]
        async fn extract_window_elements(
            &self,
            max_depth: u32,
            max_elements: usize,
            pii_level: PiiFilterLevel,
            has_full_text_consent: bool,
        ) -> Result<Vec<AccessibilityElement>, CoreError> {
            use atspi::connection::AccessibilityConnection;

            if !Self::circuit_allows() {
                return Ok(Vec::new());
            }

            let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
                PiiFilterLevel::Standard
            } else {
                pii_level
            };

            // AT-SPI is async-native, no spawn_blocking needed
            let conn = AccessibilityConnection::new().await.map_err(|e| {
                Self::record_failure();
                CoreError::PermissionDenied(format!(
                    "AT-SPI2 D-Bus connection failed. Ensure at-spi2-core is installed: {e}"
                ))
            })?;

            // Find the active window by walking registry → apps → frames
            let active_window = match Self::find_active_window(&conn).await {
                Some(w) => w,
                None => {
                    // No active window found — not an error, just nothing to traverse
                    debug!("AT-SPI2: no active window found");
                    Self::record_success();
                    return Ok(Vec::new());
                }
            };

            // Traverse the active window's subtree
            let mut remaining = max_elements;
            let elements = Self::traverse_tree(
                &conn,
                &active_window,
                0,
                max_depth,
                &mut remaining,
                effective_level,
            )
            .await;

            if elements.is_empty() {
                Self::record_failure();
            } else {
                Self::record_success();
                debug!(count = elements.len(), "AT-SPI2 window tree extracted");
            }

            Ok(elements)
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

    #[cfg(feature = "linux-atspi")]
    #[tokio::test]
    async fn extract_window_elements_atspi_connection() {
        let extractor = LinuxAccessibility::new();
        let result = extractor
            .extract_window_elements(3, 300, PiiFilterLevel::Standard, false)
            .await;
        // On CI without AT-SPI2, this may return PermissionDenied
        // On desktop Linux, should return Ok (possibly empty)
        match result {
            Ok(elements) => {
                eprintln!("AT-SPI2 returned {} elements", elements.len());
            }
            Err(oneshim_core::error::CoreError::PermissionDenied(msg)) => {
                eprintln!("AT-SPI2 not available: {msg}");
            }
            Err(e) => {
                panic!("unexpected error: {e}");
            }
        }
    }
}
