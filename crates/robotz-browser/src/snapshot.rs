//! Accessibility-style page snapshots with stable opaque refs (e1, e2, …).
//!
//! Builds a compact YAML-like tree from the DOM (roles, names, states) and
//! caches ref → selector/bounding-box mappings for subsequent tool actions.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chromiumoxide::Page;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

const DEFAULT_MAX_DEPTH: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefEntry {
    pub selector: String,
    pub role: String,
    pub name: String,
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_width: f64,
    pub rect_height: f64,
}

#[derive(Default)]
struct TabSnapshotState {
    refs: HashMap<String, RefEntry>,
    last_tree: String,
    locked: bool,
}

struct SnapshotStore {
    tabs: HashMap<String, TabSnapshotState>,
    locked_tab: Option<String>,
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self {
            tabs: HashMap::new(),
            locked_tab: None,
        }
    }
}

static SNAPSHOT_STORE: Lazy<Arc<RwLock<SnapshotStore>>> =
    Lazy::new(|| Arc::new(RwLock::new(SnapshotStore::default())));

pub fn invalidate_tab(tab_id: &str) {
    let store = SNAPSHOT_STORE.clone();
    let tab_id = tab_id.to_string();
    tokio::spawn(async move {
        let mut s = store.write().await;
        s.tabs.remove(&tab_id);
        if s.locked_tab.as_deref() == Some(tab_id.as_str()) {
            s.locked_tab = None;
        }
    });
}

pub async fn lock_tab(tab_id: &str) -> Result<()> {
    let mut s = SNAPSHOT_STORE.write().await;
    s.locked_tab = Some(tab_id.to_string());
    s.tabs.entry(tab_id.to_string()).or_default().locked = true;
    Ok(())
}

pub async fn unlock_tab() -> Result<()> {
    let mut s = SNAPSHOT_STORE.write().await;
    if let Some(tab) = s.locked_tab.take() {
        if let Some(state) = s.tabs.get_mut(&tab) {
            state.locked = false;
        }
    }
    Ok(())
}

pub async fn assert_not_locked_for_switch(current_tab: &str) -> Result<()> {
    let s = SNAPSHOT_STORE.read().await;
    if let Some(locked) = s.locked_tab.as_deref() {
        if locked != current_tab {
            return Err(anyhow!(
                "Browser is locked to tab '{}'. Call unlock before switching tabs.",
                locked
            ));
        }
    }
    Ok(())
}

pub async fn resolve_ref(tab_id: &str, ref_id: &str) -> Result<RefEntry> {
    let s = SNAPSHOT_STORE.read().await;
    s.tabs
        .get(tab_id)
        .and_then(|t| t.refs.get(ref_id))
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "Unknown ref '{}'. Call snapshot first to refresh the ref map.",
                ref_id
            )
        })
}

pub async fn ref_bounding_box(tab_id: &str, ref_id: &str) -> Result<(f64, f64, f64, f64)> {
    let entry = resolve_ref(tab_id, ref_id).await?;
    Ok((
        entry.rect_x,
        entry.rect_y,
        entry.rect_width,
        entry.rect_height,
    ))
}

pub struct SnapshotOptions {
    pub interactive: bool,
    pub max_depth: u32,
    pub compact: bool,
    pub selector: Option<String>,
    pub include_diff: bool,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            interactive: false,
            max_depth: DEFAULT_MAX_DEPTH,
            compact: false,
            selector: None,
            include_diff: false,
        }
    }
}

pub async fn capture_snapshot(page: &Page, tab_id: &str, opts: SnapshotOptions) -> Result<String> {
    let interactive = opts.interactive;
    let max_depth = opts.max_depth;
    let compact = opts.compact;
    let root_selector = opts
        .selector
        .as_deref()
        .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()))
        .unwrap_or_else(|| "null".into());

    let js = format!(
        r#"(() => {{
  const interactive = {interactive};
  const maxDepth = {max_depth};
  const compact = {compact};
  const rootSel = {root_selector};

  function roleOf(el) {{
    const explicit = el.getAttribute('role');
    if (explicit) return explicit;
    const tag = el.tagName.toLowerCase();
    if (tag === 'a' && el.href) return 'link';
    if (tag === 'button') return 'button';
    if (tag === 'input') {{
      const t = (el.type || 'text').toLowerCase();
      if (t === 'checkbox') return 'checkbox';
      if (t === 'radio') return 'radio';
      if (t === 'submit' || t === 'button') return 'button';
      return 'textbox';
    }}
    if (tag === 'select') return 'combobox';
    if (tag === 'textarea') return 'textbox';
    if (tag === 'img' && el.alt) return 'image';
    if (tag === 'h1' || tag === 'h2' || tag === 'h3') return 'heading';
    return tag;
  }}

  function nameOf(el) {{
    return (
      el.getAttribute('aria-label') ||
      el.getAttribute('title') ||
      el.getAttribute('placeholder') ||
      el.getAttribute('alt') ||
      (el.labels && el.labels[0] && el.labels[0].textContent) ||
      el.textContent ||
      ''
    ).trim().slice(0, 120);
  }}

  function isInteractive(el) {{
    const tag = el.tagName.toLowerCase();
    if (['a','button','input','select','textarea'].includes(tag)) return true;
    const role = el.getAttribute('role');
    if (role && /button|link|checkbox|radio|menuitem|tab|option|combobox|textbox|searchbox|switch/.test(role)) return true;
    if (el.hasAttribute('onclick') || el.hasAttribute('tabindex')) return true;
    return false;
  }}

  function bestSelector(el) {{
    if (el.id) return '#' + CSS.escape(el.id);
    const testId = el.getAttribute('data-testid');
    if (testId) return `[data-testid="${{CSS.escape(testId)}}"]`;
    const name = el.getAttribute('name');
    if (name) return `${{el.tagName.toLowerCase()}}[name="${{CSS.escape(name)}}"]`;
    const aria = el.getAttribute('aria-label');
    if (aria) return `${{el.tagName.toLowerCase()}}[aria-label="${{CSS.escape(aria)}}"]`;
    let s = el.tagName.toLowerCase();
    if (el.classList && el.classList.length) {{
      s += '.' + Array.from(el.classList).slice(0, 2).map(c => CSS.escape(c)).join('.');
    }}
    const p = el.parentElement;
    if (p) {{
      const same = Array.from(p.children).filter(c => c.tagName === el.tagName);
      if (same.length > 1) s += `:nth-of-type(${{same.indexOf(el) + 1}})`;
    }}
    return s;
  }}

  function visible(el) {{
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) return false;
    const st = window.getComputedStyle(el);
    return st.visibility !== 'hidden' && st.display !== 'none' && st.opacity !== '0';
  }}

  const root = rootSel ? document.querySelector(rootSel) : document.body;
  if (!root) return {{ error: 'Root selector not found', tree: '', refs: [] }};

  const refs = [];
  let counter = 0;
  const lines = [];

  function walk(el, depth) {{
    if (!el || depth > maxDepth) return;
    if (el.nodeType !== 1) return;
    const tag = el.tagName.toLowerCase();
    if (tag === 'script' || tag === 'style' || tag === 'noscript') return;
    const inter = isInteractive(el);
    if (interactive && !inter) {{
      for (const ch of el.children || []) walk(ch, depth + 1);
      return;
    }}
    if (!visible(el) && depth > 0) {{
      for (const ch of el.children || []) walk(ch, depth + 1);
      return;
    }}
    counter += 1;
    const ref = 'e' + counter;
    const role = roleOf(el);
    const name = nameOf(el);
    const r = el.getBoundingClientRect();
    const sel = bestSelector(el);
    refs.push({{
      ref,
      selector: sel,
      role,
      name,
      rect_x: r.x,
      rect_y: r.y,
      rect_width: r.width,
      rect_height: r.height,
    }});
    const indent = compact ? '' : '  '.repeat(depth);
    const stateParts = [];
    if (el.disabled) stateParts.push('disabled');
    if (el.checked) stateParts.push('checked');
    if (el.getAttribute('aria-expanded') === 'true') stateParts.push('expanded');
    const stateStr = stateParts.length ? ` [${{stateParts.join(',')}}]` : '';
    const nameStr = name ? ` "${{name.replace(/"/g, '\\"')}}"` : '';
    lines.push(`${{indent}}- ${{ref}} ${{role}}${{nameStr}}${{stateStr}}`);
    for (const ch of el.children || []) walk(ch, depth + 1);
  }}

  walk(root, 0);
  return {{ tree: lines.join('\n'), refs }};
}})()"#
    );

    let val = page
        .evaluate(js.as_str())
        .await
        .map_err(|e| anyhow!("snapshot evaluate failed: {e}"))?
        .into_value()
        .unwrap_or(Value::Null);

    if let Some(err) = val.get("error").and_then(|v| v.as_str()) {
        return Err(anyhow!("{err}"));
    }

    let tree = val
        .get("tree")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut ref_map = HashMap::new();
    if let Some(arr) = val.get("refs").and_then(|v| v.as_array()) {
        for item in arr {
            let ref_id = item
                .get("ref")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if ref_id.is_empty() {
                continue;
            }
            ref_map.insert(
                ref_id.clone(),
                RefEntry {
                    selector: item
                        .get("selector")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    role: item
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    rect_x: item.get("rect_x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    rect_y: item.get("rect_y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    rect_width: item
                        .get("rect_width")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    rect_height: item
                        .get("rect_height")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                },
            );
        }
    }

    let mut output = format!(
        "Page snapshot ({} refs). Use ref with click/type/fill/assert/hover.\n\n{}",
        ref_map.len(),
        tree
    );

    if opts.include_diff {
        let prev = {
            let s = SNAPSHOT_STORE.read().await;
            s.tabs
                .get(tab_id)
                .map(|t| t.last_tree.clone())
                .unwrap_or_default()
        };
        if !prev.is_empty() && prev != tree {
            output.push_str("\n\n--- diff: snapshot changed since last capture ---");
        } else if !prev.is_empty() {
            output.push_str("\n\n--- diff: no structural change ---");
        }
    }

    {
        let mut s = SNAPSHOT_STORE.write().await;
        let state = s.tabs.entry(tab_id.to_string()).or_default();
        state.refs = ref_map;
        state.last_tree = tree;
    }

    Ok(output)
}

pub async fn get_ref_map_json(tab_id: &str) -> Value {
    let s = SNAPSHOT_STORE.read().await;
    let refs: Vec<Value> = s
        .tabs
        .get(tab_id)
        .map(|t| {
            t.refs
                .iter()
                .map(|(k, v)| {
                    json!({
                        "ref": k,
                        "selector": v.selector,
                        "role": v.role,
                        "name": v.name,
                        "rect": [v.rect_x, v.rect_y, v.rect_width, v.rect_height],
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "refs": refs })
}
