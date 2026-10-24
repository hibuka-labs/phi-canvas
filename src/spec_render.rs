//! ScreenSpec → HTML. The only place that turns mock nodes and theme tokens
//! into markup: the model never writes a tag, so appearance stays in one file.
//!
//! Output contract:
//! - every class is `m-*` (mock) or `fidelity-*`, scoped under `.m-screen`,
//! - geometry (ratios, percentages, ring pcts) arrives as named CSS custom
//!   properties (`--ratio`, `--w`, `--pct`) — the stylesheet decides what they
//!   mean, so a knob cannot smuggle in arbitrary CSS,
//! - theme tokens land as custom properties on each variant card, so two
//!   candidates side by side are two independent palettes,
//! - copy is HTML-escaped like any other envelope metadata.

use std::collections::{BTreeMap, HashSet};

use crate::assets::escape_html;
use crate::envelope::OptionItem;
use crate::spec::{Fidelity, MockNode, ScreenSpec, SparkShape, Surface, Theme, Variant, VizKind};

/// Render a spec into a screen body (the slot `body_html` normally fills).
///
/// `options` supplies each variant's caption so the model does not type the
/// same label twice; `data-choice` on a variant card makes the picture itself
/// the pick surface, matching how the click protocol already works.
pub fn render_spec(spec: &ScreenSpec, options: &[OptionItem]) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str(&format!(
        "<div class=\"m-screen {}\">",
        fidelity_class(spec.fidelity)
    ));

    out.push_str("<div class=\"m-variants\">");
    if spec.variants.is_empty() {
        if let Some(tree) = &spec.mock {
            let theme = Theme::default();
            let emph = HashSet::new();
            let content = BTreeMap::new();
            let antd = antd_path(spec.fidelity, Some(tree));
            let ctx = RenderCtx {
                theme: &theme,
                content: &content,
                emph: &emph,
                antd,
            };
            out.push_str(&format!(
                "<div class=\"m-variant\"{}>{}</div>",
                card_attrs(spec.fidelity, &theme, antd),
                render_node(tree, &ctx, None)
            ));
        }
    } else {
        for variant in &spec.variants {
            out.push_str(&render_variant(spec, variant, options));
        }
    }
    out.push_str("</div>");

    if let Some(notice) = &spec.notice {
        out.push_str(&format!(
            "<p class=\"m-notice\">{}</p>",
            escape_html(notice)
        ));
    }
    out.push_str("</div>");
    out
}

fn render_variant(spec: &ScreenSpec, variant: &Variant, options: &[OptionItem]) -> String {
    let fallback = Theme::default();
    let theme = variant.theme.as_ref().unwrap_or(&fallback);
    let tree = variant.mock.as_ref().or(spec.mock.as_ref());
    let emph: HashSet<String> = variant.emph.iter().cloned().collect();
    let antd = antd_path(spec.fidelity, tree);
    let ctx = RenderCtx {
        theme,
        content: &variant.content,
        emph: &emph,
        antd,
    };
    let caption = caption_for(variant, options);
    // Only the host-rendered options panel is the canonical pick surface; the
    // picture becomes clickable too when the variant is a real option.
    let is_option = options.iter().any(|option| option.id == variant.id);
    let pick = if is_option {
        format!(" data-choice=\"{}\"", escape_html(&variant.id))
    } else {
        String::new()
    };
    let body = tree.map_or_else(
        || String::from("<div class=\"m-ph\">（无结构）</div>"),
        |tree| render_node(tree, &ctx, None),
    );
    // Surface family is a CSS hook: a custom property cannot be branched on
    // without container-style queries, a class can.
    let surface = if spec.fidelity == Fidelity::Texture {
        theme
            .surface
            .map(|surface| {
                let name = match surface {
                    Surface::Glass => "glass",
                    Surface::Matte => "matte",
                    Surface::Metal => "metal",
                };
                format!(" m-surface-{name}")
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    // Wide-form card when the tree is not a phone mock — see `wide_form`.
    let wide = if wide_form(tree) {
        " m-variant-wide"
    } else {
        ""
    };
    format!(
        "<div class=\"m-variant{wide}{surface}\"{pick}{}><div class=\"m-variant-cap\">{caption}</div>{body}</div>",
        card_attrs(spec.fidelity, theme, antd),
    )
}

/// A card is wide-form unless its tree is a phone mock.
///
/// A PC-web screen squeezed into a portrait column reads like a phone mock —
/// so wide forms stack full-row and phone forms keep the side-by-side rail.
/// The form factor is the model's call (it wraps the mock in `phone_shell` or
/// not); the rail follows the content, never a new vocabulary knob. A
/// `phone_shell` nested *inside* a wide composition is a widget, not the
/// form — only the root decides.
fn wide_form(tree: Option<&MockNode>) -> bool {
    !matches!(tree, Some(MockNode::PhoneShell { .. }))
}

/// Per-render context threaded down the tree. Emphasis and copy overrides are
/// variant data, not node data — that is what makes two candidates differ by a
/// one-element list instead of two whole trees.
struct RenderCtx<'a> {
    theme: &'a Theme,
    content: &'a BTreeMap<String, String>,
    emph: &'a HashSet<String>,
    /// The antd face is on (see [`antd_path`]) — mapped leaves render as
    /// hydrate slots instead of `m-*` markup.
    antd: bool,
}

/// Decision A (dual track): the antd face is for the finished look —
/// `color`/`texture` on a PC-web form. A layout question (`wireframe`) keeps
/// the greyscale mock so the shape is judged without the library's polish,
/// and a phone form keeps the greybox face (antd-mobile is the mapping
/// layer's next instance). Only the root decides the form (see [`wide_form`]).
fn antd_path(fidelity: Fidelity, tree: Option<&MockNode>) -> bool {
    fidelity != Fidelity::Wireframe && wide_form(tree)
}

/// Card-level attributes: theme custom properties for the greybox face, plus
/// the theme JSON the antd runtime reads for `ConfigProvider` (one per card,
/// so two candidates stay two independent palettes in both renderers).
fn card_attrs(fidelity: Fidelity, theme: &Theme, antd: bool) -> String {
    let mut attrs = style_attr(&theme_vars(fidelity, theme));
    if antd {
        let json = serde_json::to_string(theme).unwrap_or_default();
        attrs.push_str(&format!(" data-antd-theme='{}'", escape_attr(&json)));
    }
    attrs
}

/// Attribute value inside single quotes: JSON's `"` stays literal (readable
/// markup, greppable tests), the four characters that could break out go.
fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn fidelity_class(fidelity: Fidelity) -> &'static str {
    match fidelity {
        Fidelity::Wireframe => "fidelity-wireframe",
        Fidelity::Color => "fidelity-color",
        Fidelity::Texture => "fidelity-texture",
    }
}

/// Custom properties for one variant card. Color tokens only apply from
/// `fidelity: color` up; the texture knobs only bite at `fidelity: texture` —
/// the same `ring: {pct}` therefore renders as a greyscale outline on a layout
/// question and as a glowing arc on a texture question.
fn theme_vars(fidelity: Fidelity, theme: &Theme) -> Vec<(&'static str, String)> {
    let mut vars = Vec::new();
    if fidelity != Fidelity::Wireframe {
        for (name, value) in [
            ("m-bg", &theme.bg),
            ("m-panel", &theme.panel),
            ("m-border", &theme.border),
            ("m-accent", &theme.accent),
            ("m-accent2", &theme.accent2),
            ("m-text", &theme.text),
            ("m-text-bright", &theme.text_bright),
        ] {
            if let Some(value) = value {
                vars.push((name, value.trim().to_string()));
            }
        }
    }
    if fidelity == Fidelity::Texture {
        for (name, value) in [
            ("m-frost", theme.frost.map(|n| n.to_string())),
            ("m-elevation", theme.elevation.map(|n| n.to_string())),
            ("m-edge", theme.edge.map(|n| n.to_string())),
            ("m-glow", theme.glow.map(|n| n.to_string())),
        ] {
            if let Some(value) = value {
                vars.push((name, value));
            }
        }
    }
    vars
}

/// Inline `style="…"` from a custom-property list. Empty list → no attribute.
fn style_attr(vars: &[(&str, String)]) -> String {
    if vars.is_empty() {
        return String::new();
    }
    let body = vars
        .iter()
        .map(|(name, value)| format!("--{name}:{value}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(" style=\"{body}\"")
}

fn caption_for(variant: &Variant, options: &[OptionItem]) -> String {
    match options.iter().find(|option| option.id == variant.id) {
        Some(option) => escape_html(&option.label),
        None => escape_html(&variant.id),
    }
}

/// Which axis a band's tracks run on. A child's `w`/`h` on that axis is its
/// share of the band and becomes the track itself; on the other axis it stays
/// a size on the box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TrackAxis {
    /// `rows`: tracks are heights, so `h` is the share.
    Vertical,
    /// `cols`: tracks are widths, so `w` is the share.
    Horizontal,
}

fn render_node(node: &MockNode, ctx: &RenderCtx, track_share: Option<TrackAxis>) -> String {
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

/// The unweighted "content band" track — one per band at most (see
/// [`ratio_vars`]). Equal tracks for everything was how a phone shell became
/// three giant bands — topbar, list and tabbar at a third each — which is the
/// layout that sent the model escaping to `body_html` with `escape:
/// "phone-frame"`.
const FILL_TRACK: &str = "minmax(0, 1fr)";

/// `ratio` weights become grid tracks. Without weights the tracks follow the
/// children: chrome, controls and compact widgets hug their content, and a
/// single content band fills what is left.
fn ratio_vars(
    ratio: &[String],
    children: &[MockNode],
    vertical: bool,
) -> Vec<(&'static str, String)> {
    if ratio.is_empty() {
        let mut tracks = children
            .iter()
            .map(|child| track_of(child, vertical))
            .collect::<Vec<_>>();
        // Several content bands in one `rows` stack all hug. Equal `1fr` rows
        // in a content-sized grid do not "share what is left" — they all grow
        // to the tallest band's content (the table band made nav and title
        // bands table-tall; session 20261023_41c0f4df). The lone content band
        // keeps the fill track so a fixed-height shell (phone) is still
        // absorbed by the content, not left with a gap under the chrome.
        if vertical && tracks.iter().filter(|t| t.as_str() == FILL_TRACK).count() > 1 {
            for track in &mut tracks {
                if track == FILL_TRACK {
                    *track = "auto".to_string();
                }
            }
        }
        return vec![("ratio", tracks.join(" "))];
    }
    // `minmax(0, Nfr)` rather than plain `Nfr`: a bare `fr` track carries an
    // implicit `min-content` floor, so three switch tiles could not shrink
    // below their labels and padding and pushed the whole phone body wider
    // than its shell.
    let tracks = ratio
        .iter()
        .map(|weight| format!("minmax(0, {weight}fr)"))
        .collect::<Vec<_>>()
        .join(" ");
    vec![("ratio", tracks)]
}

/// Track for one child of a `rows`/`cols` without `ratio`.
///
/// A declared `w`/`h` share wins outright — `h: 18` in a `rows` stack means
/// "this row is 18% of the band", so the share has to be the track. A
/// percentage height on a box inside an auto-sized track is cyclic and
/// silently does nothing.
///
/// Otherwise `vertical` is the band's own axis: in a `rows` stack the track is
/// height, where chrome, controls and compact widgets (a 34px sparkline does
/// not become a band) take what they need; in a `cols` band it is width, where
/// only side panels hug — a control that hugs its column shrinks to one CJK
/// character per line.
fn track_of(node: &MockNode, vertical: bool) -> String {
    let share = match node {
        MockNode::Region { w, h, .. } => {
            if vertical {
                h.as_deref()
            } else {
                w.as_deref()
            }
        }
        _ => None,
    };
    if let Some(share) = share {
        return proportion(share);
    }
    let hug = match node {
        MockNode::Sidebar { .. } | MockNode::Nav { .. } => !vertical,
        MockNode::Topbar { .. }
        | MockNode::Tabbar { .. }
        | MockNode::Toolbar { .. }
        | MockNode::Dialog { .. }
        | MockNode::Text { .. }
        | MockNode::ListRow { .. }
        | MockNode::Switch { .. }
        | MockNode::Segment { .. }
        | MockNode::Input { .. }
        | MockNode::Button { .. }
        | MockNode::Swatches { .. }
        | MockNode::Hero { .. }
        | MockNode::Sparkline { .. }
        | MockNode::Ring { .. }
        | MockNode::Bars { .. } => vertical,
        // Layout containers and data widgets: they are the content — the one
        // band that fills a fixed-height shell when the stack has just one of
        // them (see `ratio_vars` for the several-content case).
        _ => false,
    };
    String::from(if hug { "auto" } else { FILL_TRACK })
}

/// `NN%` / `NN` / `N.N` → a percentage the stylesheet can use as a size.
fn proportion(value: &str) -> String {
    let trimmed = value.trim();
    let number = trimmed.strip_suffix('%').unwrap_or(trimmed);
    format!("{number}%")
}

/// The wire kind name (`box`, not `Region`) — read from the same serde tag the
/// parser used, so the two can never drift.
fn node_kind(node: &MockNode) -> Option<String> {
    serde_json::to_value(node)
        .ok()?
        .get("kind")?
        .as_str()
        .map(str::to_string)
}

/// One antd island: the div the bundled runtime hydrates (`runtime/src/main.jsx`
/// mounts every `.m-antd-slot[data-antd]`). The payload is the binding table's
/// component name plus resolved props — the JS adapter never sees the mock
/// schema, and the layout tree never sees the library. `None` means greybox.
fn antd_slot(node: &MockNode, ctx: &RenderCtx) -> Option<String> {
    if !ctx.antd {
        return None;
    }
    let kind = node_kind(node)?;
    let component = crate::antd_map::component_for(&kind)?;
    let props = slot_props(node, ctx)?;
    let emph = node_id(node)
        .map(|id| ctx.emph.contains(id))
        .unwrap_or(false);
    let payload = serde_json::json!({
        "kind": kind,
        "component": component,
        "props": props,
        "emph": emph,
    });
    let class = if emph {
        "m-antd-slot m-emph"
    } else {
        "m-antd-slot"
    };
    Some(format!(
        "<div class=\"{class}\" data-antd-kind=\"{kind}\" data-antd='{}'></div>",
        escape_attr(&payload.to_string()),
    ))
}

/// Recipe input for one island: the node's fields with `content` overrides
/// applied exactly where the greybox renderer applies them (the primary copy
/// slot), so the two faces never disagree on what the user meant to say.
/// `None` when the node's shape has no honest island form (a `dialog` carrying
/// a nested body node — the body is mock structure, not card content).
fn slot_props(node: &MockNode, ctx: &RenderCtx) -> Option<serde_json::Value> {
    use serde_json::json;

    let override_text = node_id(node).and_then(|id| ctx.content.get(id)).cloned();
    let text_of = |own: &str| override_text.clone().unwrap_or_else(|| own.to_string());
    Some(match node {
        MockNode::Input { text, .. } | MockNode::Button { text, .. } => json!({
            "text": text.as_deref().map(text_of),
        }),
        MockNode::Nav { items, .. }
        | MockNode::Sidebar { items, .. }
        | MockNode::Toolbar { items, .. } => json!({
            "items": items,
        }),
        MockNode::Kpi {
            label,
            value,
            delta,
            ..
        } => json!({
            "label": label,
            "value": text_of(value),
            "delta": delta,
        }),
        MockNode::Hero { text, .. } => json!({
            "text": text_of(text),
        }),
        MockNode::Ring { pct, label, .. } => json!({
            "pct": pct,
            "label": label.as_deref().map(text_of),
        }),
        MockNode::Bars { items, .. } => json!({
            "items": items
                .iter()
                .map(|item| json!({ "label": item.label, "value": item.value }))
                .collect::<Vec<_>>(),
        }),
        MockNode::Dialog {
            title,
            text,
            body,
            actions,
            ..
        } => {
            if body.is_some() {
                return None;
            }
            // Greybox semantics: the message is the primary slot; the title is
            // rewritten only when there is no message to carry the override.
            json!({
                "title": if text.is_some() { title.clone() } else { text_of(title) },
                "text": text.as_deref().map(text_of),
                "actions": actions,
            })
        }
        MockNode::ListRow {
            title,
            note,
            meta,
            chevron,
            avatar,
            ..
        } => json!({
            "title": text_of(title),
            "note": note,
            "meta": meta,
            "chevron": chevron,
            "avatar": avatar,
        }),
        MockNode::Switch { label, on, .. } => json!({
            "label": label.as_deref().map(text_of),
            "on": on,
        }),
        MockNode::Segment { items, active, .. } => json!({
            "items": items,
            "active": active,
        }),
        MockNode::Table { cols, rows, .. } => json!({
            "cols": cols,
            "rows": rows,
        }),
        MockNode::Tree { items, .. } => json!({
            "items": items
                .iter()
                .map(|item| json!({ "label": item.label, "depth": item.depth }))
                .collect::<Vec<_>>(),
        }),
        // Mapped kinds only reach this function (`antd_slot` looks the kind
        // up first); a new map entry without a recipe arm falls back to
        // greybox rather than rendering a hollow island.
        _ => return None,
    })
}

pub fn node_id(node: &MockNode) -> Option<&str> {
    match node {
        MockNode::Frame { id, .. }
        | MockNode::Cols { id, .. }
        | MockNode::Rows { id, .. }
        | MockNode::Stack { id, .. }
        | MockNode::Region { id, .. }
        | MockNode::Nav { id, .. }
        | MockNode::Sidebar { id, .. }
        | MockNode::Input { id, .. }
        | MockNode::Button { id, .. }
        | MockNode::Swatches { id, .. }
        | MockNode::Text { id, .. }
        | MockNode::Placeholder { id, .. }
        | MockNode::Kpi { id, .. }
        | MockNode::Hero { id, .. }
        | MockNode::Sparkline { id, .. }
        | MockNode::Ring { id, .. }
        | MockNode::Bars { id, .. }
        | MockNode::Area { id, .. }
        | MockNode::Viz { id, .. }
        | MockNode::PhoneShell { id, .. }
        | MockNode::Topbar { id, .. }
        | MockNode::Tabbar { id, .. }
        | MockNode::Toolbar { id, .. }
        | MockNode::Dialog { id, .. }
        | MockNode::ListRow { id, .. }
        | MockNode::Switch { id, .. }
        | MockNode::Segment { id, .. }
        | MockNode::Table { id, .. }
        | MockNode::Tree { id, .. } => id.as_deref(),
    }
}

/// Collect every node id in one tree. Used by validation to reject ambiguous
/// or dangling `emph` / `content` references.
pub fn collect_ids(node: &MockNode, out: &mut HashSet<String>) {
    if let Some(id) = node_id(node) {
        out.insert(id.to_string());
    }
    match node {
        MockNode::Frame { body, .. }
        | MockNode::PhoneShell { body, .. }
        | MockNode::Dialog {
            body: Some(body), ..
        } => collect_ids(body, out),
        MockNode::Cols { children, .. } | MockNode::Rows { children, .. } => {
            for child in children {
                collect_ids(child, out);
            }
        }
        MockNode::Stack { child, .. } => collect_ids(child, out),
        _ => {}
    }
}
