//! `robotz-browser` — Chrome-DevTools-Protocol web automation, extracted from
//! openpisci.
//!
//! [`BrowserTool`] (`browser`) drives a managed Chrome instance: navigate,
//! click, type, screenshot, Set-of-Mark element annotation, DOM interaction.
//! [`BrowserManager`] owns the Chrome-for-Testing lifecycle and page pool.
//!
//! Implements [`robotz_core::Tool`]; enable the `pisci-kernel` feature for the
//! `pisci_kernel::Tool` bridge so openpisci can consume it directly.

pub mod download;
pub mod manager;
pub mod tool;

pub use manager::{create_browser_manager, BrowserManager, BrowserOptions, SharedBrowserManager};
pub use tool::BrowserTool;

#[cfg(feature = "pisci-kernel")]
pub mod pisci_bridge;
