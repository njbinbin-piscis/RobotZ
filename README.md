# RobotZ

A standalone **computer-use agent toolkit**, extracted from
[openpiscis](../openpiscis)'s desktop-automation layer.

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
| `robotz-toolset` | Shared tool registry used by the MCP server and host. |
| `robotz-host` | Example host with a visual test panel (mouse / keyboard / capture demos). |

## Run the MCP server

```bash
cargo run -p robotz-mcp            # full tool set
cargo run -p robotz-mcp -- --readonly   # observation-only (screenshots, reads)
```

## Run the example host (visual test panel)

```bash
cargo run -p robotz-host              # opens the RobotZ Test Panel window
cargo run -p robotz-host -- tools     # print tool names and descriptions
cargo run -p robotz-host -- inspect   # headless-friendly read-only smoke
cargo run -p robotz-host -- mcp-demo  # spawn robotz-mcp and call tools over MCP
cargo run -p robotz-host -- bench     # run benchmark suite → ~/.local/share/robotz-host/bench-latest.json
```

The panel sidebar adds:

- **MCP transport** — connect to `robotz-mcp` subprocess and route tool calls through the protocol
- **Benchmark** — timing/accuracy suite written to `bench-latest.json`
- **UIA calibration** (Windows) — five-point wizard using panel anchor targets (#0, #4, #9, #14, #19)

The panel shows a grid of targets with **physical pixel coordinates** (for
`desktop_automation.click`), a drag zone, a keyboard field, and sidebar actions
that invoke the same tools as `robotz-mcp`. Point your MCP client at
`robotz-mcp` and automate this window to exercise end-to-end computer use.

**Linux**: requires an X11 session plus `xdotool`, `wmctrl`, and `xclip` (see
Runtime dependencies below).

Register it with an MCP client (e.g. Cursor / Claude Desktop) by pointing the
client at the built `robotz-mcp` binary over stdio. It exposes `screen_capture`,
`desktop_automation`, `browser`, and (on Windows) `uia`, with screenshots
returned as MCP image content.

The `robotz-*` crates expose an optional **`piscis-kernel`** feature that
re-implements `piscis_kernel::Tool` on top of the same structs, so openpiscis can
consume RobotZ as a drop-in replacement for its in-tree tools.

## Runtime dependencies

These are *external programs* the tools shell out to (not bundled):

- **Linux**: an X11 session, `xdotool`, `wmctrl`, `xclip`; the bundled
  `xi_helpers` C helper for smooth pointer movement.
- **macOS**: `cliclick`, `osascript`; Accessibility permission granted.
- **Windows**: built-in UI Automation runtime + PowerShell.
- **Browser tool**: Chrome / Chrome for Testing (auto-downloaded on first use).

## Install from GitHub Releases

Pre-built **robotz-host** (test panel) and **robotz-mcp** (MCP server) binaries
are attached to [GitHub Releases](https://github.com/njbinbin-piscis/RobotZ/releases).

**Linux / macOS** (download by version):

```bash
curl -fsSL https://raw.githubusercontent.com/njbinbin-piscis/RobotZ/v0.1.1/scripts/install.sh | bash -s -- v0.1.1
robotz-host    # open test panel
robotz-mcp     # MCP server on stdio
```

Or extract the `.tar.gz` for your platform and run `./install.sh` inside the folder.

**Windows** — download `robotz-*-pc-windows-msvc.zip`, extract, then in PowerShell:

```powershell
.\install.ps1
robotz-host.exe
```

Ensure runtime dependencies (see below) are installed before driving the desktop.

## Status

Functional. All crates build and the MCP server responds to
`initialize` / `tools/list`. The optional `piscis-kernel` feature (see each
crate's `piscis_bridge`) lets openpiscis consume these crates as a drop-in
replacement for its in-tree tools — that migration (removing the originals from
openpiscis) is the remaining step.
