# Changelog

All notable changes to `phi-canvas` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-24

Initial release (MIT). Design canvas server for the phi family — the local
visual companion for LLM coding agents: an agent pushes a screen to
`127.0.0.1`, the user looks at the browser and clicks, the picks come back as
structured events.

### Added

- **Canvas server** — loopback-only (`127.0.0.1`) HTTP server with screen
  persistence, selection events, and the three tool schemas with locked return
  texts (`canvas_open` / `canvas_push` / `canvas_close`).
- **Screen spec vocabulary + renderer** — a structured screen/mockup spec
  (layout nodes, theme tokens) rendered into the served page, with
  `screen_file` round trip for large screens and live pages.
- **Version compare** — spec versions kept so a screen can be compared against
  an earlier revision.
- **Security posture** — per-canvas 32-byte CSPRNG session token injected via
  `<meta>` at request time (never on disk, never in a URL), constant-time
  `X-Canvas-Token` comparison on `POST /events`, foreign `Host`/`Origin`
  rejection, query strings rejected.
- **antd island sources** (`runtime/`) — the tracked antd component island
  sources behind the antd design floor.

### Fixed

- Bar widths draw from `--w` percentages with tolerant magnitude parsing.
