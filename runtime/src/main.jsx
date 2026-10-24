/**
 * phi-canvas antd runtime — island hydration for `m-antd-slot` mounts.
 *
 * The Rust renderer owns shape (layout tree, grid, fidelity routing) and emits
 * one slot per component-shaped leaf with a resolved payload:
 *
 *   <div class="m-antd-slot" data-antd='{"kind","component","props","emph"}'>
 *
 * `component` is the binding table's data (`src/antd_map.rs`); the recipes
 * below are this library's adapter code — swapping the library swaps the
 * table and this file, never the spec or the server. Theme tokens ride on the
 * variant card as `data-antd-theme` and land in antd's `ConfigProvider`.
 *
 * No network, no dynamic import: everything is bundled into one IIFE asset.
 */

import { createRoot } from "react-dom/client";
import {
  Avatar,
  Button,
  Card,
  ConfigProvider,
  Input,
  List,
  Menu,
  Progress,
  Segmented,
  Space,
  Statistic,
  Switch,
  Table,
  Tree,
} from "antd";
import { RightOutlined, UserOutlined } from "@ant-design/icons";

/**
 * Named imports keep the bundle tree-shaken; the registry is how a binding
 * name from `src/antd_map.rs` reaches one of them. A name missing here means
 * the map and this adapter disagree — the slot stays unmounted rather than
 * guessing a component.
 */
const REGISTRY = {
  Avatar,
  Button,
  Card,
  Input,
  List,
  Menu,
  Progress,
  Segmented,
  Space,
  Statistic,
  Switch,
  Table,
  Tree,
  "List.Item": List.Item,
  "List.Item.Meta": List.Item.Meta,
};

/** Resolve "List.Item" against the adapter registry. */
function resolve(name) {
  return String(name || "")
    .split(".")
    .reduce((node, key) => (node == null ? node : node[key]), REGISTRY);
}

/** Theme JSON from the nearest variant card → ConfigProvider token overrides. */
function themeOf(slot) {
  const holder = slot.closest("[data-antd-theme]");
  if (!holder) return {};
  try {
    return JSON.parse(holder.getAttribute("data-antd-theme") || "{}");
  } catch {
    return {};
  }
}

/** Mock theme vocabulary → antd design tokens. Unknown knobs stay out. */
function configTheme(theme) {
  const token = {};
  if (theme.accent) token.colorPrimary = theme.accent;
  if (theme.accent2) token.colorInfo = theme.accent2;
  if (theme.bg) token.colorBgBase = theme.bg;
  if (theme.panel) token.colorBgContainer = theme.panel;
  if (theme.border) token.colorBorder = theme.border;
  if (theme.text) token.colorText = theme.text;
  if (theme.text_bright) token.colorTextHeading = theme.text_bright;
  return { token };
}

/** `tree.items` (flat with `depth`) → antd Tree `treeData`. */
function treeData(items) {
  const root = [];
  const stack = [{ depth: -1, children: root }];
  (items || []).forEach((item, index) => {
    const depth = item.depth || 0;
    while (stack.length > 1 && stack[stack.length - 1].depth >= depth) {
      stack.pop();
    }
    const node = { key: index, title: item.label, children: [] };
    stack[stack.length - 1].children.push(node);
    stack.push({ depth, children: node.children });
  });
  const prune = (nodes) =>
    nodes.map((node) => ({
      ...node,
      children: node.children.length ? prune(node.children) : undefined,
    }));
  return prune(root);
}

/**
 * Recipes: (bound component, resolved props, emph) → element.
 * `emph` is the mock's "this node gets the accent" marker — the one place a
 * recipe may change visual weight, and only that.
 */
const RECIPES = {
  input: (C, p) => <C placeholder={p.text || undefined} />,

  button: (C, p, emph) => (
    <C type={emph ? "primary" : "default"}>{p.text}</C>
  ),

  kpi: (C, p) => (
    <C size="small">
      <Statistic title={p.label} value={p.value} suffix={p.delta || undefined} />
    </C>
  ),

  hero: (C, p) => (
    <C value={p.text} style={{ textAlign: "center", width: "100%" }} />
  ),

  ring: (C, p) => (
    <C
      type="circle"
      percent={p.pct}
      size={72}
      style={{ display: "block", margin: "0 auto" }}
    >
      {p.label || `${p.pct}%`}
    </C>
  ),

  bars: (C, p) => {
    const max = Math.max(1, ...(p.items || []).map((item) => item.value || 0));
    return (
      <div>
        {(p.items || []).map((item, index) => (
          <div
            key={index}
            style={{ display: "flex", gap: 8, alignItems: "center" }}
          >
            <span style={{ minWidth: 56 }}>{item.label}</span>
            <C
              percent={Math.round(((item.value || 0) * 100) / max)}
              showInfo={false}
              size="small"
              style={{ flex: 1 }}
            />
          </div>
        ))}
      </div>
    );
  },

  // A tool strip of buttons — the action row an admin screen always has.
  toolbar: (C, p) => (
    <C wrap>
      {(p.items || []).map((item, index) => (
        <Button key={index} size="small">{item}</Button>
      ))}
    </C>
  ),

  // The mock's dialog is inline furniture inside a variant card — a real
  // `Modal` would float over the whole canvas. `Card` carries the silhouette.
  dialog: (C, p) => (
    <C
      title={p.title}
      actions={(p.actions || []).map((action, index) => (
        <span key={index}>{action}</span>
      ))}
    >
      {p.text ? <p>{p.text}</p> : null}
    </C>
  ),

  list_row: (C, p) => (
    <List
      size="small"
      dataSource={[p]}
      renderItem={(row) => (
        <C>
          <C.Meta
            avatar={row.avatar ? <Avatar icon={<UserOutlined />} /> : undefined}
            title={row.title}
            description={row.note || undefined}
          />
          {row.meta ? <span>{row.meta}</span> : null}
          {row.chevron ? <RightOutlined /> : null}
        </C>
      )}
    />
  ),

  switch: (C, p) => (
    <span style={{ display: "inline-flex", gap: 8, alignItems: "center" }}>
      {p.label ? <span>{p.label}</span> : null}
      <C checked={!!p.on} />
    </span>
  ),

  segment: (C, p) => (
    <C
      options={p.items || []}
      value={
        p.active != null ? (p.items || [])[p.active] : undefined
      }
    />
  ),

  table: (C, p) => (
    <C
      size="small"
      pagination={false}
      columns={(p.cols || []).map((col, index) => ({
        title: col,
        dataIndex: String(index),
        key: String(index),
      }))}
      dataSource={(p.rows || []).map((row, index) => ({
        key: index,
        ...Object.fromEntries(row.map((cell, cellIndex) => [String(cellIndex), cell])),
      }))}
    />
  ),

  tree: (C, p) => <C treeData={treeData(p.items)} defaultExpandAll />,

  nav: (C, p) => (
    <C mode="horizontal" items={(p.items || []).map((label, index) => ({ key: index, label }))} />
  ),

  sidebar: (C, p) => (
    <C mode="inline" items={(p.items || []).map((label, index) => ({ key: index, label }))} />
  ),
};

function mountSlot(slot) {
  if (slot.dataset.antdMounted) return;
  let payload;
  try {
    payload = JSON.parse(slot.getAttribute("data-antd") || "");
  } catch {
    return;
  }
  const recipe = RECIPES[payload.kind];
  const C = resolve(payload.component);
  if (!recipe || !C) return;
  slot.dataset.antdMounted = "1";
  const theme = themeOf(slot);
  const child = recipe(C, payload.props || {}, !!payload.emph, theme);
  createRoot(slot).render(
    <ConfigProvider theme={configTheme(theme)}>{child}</ConfigProvider>,
  );
}

function mountAll() {
  document
    .querySelectorAll(".m-antd-slot[data-antd]")
    .forEach((slot) => mountSlot(slot));
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", mountAll);
} else {
  mountAll();
}
