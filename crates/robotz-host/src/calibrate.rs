//! Windows UIA five-point calibration wizard (uses panel target coordinates).

use std::path::Path;

use anyhow::{Context, Result};
use robotz_automation::calibration;

#[cfg(target_os = "windows")]
use robotz_automation::calibration::{
    build_fingerprint, calibration_file_path, fit_monitor_calibration, save_file, set_cached,
    CalibrationFile,
};
use serde_json::json;

use crate::runner::ToolRunner;

/// Indices on the 5×4 panel grid used as calibration anchors (corners + center).
pub const CALIBRATION_TARGET_INDICES: [usize; 5] = [0, 4, 9, 14, 19];

#[derive(Debug, Clone, Default)]
pub struct CalibrationWizard {
    pub step: usize,
    pub targets: Vec<(i32, i32)>,
    pub actuals: Vec<(i32, i32)>,
    pub monitor_index: usize,
    pub message: String,
    pub finished_rms: Option<f64>,
}

impl CalibrationWizard {
    pub fn reset(&mut self) {
        *self = Self::default();
        self.message = "Ready — press Start, then run each sample.".into();
    }

    pub fn is_active(&self) -> bool {
        self.step > 0 && self.step <= CALIBRATION_TARGET_INDICES.len()
    }

    pub fn is_done(&self) -> bool {
        self.step > CALIBRATION_TARGET_INDICES.len()
    }
}

pub fn calibration_supported() -> bool {
    cfg!(target_os = "windows")
}

#[cfg(target_os = "windows")]
pub fn monitor_for_point(
    monitors: &[MonitorSnapshot],
    x: i32,
    y: i32,
) -> Option<usize> {
    monitors
        .iter()
        .find(|m| point_in_rect(x, y, m.rect))
        .map(|m| m.index)
}

#[cfg(not(target_os = "windows"))]
pub fn monitor_for_point(_monitors: &[calibration::MonitorSnapshot], _x: i32, _y: i32) -> Option<usize> {
    None
}

#[allow(dead_code)]
fn point_in_rect(x: i32, y: i32, rect: [i32; 4]) -> bool {
    let [l, t, r, b] = rect;
    x >= l && x < r && y >= t && y < b
}

/// Run one wizard step: click via `uia` without calibration, record actual cursor.
pub fn run_sample(
    runner: &ToolRunner,
    wizard: &mut CalibrationWizard,
    target: (i32, i32),
) -> Result<()> {
    if !calibration_supported() {
        anyhow::bail!("UIA calibration is only supported on Windows");
    }

    let (x, y) = target;
    let r = runner.call_sync(
        "uia",
        json!({
            "action": "click",
            "x": x,
            "y": y,
            "_skip_calibration": true
        }),
    )?;
    if r.is_error {
        anyhow::bail!("uia.click failed: {}", r.content);
    }

    std::thread::sleep(std::time::Duration::from_millis(120));

    let actual = read_cursor(runner).context("read cursor after uia.click")?;
    wizard.targets.push(target);
    wizard.actuals.push(actual);
    wizard.message = format!(
        "Sample {}: target ({x},{y}) → actual ({},{})",
        wizard.actuals.len(),
        actual.0,
        actual.1
    );
    Ok(())
}

fn read_cursor(runner: &ToolRunner) -> Result<(i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = calibration::windows_helpers::cursor_position() {
            return Ok(p);
        }
    }
    let r = runner.call_sync(
        "desktop_automation",
        json!({ "action": "get_cursor_position" }),
    )?;
    parse_cursor(&r.content).ok_or_else(|| anyhow::anyhow!("parse cursor: {}", r.content))
}

fn parse_cursor(content: &str) -> Option<(i32, i32)> {
    let start = content.find('(')?;
    let rest = &content[start + 1..];
    let end = rest.find(')')?;
    let parts: Vec<_> = rest[..end].split(',').collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].trim().parse().ok()?, parts[1].trim().parse().ok()?))
}

/// Finalize fit and persist `uia_calibration.json` under [`crate::bench::host_data_dir`].
#[cfg(target_os = "windows")]
pub fn finalize_wizard(wizard: &mut CalibrationWizard, app_data: &Path) -> Result<()> {
    use calibration::windows_helpers;

    if wizard.targets.len() < 3 {
        anyhow::bail!("need at least 3 samples, have {}", wizard.targets.len());
    }

    let virtual_screen = windows_helpers::virtual_screen_rect();
    let monitors = windows_helpers::enumerate_monitors_with_dpi();
    let monitor_index = wizard.monitor_index;
    let monitor = monitors
        .iter()
        .find(|m| m.index == monitor_index)
        .or(monitors.first())
        .context("no monitor for calibration")?;

    let entry = fit_monitor_calibration(
        monitor.index,
        monitor.rect,
        &wizard.targets,
        &wizard.actuals,
    );
    wizard.finished_rms = Some(entry.residual_rms_px);

    let fingerprint = build_fingerprint(&virtual_screen, &monitors);
    let file = CalibrationFile {
        fingerprint,
        monitors: vec![entry],
        version: 1,
    };

    let path = calibration_file_path(app_data);
    save_file(&path, &file)?;
    calibration::refresh_cache(app_data);
    set_cached(file);

    wizard.step = CALIBRATION_TARGET_INDICES.len() + 1;
    wizard.message = format!(
        "Saved calibration to {} (RMS {:.2}px)",
        path.display(),
        wizard.finished_rms.unwrap_or(0.0)
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn finalize_wizard(_wizard: &mut CalibrationWizard, _app_data: &Path) -> Result<()> {
    anyhow::bail!("UIA calibration is only supported on Windows")
}
