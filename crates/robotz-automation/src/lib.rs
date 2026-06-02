//! `robotz-automation` — cross-platform desktop control tools extracted from
//! openpisci.
//!
//! - [`ScreenTool`] (`screen_capture`): full-screen / window / region capture,
//!   multi-monitor enumeration, coordinate-grid + cursor-crosshair overlays.
//! - [`DesktopAutomationTool`] (`desktop_automation`): mouse, keyboard, window
//!   management, app launching.
//! - `UiaTool` (`uia`, Windows only): UI Automation tree navigation, precise
//!   element click/type/drag, backed by [`calibration`] for residual-drift
//!   compensation.
//!
//! All tools implement [`robotz_core::Tool`]. Enable the `pisci-kernel`
//! feature to additionally implement `pisci_kernel::agent::tool::Tool` (see
//! [`pisci_bridge`]) so openpisci can consume the very same structs.

pub mod calibration;
pub mod desktop_automation;
pub mod screen;

#[cfg(target_os = "windows")]
pub mod uia;

pub use desktop_automation::DesktopAutomationTool;
pub use screen::ScreenTool;

#[cfg(target_os = "windows")]
pub use uia::UiaTool;

#[cfg(feature = "pisci-kernel")]
pub mod pisci_bridge;
