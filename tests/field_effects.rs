//! The silent-no-op guard: every field the vocabulary declares must leave a
//! trace in the rendered HTML.
//!
//! The share-as-track bug shipped past the suite because nothing asserted a
//! parsed field *did* anything — `h: "18"` parsed, validated, and rendered as
//! if absent. This sweep is one row per declared field: give the field a
//! distinctive value, render, demand its marker. A field that parses but
//! leaves no trace fails here, not in a live session.

use phi_canvas::{ScreenSpec, render_spec};
use serde_json::json;

fn rendered(node: serde_json::Value) -> String {
    let spec = ScreenSpec::parse(&json!({
        "fidelity": "color",
        "mock": node
    }))
    .expect("spec renders");
    render_spec(&spec, &[])
}

/// Render `node` and demand `needle` — the row label `why` names the field.
fn assert_trace(node: serde_json::Value, needle: &str, why: &str) {
    let html = rendered(node);
    assert!(
        html.contains(needle),
        "{why}: expected {needle:?} in output\n{html}"
    );
}

#[test]
fn layout_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "frame", "title": "窗口标题", "body": { "kind": "text", "text": "内文" } }),
        "m-frame-hd\">窗口标题",
        "frame.title",
    );
    assert_trace(
        json!({ "kind": "cols", "ratio": [1, 3], "children": [
            { "kind": "text", "text": "a" }, { "kind": "text", "text": "b" }
        ] }),
        "minmax(0, 3fr)",
        "cols.ratio",
    );
    assert_trace(
        json!({ "kind": "rows", "ratio": [2, 5], "children": [
            { "kind": "text", "text": "a" }, { "kind": "text", "text": "b" }
        ] }),
        "minmax(0, 5fr)",
        "rows.ratio",
    );
    assert_trace(
        json!({ "kind": "stack", "count": 3, "child": { "kind": "text", "text": "s" } }),
        "--rows:3",
        "stack.count",
    );
    assert_trace(
        json!({ "kind": "box", "w": "24%", "h": 40, "text": "面板1" }),
        "--w:24%",
        "box.w (cross-axis share)",
    );
    assert_trace(
        json!({ "kind": "box", "w": "24%", "h": 40, "text": "面板1" }),
        "--h:40%",
        "box.h (numeric share)",
    );
    assert_trace(
        json!({ "kind": "box", "w": "24%", "h": 40, "text": "面板1" }),
        "面板1",
        "box.text",
    );
}

#[test]
fn data_widget_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "kpi", "label": "收入", "value": "¥2.1M", "delta": "+4.2%" }),
        "m-kpi-l\">收入",
        "kpi.label",
    );
    assert_trace(
        json!({ "kind": "kpi", "label": "收入", "value": "¥2.1M", "delta": "+4.2%" }),
        "m-kpi-v\">¥2.1M",
        "kpi.value",
    );
    assert_trace(
        json!({ "kind": "kpi", "label": "收入", "value": "¥2.1M", "delta": "+4.2%" }),
        "m-kpi-d\">+4.2%",
        "kpi.delta",
    );
    assert_trace(
        json!({ "kind": "hero", "text": "128,640" }),
        "128,640",
        "hero.text",
    );
    assert_trace(
        json!({ "kind": "sparkline", "shape": "down" }),
        "m-spark-down",
        "sparkline.shape",
    );
    assert_trace(
        json!({ "kind": "ring", "pct": 86, "label": "完成" }),
        "--pct:86",
        "ring.pct",
    );
    assert_trace(
        json!({ "kind": "ring", "pct": 86, "label": "完成" }),
        "完成",
        "ring.label",
    );
    assert_trace(
        json!({ "kind": "bars", "items": [{ "label": "甲", "value": 3 }, { "label": "乙" }] }),
        "--w:100",
        "bars.items[].value (max width)",
    );
    assert_trace(
        json!({ "kind": "bars", "items": [{ "label": "甲", "value": 3 }, { "label": "乙" }] }),
        "甲",
        "bars.items[].label",
    );
    assert_trace(
        json!({ "kind": "area", "label": "趋势" }),
        "趋势",
        "area.label",
    );
    assert_trace(
        json!({ "kind": "viz", "form": "funnel", "label": "转化" }),
        "m-viz-funnel",
        "viz.form",
    );
    assert_trace(
        json!({ "kind": "viz", "form": "funnel", "label": "转化" }),
        "转化",
        "viz.label",
    );
}

#[test]
fn chrome_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "phone_shell", "body": { "kind": "text", "text": "屏" } }),
        "m-phone-status",
        "phone_shell draws the status bar around body",
    );
    assert_trace(
        json!({ "kind": "topbar", "title": "设置", "back": true, "action": "编辑" }),
        "m-topbar-back",
        "topbar.back",
    );
    assert_trace(
        json!({ "kind": "topbar", "title": "设置", "back": true, "action": "编辑" }),
        "m-topbar-t\">设置",
        "topbar.title",
    );
    assert_trace(
        json!({ "kind": "topbar", "title": "设置", "back": true, "action": "编辑" }),
        "m-topbar-a\">编辑",
        "topbar.action",
    );
    assert_trace(
        json!({ "kind": "tabbar", "items": ["首页", "我的"], "active": 1 }),
        "m-tab-on",
        "tabbar.active",
    );
    assert_trace(
        json!({ "kind": "toolbar", "items": ["文件"] }),
        "文件",
        "toolbar.items",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "删除文件？", "text": "不可撤销", "actions": ["取消", "删除"] }),
        "m-dialog-hd\">删除文件？",
        "dialog.title",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "删除文件？", "text": "不可撤销", "actions": ["取消", "删除"] }),
        "m-dialog-bd\">不可撤销",
        "dialog.text",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "删除文件？", "text": "不可撤销", "actions": ["取消", "删除"] }),
        "m-dialog-key\">删除",
        "dialog.actions (last is the key one)",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "t", "body": { "kind": "text", "text": "体" }, "actions": ["好"] }),
        "体",
        "dialog.body",
    );
    assert_trace(
        json!({ "kind": "nav", "items": ["总览"] }),
        "总览",
        "nav.items",
    );
    assert_trace(
        json!({ "kind": "sidebar", "items": ["项目"] }),
        "项目",
        "sidebar.items",
    );
}

#[test]
fn control_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-t\">通知",
        "list_row.title",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-n\">推送",
        "list_row.note",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-meta\">开",
        "list_row.meta",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-ch",
        "list_row.chevron",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-av",
        "list_row.avatar",
    );
    assert_trace(
        json!({ "kind": "switch", "label": "深色模式", "on": true }),
        "深色模式",
        "switch.label",
    );
    assert_trace(
        json!({ "kind": "switch", "label": "深色模式", "on": true }),
        "m-switch-t m-on",
        "switch.on",
    );
    assert_trace(
        json!({ "kind": "segment", "items": ["日", "周"], "active": 1 }),
        "m-seg-on",
        "segment.active",
    );
    assert_trace(
        json!({ "kind": "input", "text": "搜索" }),
        "搜索",
        "input.text",
    );
    assert_trace(
        json!({ "kind": "button", "text": "导出" }),
        "导出",
        "button.text",
    );
}

#[test]
fn dense_content_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "table", "cols": ["名称", "状态"], "rows": [["api", "就绪"], ["web", ""]] }),
        "m-th",
        "table.cols",
    );
    assert_trace(
        json!({ "kind": "table", "cols": ["名称", "状态"], "rows": [["api", "就绪"], ["web", ""]] }),
        "就绪",
        "table.rows",
    );
    assert_trace(
        json!({ "kind": "tree", "items": [{ "label": "src", "depth": 2 }] }),
        "--d:2",
        "tree.items[].depth",
    );
    assert_trace(
        json!({ "kind": "tree", "items": [{ "label": "src", "depth": 2 }] }),
        "src",
        "tree.items[].label",
    );
    assert_trace(
        json!({ "kind": "text", "text": "说明" }),
        "说明",
        "text.text",
    );
    assert_trace(
        json!({ "kind": "placeholder", "text": "占位区" }),
        "占位区",
        "placeholder.text",
    );
}
