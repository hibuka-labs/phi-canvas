//! Frame assembly: template assets compiled into the binary plus the
//! host-rendered options panel.
//!
//! Rendering contract:
//! - envelope metadata (title / subtitle / labels / notes) is HTML-escaped,
//! - `body_html` is spliced in verbatim (it is model-authored markup — that is
//!   the point of the body), but only ever after every template marker has
//!   been filled, so it cannot re-trigger substitution,
//! - the session token is injected into the page at serve time and nowhere
//!   else (never on disk, never in a URL).

use serde_json::Value;

use crate::envelope::ScreenEnvelope;

pub const FRAME_HTML: &str = include_str!("assets/frame.html");
pub const COMPARE_HTML: &str = include_str!("assets/compare.html");
pub const COMPONENTS_CSS: &str = include_str!("assets/components.css");
pub const HELPER_JS: &str = include_str!("assets/helper.js");
pub const COMPARE_JS: &str = include_str!("assets/compare.js");
/// The bundled React+antd island runtime (built from `runtime/`, version-pinned
/// with the crate). Served only to pages whose body actually carries
/// `m-antd-slot` islands — see [`scripts_for`].
pub const ANTD_RUNTIME_JS: &str = include_str!("assets/antd-runtime.js");

/// The runtime script tag when the page carries antd islands, else nothing —
/// a wireframe page must stay exactly the page it was.
pub fn scripts_for(has_islands: bool) -> &'static str {
    if has_islands {
        r#"<script src="/assets/antd-runtime.js"></script>"#
    } else {
        ""
    }
}

/// One design screen in the session's version history (gate 2 ③). Interstitial
/// screens are excluded before they get here: the rail is the design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub seq: u64,
    pub title: String,
}

/// How two versions are laid out beside each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    /// Two columns, whole screens at once.
    SideBySide,
    /// One stack with a draggable wipe (slider).
    Wipe,
}

/// Fill `{{MARKER}}` placeholders in one left-to-right pass. Output is never
/// rescanned, so substituted values may contain `{{` without consequence.
fn fill_markers(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 512);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = &after[..end];
                match vars.iter().find(|(key, _)| *key == name) {
                    Some((_, value)) => out.push_str(value),
                    None => out.push_str(&rest[start..start + 2 + end + 2]),
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Minimal HTML text escaping for untrusted envelope metadata.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Render one screen: frame + escaped metadata + verbatim body + options panel.
///
/// `versions` is the design history for the version rail (gate 2 ③). Passing
/// fewer than two entries renders no rail — a one-screen session has nothing
/// to look back at yet.
///
/// `live` is whether this screen is the one currently on offer. A historical
/// version (the rail is for looking) renders its question as a record: no
/// clickable options, no pick actions, and the helper leaves clicks alone.
///
/// `prior_pick` is what the user chose on an *earlier* screen. The page
/// reloads on every screen change, so the click's highlight is gone with the
/// old page — the receipt is rendered here so the next screen still says it.
pub fn render_screen(
    env: &ScreenEnvelope,
    seq: u64,
    token: &str,
    versions: &[VersionEntry],
    live: bool,
    prior_pick: Option<&str>,
) -> String {
    let subtitle = match &env.subtitle {
        Some(text) => format!("<p class=\"subtitle\">{}</p>", escape_html(text)),
        None => String::new(),
    };
    let indicator = if !live {
        "查看中 · 选择请回到当前屏".to_string()
    } else if env.is_display_only() {
        "展示中 · 回到终端继续讨论".to_string()
    } else if env.multiselect {
        "点选后按「确认」提交，或「跳过」".to_string()
    } else {
        "点击选择，然后回到终端继续".to_string()
    };
    let indicator = match prior_pick {
        Some(label) if live => format!("上一问已选：{} ✓ · {indicator}", escape_html(label)),
        _ => indicator,
    };

    let (head, tail) = FRAME_HTML
        .split_once("{{BODY}}")
        .expect("frame template must contain a {{BODY}} marker");
    let vars = [
        ("TOKEN", token),
        ("SEQ", &seq.to_string()),
        ("LIVE", if live { "1" } else { "0" }),
        ("BODYCLASS", if live { "" } else { " class=\"view-only\"" }),
        ("VERSIONS", &render_versions(versions, seq)),
        ("TITLE", &escape_html(&env.title)),
        ("SUBTITLE", &subtitle),
        ("REFERENCE", &render_reference(env)),
        ("OPTIONS", &render_options(env, live)),
        ("INDICATOR", indicator.as_str()),
        (
            "SCRIPTS",
            scripts_for(env.body_html.contains("m-antd-slot")),
        ),
    ];
    // A look-back keeps the record, not the affordance: every pick marker in
    // the body (spec variant cards and `body_html` spatial picks alike) is
    // renamed out of the clickable vocabulary. The helper also refuses to
    // post on a not-live page, and the server only takes a choice for the
    // screen on offer — three layers, this one is the visible one.
    let body = if live {
        env.body_html.clone()
    } else {
        env.body_html.replace("data-choice=\"", "data-view=\"")
    };
    format!(
        "{}{}{}",
        fill_markers(head, &vars),
        body,
        fill_markers(tail, &vars)
    )
}

/// The version rail: every design screen so far, this one marked, plus the way
/// into the compare view. Hidden until there is more than one version — the
/// rail is history, and history of one is just the present.
fn render_versions(versions: &[VersionEntry], current: u64) -> String {
    if versions.len() < 2 {
        return String::new();
    }
    let mut chips = String::new();
    for version in versions {
        let class = if version.seq == current {
            "version current"
        } else {
            "version"
        };
        chips.push_str(&format!(
            r#"<a class="{class}" href="/screens/{seq}" title="{title}">{seq} · {title}</a>"#,
            class = class,
            seq = version.seq,
            title = escape_html(&version.title),
        ));
    }
    let (before, after) = default_compare_pair(versions, current);
    format!(
        r#"<nav class="versions" aria-label="版本">
  <span class="versions-label">版本</span>
  {chips}
  <a class="versions-compare" href="/compare/{before}/{after}">对比</a>
</nav>"#,
        chips = chips,
        before = before,
        after = after,
    )
}

/// The pair the compare view opens on: this screen and the version before it,
/// so "what changed" is one click away from the screen that changed.
fn default_compare_pair(versions: &[VersionEntry], current: u64) -> (u64, u64) {
    if let Some(index) = versions.iter().position(|version| version.seq == current) {
        if index > 0 {
            return (versions[index - 1].seq, current);
        }
        if index + 1 < versions.len() {
            return (current, versions[index + 1].seq);
        }
    }
    (
        versions
            .first()
            .map(|version| version.seq)
            .unwrap_or(current),
        versions
            .last()
            .map(|version| version.seq)
            .unwrap_or(current),
    )
}

/// The reference image block, or nothing. `resolved` is the host's doing (a
/// copied file served from `/screens/{seq}/reference`, or a URI the page loads
/// itself) — a reference that never resolved is left off the screen entirely
/// rather than shown as a broken image.
fn render_reference(env: &ScreenEnvelope) -> String {
    let Some(reference) = env.reference.as_ref() else {
        return String::new();
    };
    let Some(src) = reference.resolved.as_deref() else {
        return String::new();
    };
    let caption = match &reference.caption {
        Some(text) => format!("<figcaption>{}</figcaption>", escape_html(text)),
        None => String::new(),
    };
    format!(
        r#"<figure class="reference">
  <img src="{src}" alt="参考图" loading="lazy">
  {caption}
</figure>"#,
        src = escape_html(src),
        caption = caption,
    )
}

/// Host-rendered options panel — the canonical click surface. `body_html` may
/// also carry `data-choice="<id>"` for spatial picks, but the model is told
/// not to duplicate this panel.
///
/// On a screen that is not on offer (`live == false`) the same question is
/// drawn as a record: no `data-choice`, no pick actions — a look back at what
/// was asked is not a second chance to answer it.
fn render_options(env: &ScreenEnvelope, live: bool) -> String {
    if env.is_display_only() {
        return String::new();
    }
    let mut items = String::new();
    for option in &env.options {
        let letter = option
            .id
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
            .unwrap_or('?');
        let note = match &option.note {
            Some(text) => format!("<p>{}</p>", escape_html(text)),
            None => String::new(),
        };
        let pick = if live {
            format!(
                " data-choice=\"{id}\" role=\"button\" tabindex=\"0\"",
                id = escape_html(&option.id)
            )
        } else {
            String::new()
        };
        items.push_str(&format!(
            r#"<div class="option"{pick}>
  <div class="letter">{letter}</div>
  <div class="content"><h3>{label}</h3>{note}</div>
</div>"#,
            pick = pick,
            letter = letter,
            label = escape_html(&option.label),
            note = note,
        ));
    }

    if !live {
        return format!(
            "<div class=\"options\" data-multiselect=\"{multi}\">\n{items}</div>",
            multi = env.multiselect,
            items = items
        );
    }
    let confirm = if env.multiselect {
        r#"<button class="btn" data-action="confirm">确认</button>"#
    } else {
        ""
    };
    format!(
        r#"<div class="options" data-multiselect="{multi}">
{items}</div>
<div class="actions">{confirm}<button class="btn btn-ghost" data-action="skip">跳过</button></div>"#,
        multi = env.multiselect,
        items = items,
        confirm = confirm,
    )
}

/// A frame with a single notice card (empty canvas, farewell screens).
///
/// `live` is whether the page chases the current screen (helper.js reloads on
/// a seq change only when the meta says live). The waiting notice must — the
/// first push usually lands after the browser is already open, and a page
/// that does not chase leaves the user staring at "等待第一屏" while the
/// terminal insists the screen is up. Farewells and the compare stub are ends
/// in themselves: reloading them would only reset the view.
pub fn render_notice(title: &str, message: &str, token: &str, seq: u64, live: bool) -> String {
    let (head, tail) = FRAME_HTML
        .split_once("{{BODY}}")
        .expect("frame template must contain a {{BODY}} marker");
    let body = format!(
        r#"<div class="notice"><h2>{title}</h2><p>{message}</p></div>"#,
        title = escape_html(title),
        message = escape_html(message),
    );
    let vars = [
        ("TOKEN", token),
        ("SEQ", &seq.to_string()),
        ("LIVE", if live { "1" } else { "0" }),
        ("BODYCLASS", " class=\"view-only\""),
        ("VERSIONS", ""),
        ("TITLE", &escape_html(title)),
        ("SUBTITLE", ""),
        ("REFERENCE", ""),
        ("OPTIONS", ""),
        ("INDICATOR", "回到终端继续讨论"),
        ("SCRIPTS", ""),
    ];
    format!(
        "{}{}{}",
        fill_markers(head, &vars),
        body,
        fill_markers(tail, &vars)
    )
}

// ── version comparison (gate 2 ③) ─────────────────────────────────────────

/// The compare page: pickers over the version history, the two screens side by
/// side or wiped over each other, and — where both sides carry structure — a
/// field-level list of what actually changed.
pub fn render_compare(
    before: (u64, &ScreenEnvelope),
    after: (u64, &ScreenEnvelope),
    versions: &[VersionEntry],
    mode: CompareMode,
) -> String {
    let stage = match mode {
        CompareMode::SideBySide => side_by_side(before, after),
        CompareMode::Wipe => wipe_over(before, after),
    };
    let islands =
        before.1.body_html.contains("m-antd-slot") || after.1.body_html.contains("m-antd-slot");
    let vars: [(&str, &str); 6] = [
        ("PICK_A", &picker(versions, before.0)),
        ("PICK_B", &picker(versions, after.0)),
        ("MODES", &mode_links(before.0, after.0, mode)),
        ("DIFF", &render_changes(before.1, after.1)),
        ("COMPARE", &stage),
        ("SCRIPTS", scripts_for(islands)),
    ];
    fill_markers(COMPARE_HTML, &vars)
}

fn picker(versions: &[VersionEntry], selected: u64) -> String {
    versions
        .iter()
        .map(|version| {
            let mark = if version.seq == selected {
                " selected"
            } else {
                ""
            };
            format!(
                r#"<option value="{seq}"{mark}>{seq} · {title}</option>"#,
                seq = version.seq,
                mark = mark,
                title = escape_html(&version.title),
            )
        })
        .collect()
}

/// Mode is a link, not a widget: the page is one GET, and the browser's back
/// button then walks the views the user tried.
fn mode_links(before: u64, after: u64, mode: CompareMode) -> String {
    let link = |label: &str, active: CompareMode, param: &str| {
        let class = if mode == active {
            "mode current"
        } else {
            "mode"
        };
        format!(r#"<a class="{class}" href="/compare/{before}/{after}/{param}">{label}</a>"#)
    };
    format!(
        "{}{}",
        link("并排", CompareMode::SideBySide, "split"),
        link("滑块", CompareMode::Wipe, "slide")
    )
}

fn side_by_side(before: (u64, &ScreenEnvelope), after: (u64, &ScreenEnvelope)) -> String {
    format!(
        r#"<div class="compare split">
  <div class="pane">{}</div>
  <div class="pane">{}</div>
</div>"#,
        pane(before),
        pane(after),
    )
}

/// The slider view: both bodies in one stack, the older one clipped to the
/// left of the handle. A grid cell holds both, so the stack is as tall as the
/// taller version and nothing is cropped away.
fn wipe_over(before: (u64, &ScreenEnvelope), after: (u64, &ScreenEnvelope)) -> String {
    let (before_seq, before_title) = head_of(before);
    let (after_seq, after_title) = head_of(after);
    format!(
        r#"<div class="compare wipe">
  <div class="wipe-heads">
    <div class="wipe-head">{before_seq} · {before_title}</div>
    <div class="wipe-head">{after_seq} · {after_title}</div>
  </div>
  <div class="wipe-stack" id="wipe-stack">
    <div class="section body-region wipe-base">{after_body}</div>
    <div class="section body-region wipe-cover">{before_body}</div>
  </div>
  <input class="wipe-range" id="wipe-range" type="range" min="0" max="100" value="50" aria-label="拖动对比两版">
</div>"#,
        before_seq = before_seq,
        before_title = before_title,
        after_seq = after_seq,
        after_title = after_title,
        before_body = before.1.body_html,
        after_body = after.1.body_html,
    )
}

fn pane((seq, env): (u64, &ScreenEnvelope)) -> String {
    let (head_seq, head_title) = head_of((seq, env));
    format!(
        r#"<div class="pane-head">{head_seq} · {head_title}</div>
<div class="section body-region">{body}</div>"#,
        head_seq = head_seq,
        head_title = head_title,
        body = env.body_html,
    )
}

fn head_of((seq, env): (u64, &ScreenEnvelope)) -> (String, String) {
    (seq.to_string(), escape_html(&env.title))
}

/// Field-level change lines between two screens: what actually changed, in the
/// structured fields. `body_html` is left out on purpose — it is what the
/// compare stage already shows, and a line diff of markup is noise.
pub fn change_lines(before: &ScreenEnvelope, after: &ScreenEnvelope) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(before), Some(after)) = (metadata(before), metadata(after)) else {
        return out;
    };
    diff_values("", &before, &after, &mut out);
    out
}

/// The envelope as a value, minus the body.
fn metadata(env: &ScreenEnvelope) -> Option<serde_json::Value> {
    let mut clone = env.clone();
    clone.body_html = String::new();
    serde_json::to_value(&clone).ok()
}

fn diff_values(path: &str, before: &Value, after: &Value, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            for (key, value) in before {
                let child = join_path(path, key);
                match after.get(key) {
                    Some(other) => diff_values(&child, value, other, out),
                    None => out.push(format!("{child}: 没了")),
                }
            }
            for (key, value) in after {
                if !before.contains_key(key) {
                    out.push(format!(
                        "{}: 新增 {}",
                        join_path(path, key),
                        show_value(value)
                    ));
                }
            }
        }
        (Value::Array(before), Value::Array(after)) => {
            for (index, (value, other)) in before.iter().zip(after.iter()).enumerate() {
                diff_values(&format!("{path}[{index}]"), value, other, out);
            }
            if before.len() != after.len() {
                out.push(format!("{path}: {} 项 → {} 项", before.len(), after.len()));
            }
        }
        _ if before != after => {
            out.push(format!(
                "{path}: {} → {}",
                show_value(before),
                show_value(after)
            ));
        }
        _ => {}
    }
}

fn join_path(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn show_value(value: &Value) -> String {
    const MAX: usize = 48;
    let text = match value {
        Value::String(text) => format!("{text:?}"),
        other => other.to_string(),
    };
    if text.chars().count() > MAX {
        let head: String = text.chars().take(MAX).collect();
        format!("{head}…")
    } else {
        text
    }
}

fn render_changes(before: &ScreenEnvelope, after: &ScreenEnvelope) -> String {
    let lines = change_lines(before, after);
    if lines.is_empty() {
        return String::new();
    }
    let items: String = lines
        .iter()
        .map(|line| format!("<li>{}</li>", escape_html(line)))
        .collect();
    format!(
        r#"<details class="changes" open><summary>改了什么 · {} 项</summary><ul>{}</ul></details>"#,
        lines.len(),
        items
    )
}
