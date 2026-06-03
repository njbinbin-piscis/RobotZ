//! RobotZ example host — visual panel + MCP client + benchmarks.

pub mod bench;
pub mod calibrate;
pub mod mcp_client;
pub mod panel;
pub mod runner;

pub use bench::{host_data_dir, run_benchmark, write_report, BenchOptions, BenchmarkReport};
pub use calibrate::{CalibrationWizard, CALIBRATION_TARGET_INDICES};
pub use mcp_client::{find_robotz_mcp_binary, mcp_demo_sync, mcp_result_summary, McpSession};
pub use panel::{PanelApp, PanelState, WINDOW_TITLE};
pub use runner::ToolRunner;
