# phi-canvas

Design canvas server for the phi family — the local visual companion for LLM
coding agents. An agent pushes a screen (layout mockups, option sets,
comparisons) to `127.0.0.1`; the user looks at the browser and clicks; the
picks come back as structured events. One binary-worth of dependencies:
`tokio` + `axum` + `serde` + `getrandom` + `thiserror`. No node, no python.

**No product semantics.** phi-canvas doesn't know what a consent gate, a
click mailbox, or a synthesized user message is — those live in the host
(phimint). What lives here is the mechanism: the loopback HTTP server, the
session-token / Origin / Host validation, screen persistence, the three tool
schemas with their locked return texts, and the frame template.

## Security posture

A canvas server is an answer-injection channel: any webpage can POST a forged
"click" to `localhost:PORT`, and a click is a design answer. Three rules:

- binds `127.0.0.1` only — never `0.0.0.0`
- per-canvas 32-byte CSPRNG session token, injected into the served page via
  `<meta>` at request time; **never on disk, never in a URL query**
- `POST /events` requires `X-Canvas-Token` (constant-time compare); foreign
  `Host` / `Origin` are rejected; any query string is rejected

## Tools

| tool | return |
|---|---|
| `canvas_open` | `canvas: http://127.0.0.1:PORT \| state: open \| awaiting consent` (or `\| browser launched`) |
| `canvas_push` (options) | `canvas: http://127.0.0.1:PORT \| screen: <title> \| options: <N> \| awaiting selection` |
| `canvas_push` (display-only) | `canvas: http://127.0.0.1:PORT \| screen: <title> \| display only` |
| `canvas_close` | `canvas: closed` |

`canvas_push` carries a structured envelope — `title` / `subtitle` /
`options[]` / `body_html` / `multiselect`. The host renders the options panel
from the metadata; `body_html` is spliced in verbatim as the visual body.
`options: []` makes a display-only screen.

Opening the browser is host policy behind `BrowserLauncher`. An implicit open
(from a `canvas_push` with no live canvas) binds the server only — a browser
must never appear without the user's consent.

## Usage

```rust
use phi_canvas::{Canvas, CanvasConfig, ScreenEnvelope};
use serde_json::json;

let canvas = Canvas::open(CanvasConfig::new("/tmp/sess/canvas", "sess-1")).await?;
let env = ScreenEnvelope::parse(&json!({
    "title": "大屏顶部导航，哪个布局更专业？",
    "options": [
        { "id": "a", "label": "侧边栏导航" },
        { "id": "b", "label": "顶部横向导航" }
    ],
    "body_html": "<div class=\"mockup\"><div class=\"mock-body\">…</div></div>"
}))?;
let receipt = canvas.push(env).await?;
// host: subscribe to clicks, synthesize user messages, own the browser
let mut clicks = canvas.subscribe();
```

Or drive the three tools directly (`CanvasTools`), which is how the host
adapts them into its registry:

```rust
use phi_canvas::{CanvasTools, tool_descriptors};
use serde_json::json;

let tools = CanvasTools::new("/tmp/sess/canvas", "sess-1");
println!("{}", tools.open().await?);                 // awaiting consent / launched
println!("{}", tools.push(&json!({ "title": "…", "options": [] })).await?);
println!("{}", tools.close().await?);                // canvas: closed
let schemas = tool_descriptors();                    // name + description + schema
```

## Layout on disk (per session)

```
canvas/
  server-info        # session_id, url, port, started_at_ms — never the token
  events             # JSONL click stream
  screens/0001.html  # body fragment
  screens/0001.json  # envelope metadata (no body)
```

## Develop

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cargo run --example demo   # pushes three screens and opens the page
```

License: MIT
