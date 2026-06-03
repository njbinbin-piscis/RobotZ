//! Visual test surface for mouse, keyboard, and screen-capture workflows.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use base64::Engine;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::json;

use crate::bench::{host_data_dir, run_benchmark, write_report, BenchOptions};
use crate::calibrate::{
    self, CalibrationWizard, CALIBRATION_TARGET_INDICES,
};
use crate::mcp_client::{find_robotz_mcp_binary, mcp_result_summary, McpSession};
use crate::runner::ToolRunner;

pub const WINDOW_TITLE: &str = "RobotZ Test Panel";

const GRID_COLS: usize = 5;
const GRID_ROWS: usize = 4;
const TARGET_COUNT: usize = GRID_COLS * GRID_ROWS;

#[derive(Clone, Debug)]
pub struct LogLine {
    pub at: Instant,
    pub text: String,
    pub is_error: bool,
}

#[derive(Default)]
pub struct PanelState {
    /// Index of the last target the pointer hovered (from tool poll).
    pub hover_target: Option<usize>,
    /// Index of targets clicked (panel-local egui clicks).
    pub clicked_targets: Vec<usize>,
    /// Last desktop_automation cursor read (physical pixels).
    pub cursor_physical: Option<(i32, i32)>,
    /// Screen coords shown on each target button (estimated or calibrated).
    pub target_screen_coords: Vec<(i32, i32)>,
    /// Per-target coords learned from `get_cursor_position` after a local click.
    pub calibrated_targets: Vec<Option<(i32, i32)>>,
    /// Keyboard test field content (also target for type_text).
    pub keyboard_text: String,
    pub hotkey_log: String,
    pub logs: VecDeque<LogLine>,
    pub last_capture_png: Option<Vec<u8>>,
    pub last_tool_message: String,
    pub drag_start: Option<Pos2>,
    pub drag_end: Option<Pos2>,
    pub running_action: bool,
    /// When true, tool calls go through the MCP subprocess instead of direct `Tool`.
    pub use_mcp: bool,
    pub mcp_status: String,
    pub cal_wizard: CalibrationWizard,
    pub last_bench_report: Option<String>,
}

impl PanelState {
    pub fn push_log(&mut self, text: impl Into<String>, is_error: bool) {
        self.logs.push_front(LogLine {
            at: Instant::now(),
            text: text.into(),
            is_error,
        });
        if self.logs.len() > 80 {
            self.logs.pop_back();
        }
    }
}

pub struct PanelApp {
    pub runner: Arc<ToolRunner>,
    pub mcp: Arc<McpSession>,
    pub state: PanelState,
}

impl PanelApp {
    pub fn new(runner: Arc<ToolRunner>) -> Self {
        let mut state = PanelState::default();
        state.mcp_status = format!(
            "MCP binary: {}",
            find_robotz_mcp_binary().display()
        );
        state.cal_wizard.message = if calibrate::calibration_supported() {
            "Windows: 5-point UIA calibration available.".into()
        } else {
            "UIA calibration requires Windows.".into()
        };
        Self {
            runner,
            mcp: Arc::new(McpSession::new(find_robotz_mcp_binary())),
            state,
        }
    }

    fn run_tool(&mut self, name: &str, input: serde_json::Value, label: &str) {
        if self.state.running_action {
            return;
        }
        self.state.running_action = true;
        let via = if self.state.use_mcp { "MCP" } else { "direct" };
        let outcome = if self.state.use_mcp {
            self.mcp
                .call_tool_sync(name, input)
                .map(|r| {
                    let (text, png, is_error) = mcp_result_summary(&r);
                    (text, png, is_error)
                })
                .map_err(|e| e.to_string())
        } else {
            self.runner
                .call_sync(name, input)
                .map(|r| {
                    let png = r.image.as_ref().and_then(|img| {
                        base64::engine::general_purpose::STANDARD
                            .decode(&img.base64)
                            .ok()
                    });
                    (r.content, png, r.is_error)
                })
                .map_err(|e| e.to_string())
        };

        match outcome {
            Ok((text, png, is_error)) => {
                self.state.last_tool_message = text.clone();
                if let Some(bytes) = png {
                    self.state.last_capture_png = Some(bytes);
                }
                self.state.push_log(
                    format!("[{via}] {label}: {}", text.lines().next().unwrap_or("ok")),
                    is_error,
                );
            }
            Err(e) => {
                self.state.last_tool_message = e.clone();
                self.state.push_log(format!("[{via}] {label} failed: {e}"), true);
            }
        }
        self.state.running_action = false;
    }

    pub fn poll_cursor(&mut self) {
        if self.state.running_action {
            return;
        }
        if let Ok(r) = self
            .runner
            .call_sync("desktop_automation", json!({ "action": "get_cursor_position" }))
        {
            if !r.is_error {
                if let Some((x, y)) = parse_cursor(&r.content) {
                    self.state.cursor_physical = Some((x, y));
                    self.state.hover_target = nearest_target(&self.state.target_screen_coords, x, y);
                }
            }
        }
    }
}

impl eframe::App for PanelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        self.poll_cursor();

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("RobotZ Host");
                ui.separator();
                if ui
                    .button("📷 Capture + grid")
                    .on_hover_text("screen_capture with coordinate grid overlay")
                    .clicked()
                {
                    self.run_tool(
                        "screen_capture",
                        json!({ "action": "capture", "grid": true, "format": "png" }),
                        "screen_capture",
                    );
                }
                if ui.button("🖥 List monitors").clicked() {
                    self.run_tool(
                        "screen_capture",
                        json!({ "action": "list_monitors" }),
                        "list_monitors",
                    );
                }
                if ui.button("📍 Poll cursor (tool)").clicked() {
                    self.poll_cursor();
                    if let Some((x, y)) = self.state.cursor_physical {
                        self.state
                            .push_log(format!("cursor at physical ({x}, {y})"), false);
                    }
                }
                if ui.button("🪟 List windows").clicked() {
                    self.run_tool(
                        "desktop_automation",
                        json!({ "action": "list_windows" }),
                        "list_windows",
                    );
                }
                ui.separator();
                ui.label(
                    RichText::new("Use MCP / desktop_automation against this window — targets show physical pixel coords.")
                        .small()
                        .weak(),
                );
            });
        });

        egui::SidePanel::right("sidebar")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Tool console");
                ui.separator();

                ui.label("Last result");
                ui.add(
                    egui::TextEdit::multiline(&mut self.state.last_tool_message)
                        .desired_width(f32::INFINITY)
                        .desired_rows(4),
                );

                if let Some((x, y)) = self.state.cursor_physical {
                    ui.label(format!("Physical cursor: ({x}, {y})"));
                    if let Some(idx) = self.state.hover_target {
                        ui.label(format!("Nearest target: #{idx}"));
                    }
                }

                ui.separator();
                ui.heading("MCP transport");
                ui.label(RichText::new(&self.state.mcp_status).small());
                ui.checkbox(&mut self.state.use_mcp, "Route tools via MCP subprocess");
                ui.horizontal(|ui| {
                    if ui.button("Connect MCP").clicked() {
                        match self.mcp.connect_sync(false) {
                            Ok(()) => {
                                self.state.use_mcp = true;
                                self.state.mcp_status =
                                    format!("Connected: {}", self.mcp.mcp_path().display());
                                self.state.push_log("MCP connected", false);
                            }
                            Err(e) => {
                                self.state.mcp_status = format!("Connect failed: {e}");
                                self.state.push_log(format!("MCP connect: {e}"), true);
                            }
                        }
                    }
                    if ui.button("Disconnect").clicked() {
                        self.mcp.disconnect_sync();
                        self.state.use_mcp = false;
                        self.state.mcp_status = "Disconnected".into();
                    }
                });
                if ui.button("MCP list_tools").clicked() {
                    if let Ok(t) = self.mcp.list_tools_sync() {
                        let names: Vec<_> = t.tools.iter().map(|x| x.name.as_ref()).collect();
                        self.state.last_tool_message = names.join(", ");
                        self.state.push_log(format!("MCP tools: {}", names.join(", ")), false);
                    }
                }

                ui.separator();
                ui.heading("Benchmark");
                if ui.button("Run benchmark → JSON").clicked() {
                    let targets: Vec<_> = CALIBRATION_TARGET_INDICES
                        .iter()
                        .filter_map(|&i| self.state.calibrated_targets.get(i).copied().flatten())
                        .collect();
                    let report = run_benchmark(
                        &self.runner,
                        BenchOptions {
                            click_targets: targets,
                            ..Default::default()
                        },
                    );
                    let path = host_data_dir().join("bench-latest.json");
                    match write_report(&path, &report) {
                        Ok(()) => {
                            let msg = format!(
                                "Benchmark {}/{} passed → {}",
                                report.passed,
                                report.cases.len(),
                                path.display()
                            );
                            self.state.last_bench_report = Some(msg.clone());
                            self.state.push_log(msg, false);
                        }
                        Err(e) => self.state.push_log(format!("bench write: {e}"), true),
                    }
                }
                if let Some(ref msg) = self.state.last_bench_report {
                    ui.label(RichText::new(msg).small().weak());
                }

                ui.separator();
                ui.heading("UIA calibration (Windows)");
                ui.label(RichText::new(&self.state.cal_wizard.message).small());
                if calibrate::calibration_supported() {
                    if ui.button("Start 5-point wizard").clicked() {
                        self.state.cal_wizard.reset();
                        self.state.cal_wizard.step = 1;
                        self.state.cal_wizard.message =
                            "Step 1/5: run sample on target #0 (corner).".into();
                    }
                    if self.state.cal_wizard.is_active() {
                        let step_idx = self.state.cal_wizard.step - 1;
                        if step_idx < CALIBRATION_TARGET_INDICES.len() {
                            let grid_idx = CALIBRATION_TARGET_INDICES[step_idx];
                            if ui.button(format!("Run sample #{step_idx} (grid #{grid_idx})"))
                                .clicked()
                            {
                                let target = self
                                    .state
                                    .calibrated_targets
                                    .get(grid_idx)
                                    .and_then(|o| *o)
                                    .or_else(|| {
                                        self.state.target_screen_coords.get(grid_idx).copied()
                                    });
                                if let Some(t) = target {
                                    if let Err(e) =
                                        calibrate::run_sample(&self.runner, &mut self.state.cal_wizard, t)
                                    {
                                        self.state.cal_wizard.message = e.to_string();
                                    } else {
                                        self.state.cal_wizard.step += 1;
                                        if self.state.cal_wizard.step
                                            > CALIBRATION_TARGET_INDICES.len()
                                        {
                                            if let Err(e) = calibrate::finalize_wizard(
                                                &mut self.state.cal_wizard,
                                                &host_data_dir(),
                                            ) {
                                                self.state.cal_wizard.message = e.to_string();
                                            }
                                        } else {
                                            let next =
                                                CALIBRATION_TARGET_INDICES[self.state.cal_wizard.step - 1];
                                            self.state.cal_wizard.message = format!(
                                                "Step {}/5: sample grid #{next}",
                                                self.state.cal_wizard.step
                                            );
                                        }
                                    }
                                } else {
                                    self.state.cal_wizard.message =
                                        "Calibrate grid target first (click it locally).".into();
                                }
                            }
                        }
                    }
                    if ui.button("Clear saved calibration").clicked() {
                        let path = robotz_automation::calibration::calibration_file_path(
                            &host_data_dir(),
                        );
                        let _ = robotz_automation::calibration::delete_file(&path);
                        robotz_automation::calibration::clear_cache();
                        self.state.cal_wizard.reset();
                        self.state.push_log("Calibration cleared", false);
                    }
                }

                ui.separator();
                ui.heading("Automation actions");
                ui.horizontal_wrapped(|ui| {
                    for idx in 0..TARGET_COUNT.min(8) {
                        if ui.button(format!("#{idx} click")).clicked() {
                            if let Some(&(x, y)) = self.state.target_screen_coords.get(idx) {
                                self.run_tool(
                                    "desktop_automation",
                                    json!({ "action": "click", "x": x, "y": y }),
                                    &format!("click #{idx}"),
                                );
                            }
                        }
                    }
                });
                if ui.button("Type sample text into field").clicked() {
                    self.run_tool(
                        "desktop_automation",
                        json!({ "action": "type_text", "text": "RobotZ keyboard test 123!" }),
                        "type_text",
                    );
                }
                if ui.button("Hotkey Ctrl+A").clicked() {
                    self.run_tool(
                        "desktop_automation",
                        json!({ "action": "hotkey", "keys": ["ctrl", "a"] }),
                        "hotkey",
                    );
                    self.state.hotkey_log = "Sent ctrl+a via tool".into();
                }

                ui.separator();
                if let Some(ref png) = self.state.last_capture_png {
                    ui.label("Last capture preview");
                    if let Ok(img) = image::load_from_memory(png) {
                        let tex = ctx.load_texture(
                            "last_capture",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [img.width() as usize, img.height() as usize],
                                &img.to_rgba8(),
                            ),
                            egui::TextureOptions::LINEAR,
                        );
                        let max_w = ui.available_width();
                        let scale = (max_w / img.width() as f32).min(1.0);
                        let size = Vec2::new(img.width() as f32 * scale, img.height() as f32 * scale);
                        ui.image((tex.id(), size));
                    }
                }

                ui.separator();
                ui.heading("Log");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for line in &self.state.logs {
                            let color = if line.is_error {
                                Color32::LIGHT_RED
                            } else {
                                Color32::LIGHT_GRAY
                            };
                            ui.label(RichText::new(&line.text).color(color).small());
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Mouse targets");
            ui.label(
                "Click targets locally or via desktop_automation / MCP. Green = clicked; yellow ring = cursor near target.",
            );

            let spacing = 8.0;
            let button_size = Vec2::new(88.0, 52.0);
            self.state.target_screen_coords.clear();

            for row in 0..GRID_ROWS {
                ui.horizontal(|ui| {
                    for col in 0..GRID_COLS {
                        let idx = row * GRID_COLS + col;
                        let label = format!("#{idx}");
                        let (rect, response) = ui.allocate_exact_size(button_size, Sense::click());
                        let estimated = pointer_screen_physical(ctx, &response, rect);
                        let screen = self
                            .state
                            .calibrated_targets
                            .get(idx)
                            .and_then(|o| *o)
                            .unwrap_or(estimated);
                        self.state.target_screen_coords.push(screen);

                        let clicked = self.state.clicked_targets.contains(&idx);
                        let near = self.state.hover_target == Some(idx);

                        let is_cal_anchor = CALIBRATION_TARGET_INDICES.contains(&idx);
                        let fill = if clicked {
                            Color32::from_rgb(40, 120, 60)
                        } else if is_cal_anchor {
                            Color32::from_rgb(80, 60, 30)
                        } else {
                            Color32::from_rgb(45, 55, 75)
                        };
                        let stroke = if near {
                            Stroke::new(3.0, Color32::YELLOW)
                        } else {
                            Stroke::new(1.0, Color32::from_gray(100))
                        };

                        ui.painter().rect(rect, 6.0, fill, stroke);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &label,
                            egui::FontId::proportional(16.0),
                            Color32::WHITE,
                        );
                        ui.painter().text(
                            rect.center() + Vec2::new(0.0, 14.0),
                            egui::Align2::CENTER_CENTER,
                            format!("{screen:?}"),
                            egui::FontId::monospace(9.0),
                            Color32::from_gray(180),
                        );

                        if response.clicked() {
                            if !self.state.clicked_targets.contains(&idx) {
                                self.state.clicked_targets.push(idx);
                            }
                            self.poll_cursor();
                            if let Some(c) = self.state.cursor_physical {
                                if self.state.calibrated_targets.len() <= idx {
                                    self.state.calibrated_targets.resize(idx + 1, None);
                                }
                                self.state.calibrated_targets[idx] = Some(c);
                                self.state.push_log(
                                    format!("Panel click #{idx} — calibrated cursor {c:?}"),
                                    false,
                                );
                            } else {
                                self.state.push_log(
                                    format!("Panel click on #{idx} at {screen:?}"),
                                    false,
                                );
                            }
                        }
                        ui.add_space(spacing);
                    }
                });
                ui.add_space(spacing);
            }

            ui.separator();
            ui.heading("Drag test zone");
            let (drag_rect, drag_resp) = ui.allocate_exact_size(Vec2::new(400.0, 80.0), Sense::drag());
            let drag_color = Color32::from_rgb(70, 50, 90);
            ui.painter().rect(
                drag_rect,
                8.0,
                drag_color,
                Stroke::new(1.5, Color32::from_rgb(140, 100, 180)),
            );
            ui.painter().text(
                drag_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drag here (panel or desktop_automation drag)",
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );
            if drag_resp.drag_started() {
                self.state.drag_start = drag_resp.interact_pointer_pos();
            }
            if drag_resp.drag_stopped() {
                self.state.drag_end = drag_resp.interact_pointer_pos();
                if let (Some(a), Some(b)) = (self.state.drag_start, self.state.drag_end) {
                    let s1 = rect_screen_center(ctx, Rect::from_min_size(a, Vec2::ZERO));
                    let s2 = rect_screen_center(ctx, Rect::from_min_size(b, Vec2::ZERO));
                    self.state.push_log(
                        format!("Panel drag {s1:?} → {s2:?} (use tool drag with these coords)"),
                        false,
                    );
                    if ui.input(|i| i.pointer.secondary_clicked()) {
                        let _ = (s1, s2);
                    }
                }
            }
            if ui.button("Run tool drag across zone").clicked() {
                let s1 = rect_screen_center(ctx, drag_rect);
                let s2 = (
                    s1.0 + drag_rect.width() as i32 - 20,
                    s1.1 + drag_rect.height() as i32 / 2,
                );
                self.run_tool(
                    "desktop_automation",
                    json!({
                        "action": "drag",
                        "x": s1.0,
                        "y": s1.1,
                        "to_x": s2.0,
                        "to_y": s2.1
                    }),
                    "drag",
                );
            }

            ui.separator();
            ui.heading("Keyboard test");
            ui.label("Focus the field below, then use type_text / hotkey from the sidebar or an MCP client.");
            ui.add(
                egui::TextEdit::singleline(&mut self.state.keyboard_text)
                    .desired_width(f32::INFINITY)
                    .hint_text("Type here manually or via desktop_automation.type_text…"),
            );
            ui.label(format!("Hotkey log: {}", self.state.hotkey_log));

            let keys: Vec<String> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Key { key, pressed: true, .. } = e {
                            Some(format!("{key:?}"))
                        } else {
                            None
                        }
                    })
                    .collect()
            });
            if !keys.is_empty() {
                self.state.hotkey_log = keys.join(", ");
            }
        });

        if self.state.running_action {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

/// Best-effort physical pixels from egui pointer position (screen points × PPP).
fn pointer_screen_physical(ctx: &egui::Context, response: &egui::Response, rect: Rect) -> (i32, i32) {
    let ppp = ctx.pixels_per_point();
    let pos = response
        .hover_pos()
        .or_else(|| ctx.input(|i| i.pointer.hover_pos()))
        .unwrap_or(rect.center());
    ((pos.x * ppp).round() as i32, (pos.y * ppp).round() as i32)
}

fn rect_screen_center(ctx: &egui::Context, rect: Rect) -> (i32, i32) {
    let ppp = ctx.pixels_per_point();
    let c = rect.center();
    ((c.x * ppp).round() as i32, (c.y * ppp).round() as i32)
}

fn parse_cursor(content: &str) -> Option<(i32, i32)> {
    // "Cursor position: (123, 456)" or similar
    let start = content.find('(')?;
    let rest = &content[start + 1..];
    let end = rest.find(')')?;
    let parts: Vec<_> = rest[..end].split(',').collect();
    if parts.len() != 2 {
        return None;
    }
    let x = parts[0].trim().parse().ok()?;
    let y = parts[1].trim().parse().ok()?;
    Some((x, y))
}

fn nearest_target(coords: &[(i32, i32)], x: i32, y: i32) -> Option<usize> {
    const THRESHOLD: i32 = 48;
    coords
        .iter()
        .enumerate()
        .min_by_key(|(_, (tx, ty))| (tx - x).abs() + (ty - y).abs())
        .filter(|(_, (tx, ty))| (tx - x).abs() < THRESHOLD && (ty - y).abs() < THRESHOLD)
        .map(|(i, _)| i)
}
