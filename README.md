# RobotZ

A standalone **computer-use agent toolkit**, extracted from
[openpisci](../openpisci)'s desktop-automation layer.

RobotZ packages the ability to *drive a computer* — take screenshots, move the
mouse, type, manage windows, read the Windows UI Automation tree, and automate
a browser over the Chrome DevTools Protocol — as a set of reusable tools, and
exposes them as an **MCP server** so any MCP client (Cursor, Claude Desktop, …)
can control the machine.

## Workspace layout

| Crate | Responsibility |
|-------|----------------|
| `robotz-core` | Dependency-free `Tool` / `ToolResult` / `ImageData` abstraction + cross-platform `proc` spawn helper. |
| `robotz-automation` | Desktop control: screen capture, mouse/keyboard, window management, Windows UIA, mouse calibration. |
| `robotz-browser` | Chrome-DevTools-Protocol web automation. |
| `robotz-mcp` | MCP server binary (`rmcp`, stdio) exposing the tools to any MCP client. |

## Run the MCP server

```bash
cargo run -p robotz-mcp            # full tool set
cargo run -p robotz-mcp -- --readonly   # observation-only (screenshots, reads)
```

Register it with an MCP client (e.g. Cursor / Claude Desktop) by pointing the
client at the built `robotz-mcp` binary over stdio. It exposes `screen_capture`,
`desktop_automation`, `browser`, and (on Windows) `uia`, with screenshots
returned as MCP image content.

The `robotz-*` crates expose an optional **`pisci-kernel`** feature that
re-implements `pisci_kernel::Tool` on top of the same structs, so openpisci can
consume RobotZ as a drop-in replacement for its in-tree tools.

## Runtime dependencies

These are *external programs* the tools shell out to (not bundled):

- **Linux**: an X11 session, `xdotool`, `wmctrl`, `xclip`; the bundled
  `xi_helpers` C helper for smooth pointer movement.
- **macOS**: `cliclick`, `osascript`; Accessibility permission granted.
- **Windows**: built-in UI Automation runtime + PowerShell.
- **Browser tool**: Chrome / Chrome for Testing (auto-downloaded on first use).

## Status

Functional. All four crates build and the MCP server responds to
`initialize` / `tools/list`. The optional `pisci-kernel` feature (see each
crate's `pisci_bridge`) lets openpisci consume these crates as a drop-in
replacement for its in-tree tools — that migration (removing the originals from
openpisci) is the remaining step.
