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

mod antd;
mod nodes;

use std::collections::{BTreeMap, HashSet};

use crate::assets::escape_html;
use crate::envelope::OptionItem;
use crate::spec::{Fidelity, MockNode, ScreenSpec, Surface, Theme, Variant};

pub use antd::{collect_ids, node_id};
use nodes::render_node;

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
pub(crate) struct RenderCtx<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) content: &'a BTreeMap<String, String>,
    pub(crate) emph: &'a HashSet<String>,
    /// The antd face is on (see [`antd_path`]) — mapped leaves render as
    /// hydrate slots instead of `m-*` markup.
    pub(crate) antd: bool,
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
pub(crate) fn escape_attr(value: &str) -> String {
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
pub(crate) fn style_attr(vars: &[(&str, String)]) -> String {
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
pub(crate) enum TrackAxis {
    /// `rows`: tracks are heights, so `h` is the share.
    Vertical,
    /// `cols`: tracks are widths, so `w` is the share.
    Horizontal,
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
pub(crate) fn ratio_vars(
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
pub(crate) fn proportion(value: &str) -> String {
    let trimmed = value.trim();
    let number = trimmed.strip_suffix('%').unwrap_or(trimmed);
    format!("{number}%")
}
