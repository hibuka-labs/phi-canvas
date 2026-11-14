//! Node rendering: mock nodes to `m-*` markup, plus the joins and picked rows
//! that turn child lists into bands.

use crate::assets::escape_html;
use crate::spec::{MockNode, SparkShape, VizKind};

use super::antd::{antd_slot, node_id};
use super::{RenderCtx, TrackAxis, proportion, ratio_vars, style_attr};

pub(crate) fn render_node(
    node: &MockNode,
    ctx: &RenderCtx,
    track_share: Option<TrackAxis>,
) -> String {
    // A mapped leaf on the antd path becomes a hydrate slot; everything else
    // stays the `m-*` markup below (shape is the vocabulary's, face is the
    // library's — see `crate::antd_map`).
    if let Some(slot) = antd_slot(node, ctx) {
        return slot;
    }
    let override_text = node_id(node).and_then(|id| ctx.content.get(id)).cloned();
    let emph = node_id(node)
        .map(|id| ctx.emph.contains(id))
        .unwrap_or(false);
    let class = |base: &str| {
        if emph {
            format!("{base} m-emph")
        } else {
            base.to_string()
        }
    };
    // `content[id]` replaces the node's primary copy slot — the one call site
    // below that routes through `text_of`. Secondary slots stay as authored.
    let text_of = |own: &str| override_text.clone().unwrap_or_else(|| own.to_string());

    match node {
        MockNode::Frame { title, body, .. } => {
            let title = title
                .as_deref()
                .map(|t| {
                    format!(
                        "<div class=\"m-frame-hd\">{}</div>",
                        escape_html(&text_of(t))
                    )
                })
                .unwrap_or_default();
            format!(
                "<div class=\"{}\"><div class=\"m-frame-bar\"><i></i><i></i><i></i></div>{title}<div class=\"m-frame-bd\">{}</div></div>",
                class("m-frame"),
                render_node(body, ctx, None)
            )
        }
        MockNode::Cols {
            ratio, children, ..
        } => format!(
            "<div class=\"{}\"{}>{}</div>",
            class("m-cols"),
            style_attr(&ratio_vars(ratio, children, false)),
            join_nodes(
                children,
                ctx,
                ratio.is_empty().then_some(TrackAxis::Horizontal)
            )
        ),
        MockNode::Rows {
            ratio, children, ..
        } => format!(
            "<div class=\"{}\"{}>{}</div>",
            class("m-rows"),
            style_attr(&ratio_vars(ratio, children, true)),
            join_nodes(
                children,
                ctx,
                ratio.is_empty().then_some(TrackAxis::Vertical)
            )
        ),
        MockNode::Stack { count, child, .. } => {
            let mut children = String::new();
            for _ in 0..*count {
                children.push_str(&render_node(child, ctx, None));
            }
            format!(
                "<div class=\"{}\" style=\"--rows:{count}\">{children}</div>",
                class("m-stack")
            )
        }
        MockNode::Region { w, h, text, .. } => {
            let mut vars: Vec<(&str, String)> = Vec::new();
            // The track-axis share already became the track (see `track_of`);
            // only the cross-axis share still sizes the box in its cell.
            if let Some(w) = w
                && track_share != Some(TrackAxis::Horizontal)
            {
                vars.push(("w", proportion(w)));
            }
            if let Some(h) = h
                && track_share != Some(TrackAxis::Vertical)
            {
                vars.push(("h", proportion(h)));
            }
            let body = match text {
                Some(text) => format!("<span>{}</span>", escape_html(&text_of(text))),
                None => String::new(),
            };
            format!(
                "<div class=\"{}\"{}>{body}</div>",
                class("m-box"),
                style_attr(&vars)
            )
        }
        MockNode::Nav { items, .. } => format!(
            "<div class=\"{}\">{}</div>",
            class("m-nav"),
            join_spans(items)
        ),
        MockNode::Sidebar { items, .. } => format!(
            "<div class=\"{}\">{}</div>",
            class("m-sidebar"),
            join_spans(items)
        ),
        MockNode::Input { text, .. } => {
            let text = text
                .as_deref()
                .map(|t| escape_html(&text_of(t)))
                .unwrap_or_default();
            format!("<div class=\"{}\">{text}</div>", class("m-input"))
        }
        MockNode::Button { text, .. } => {
            let text = text
                .as_deref()
                .map(|t| escape_html(&text_of(t)))
                .unwrap_or_default();
            format!("<div class=\"{}\">{text}</div>", class("m-button"))
        }
        MockNode::Swatches { .. } => {
            let chips = [
                ctx.theme.bg.as_deref(),
                ctx.theme.panel.as_deref(),
                ctx.theme.border.as_deref(),
                ctx.theme.accent.as_deref(),
                ctx.theme.accent2.as_deref(),
                ctx.theme.text.as_deref(),
            ]
            .into_iter()
            .flatten()
            .map(|color| {
                format!(
                    "<i class=\"m-chip\" style=\"--chip:{}\"></i>",
                    escape_html(color)
                )
            })
            .collect::<Vec<_>>()
            .join("");
            // No theme yet: show the token slots rather than an empty strip.
            let chips = if chips.is_empty() {
                String::from(
                    "<i class=\"m-chip\"></i><i class=\"m-chip\"></i><i class=\"m-chip\"></i>",
                )
            } else {
                chips
            };
            format!("<div class=\"{}\">{chips}</div>", class("m-swatches"))
        }
        MockNode::Text { text, .. } => format!(
            "<div class=\"{}\">{}</div>",
            class("m-text"),
            escape_html(&text_of(text))
        ),
        MockNode::Placeholder { text, .. } => {
            let text = text
                .as_deref()
                .map(|t| escape_html(&text_of(t)))
                .unwrap_or_else(|| String::from("内容区"));
            format!("<div class=\"{}\">{text}</div>", class("m-ph"))
        }
        MockNode::Kpi {
            label,
            value,
            delta,
            ..
        } => {
            let label = label
                .as_deref()
                .map(|t| format!("<span class=\"m-kpi-l\">{}</span>", escape_html(t)))
                .unwrap_or_default();
            let delta = delta
                .as_deref()
                .map(|t| format!("<span class=\"m-kpi-d\">{}</span>", escape_html(t)))
                .unwrap_or_default();
            format!(
                "<div class=\"{}\">{label}<span class=\"m-kpi-v\">{}</span>{delta}</div>",
                class("m-kpi"),
                escape_html(&text_of(value))
            )
        }
        MockNode::Hero { text, .. } => format!(
            "<div class=\"{}\">{}</div>",
            class("m-hero"),
            escape_html(&text_of(text))
        ),
        MockNode::Sparkline { shape, .. } => {
            let shape = match shape.unwrap_or(SparkShape::Up) {
                SparkShape::Up => "up",
                SparkShape::Down => "down",
                SparkShape::Flat => "flat",
            };
            format!(
                "<div class=\"{}\"></div>",
                class(&format!("m-spark m-spark-{shape}"))
            )
        }
        MockNode::Ring { pct, label, .. } => {
            let label = label
                .as_deref()
                .map(|t| format!("<span>{}</span>", escape_html(&text_of(t))))
                .unwrap_or_else(|| format!("<span>{pct}%</span>"));
            format!(
                "<div class=\"{}\" style=\"--pct:{pct}\">{label}</div>",
                class("m-ring")
            )
        }
        MockNode::Bars { items, .. } => {
            let max = items
                .iter()
                .filter_map(|item| item.value)
                .fold(0.0_f64, f64::max);
            let rows = items
                .iter()
                .map(|item| {
                    // `--w` is a percent number the stylesheet multiplies back
                    // (see `.m-bar::before`); rounding is plenty for a width.
                    let width = match (item.value, max) {
                        (Some(value), max) if max > 0.0 => {
                            format!(" style=\"--w:{}\"", (value / max * 100.0).round())
                        }
                        _ => String::new(),
                    };
                    format!(
                        "<div class=\"m-bar\"{width}><span>{}</span></div>",
                        escape_html(&item.label)
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            format!("<div class=\"{}\">{rows}</div>", class("m-bars"))
        }
        MockNode::Area { label, .. } => {
            let label = label
                .as_deref()
                .map(|t| format!("<span>{}</span>", escape_html(&text_of(t))))
                .unwrap_or_default();
            format!("<div class=\"{}\">{label}</div>", class("m-area"))
        }
        MockNode::Viz { form, label, .. } => {
            let form = match form {
                VizKind::Map => "map",
                VizKind::Funnel => "funnel",
                VizKind::Graph => "graph",
                VizKind::Area => "area",
            };
            let label = label
                .as_deref()
                .map(|t| format!("<span>{}</span>", escape_html(&text_of(t))))
                .unwrap_or_default();
            format!(
                "<div class=\"{}\">{label}</div>",
                class(&format!("m-viz m-viz-{form}"))
            )
        }

        // ---- device & window chrome ----
        MockNode::PhoneShell { body, .. } => format!(
            "<div class=\"{}\"><div class=\"m-phone-status\"></div><div class=\"m-phone-bd\">{}</div><div class=\"m-phone-home\"></div></div>",
            class("m-phone"),
            render_node(body, ctx, None)
        ),
        MockNode::Topbar {
            title,
            back,
            action,
            ..
        } => {
            let back = if *back {
                String::from("<i class=\"m-topbar-back\"></i>")
            } else {
                String::new()
            };
            let title = title
                .as_deref()
                .map(|t| {
                    format!(
                        "<span class=\"m-topbar-t\">{}</span>",
                        escape_html(&text_of(t))
                    )
                })
                .unwrap_or_default();
            let action = action
                .as_deref()
                .map(|t| format!("<span class=\"m-topbar-a\">{}</span>", escape_html(t)))
                .unwrap_or_default();
            format!(
                "<div class=\"{}\">{back}{title}{action}</div>",
                class("m-topbar")
            )
        }
        MockNode::Tabbar { items, active, .. } => format!(
            "<div class=\"{}\">{}</div>",
            class("m-tabbar"),
            // `lead` is the tab's icon square — decoration the stylesheet
            // shapes, not something the model sends.
            join_picked(items, *active, "m-tab", "m-tab-on", "<i></i>")
        ),
        MockNode::Toolbar { items, .. } => format!(
            "<div class=\"{}\">{}</div>",
            class("m-toolbar"),
            join_spans(items)
        ),
        MockNode::Dialog {
            title,
            text,
            body,
            actions,
            ..
        } => {
            // The message is the primary copy slot; a dialog that says it with
            // a body node instead overrides that node's own slot.
            let title_html = if text.is_some() {
                escape_html(title)
            } else {
                escape_html(&text_of(title))
            };
            let message = text
                .as_deref()
                .map(|t| {
                    format!(
                        "<div class=\"m-dialog-bd\">{}</div>",
                        escape_html(&text_of(t))
                    )
                })
                .unwrap_or_default();
            let body = body
                .as_deref()
                .map(|node| render_node(node, ctx, None))
                .unwrap_or_default();
            // The last action is the key one (cancel / confirm), the convention
            // the dialog's silhouette is drawn around.
            let acts: String = actions
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let key = if index + 1 == actions.len() {
                        " m-dialog-key"
                    } else {
                        ""
                    };
                    format!(
                        "<span class=\"m-dialog-act{key}\">{}</span>",
                        escape_html(action)
                    )
                })
                .collect();
            let acts = if acts.is_empty() {
                String::new()
            } else {
                format!("<div class=\"m-dialog-acts\">{acts}</div>")
            };
            format!(
                "<div class=\"{}\"><div class=\"m-dialog-hd\">{title_html}</div>{message}{body}{acts}</div>",
                class("m-dialog")
            )
        }

        // ---- high-frequency controls ----
        MockNode::ListRow {
            title,
            note,
            meta,
            chevron,
            avatar,
            ..
        } => {
            let av = if *avatar {
                String::from("<i class=\"m-row-av\"></i>")
            } else {
                String::new()
            };
            let note = note
                .as_deref()
                .map(|t| format!("<span class=\"m-row-n\">{}</span>", escape_html(t)))
                .unwrap_or_default();
            let meta = meta
                .as_deref()
                .map(|t| format!("<span class=\"m-row-meta\">{}</span>", escape_html(t)))
                .unwrap_or_default();
            let ch = if *chevron {
                String::from("<i class=\"m-row-ch\"></i>")
            } else {
                String::new()
            };
            format!(
                "<div class=\"{}\">{av}<div class=\"m-row-txt\"><span class=\"m-row-t\">{}</span>{note}</div>{meta}{ch}</div>",
                class("m-row"),
                escape_html(&text_of(title))
            )
        }
        MockNode::Switch { label, on, .. } => {
            let label = label
                .as_deref()
                .map(|t| format!("<span>{}</span>", escape_html(&text_of(t))))
                .unwrap_or_default();
            let track = if *on { "m-switch-t m-on" } else { "m-switch-t" };
            format!(
                "<div class=\"{}\">{label}<i class=\"{track}\"></i></div>",
                class("m-switch")
            )
        }
        MockNode::Segment { items, active, .. } => format!(
            "<div class=\"{}\">{}</div>",
            class("m-segment"),
            join_picked(items, *active, "m-seg", "m-seg-on", "")
        ),

        // ---- dense content ----
        MockNode::Table { cols, rows, .. } => {
            let header = cols
                .iter()
                .map(|col| format!("<span>{}</span>", escape_html(col)))
                .collect::<Vec<_>>()
                .join("");
            let body = rows
                .iter()
                .map(|row| {
                    let cells = row
                        .iter()
                        .map(|cell| format!("<span>{}</span>", escape_html(cell)))
                        .collect::<Vec<_>>()
                        .join("");
                    format!("<div class=\"m-tr\">{cells}</div>")
                })
                .collect::<Vec<_>>()
                .join("");
            format!(
                "<div class=\"{}\"><div class=\"m-tr m-th\">{header}</div>{body}</div>",
                class("m-table")
            )
        }
        MockNode::Tree { items, .. } => {
            let rows = items
                .iter()
                .map(|item| {
                    let depth = item.depth.unwrap_or(0);
                    format!(
                        "<div class=\"m-tree-i\" style=\"--d:{depth}\"><i></i>{}</div>",
                        escape_html(&item.label)
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            format!("<div class=\"{}\">{rows}</div>", class("m-tree"))
        }
    }
}
fn join_nodes(nodes: &[MockNode], ctx: &RenderCtx, track_share: Option<TrackAxis>) -> String {
    nodes
        .iter()
        .map(|node| render_node(node, ctx, track_share))
        .collect::<Vec<_>>()
        .join("")
}
fn join_spans(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("<span>{}</span>", escape_html(item)))
        .collect::<Vec<_>>()
        .join("")
}
/// `tabbar` / `segment`: N equal cells, the `active`-th wearing `on` class.
/// `lead` is per-kind decoration (`<i></i>` is a tab's icon square).
fn join_picked(items: &[String], active: Option<u8>, base: &str, on: &str, lead: &str) -> String {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let cls = if active == Some(index as u8) {
                format!("{base} {on}")
            } else {
                base.to_string()
            };
            format!("<span class=\"{cls}\">{lead}{}</span>", escape_html(item))
        })
        .collect::<Vec<_>>()
        .join("")
}
