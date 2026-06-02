//! `robotz-core` — the minimal, dependency-free foundation every RobotZ
//! tool builds on.
//!
//! It intentionally mirrors the shape of `pisci_kernel::agent::tool` so that
//! the automation tools extracted from openpisci compile against this crate
//! with a one-line import change (`pisci_kernel::agent::tool` →
//! `robotz_core`). An optional `pisci-kernel` feature in the downstream
//! crates then re-implements `pisci_kernel::Tool` on top of these types so
//! openpisci can keep consuming the very same structs.

pub mod proc;
pub mod tool;

pub use tool::{ImageData, Tool, ToolContext, ToolResult};
