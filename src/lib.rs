//! `phi-canvas` — design canvas server for the phi family.
//!
//! A leaf crate: process-local HTTP server that lets an agent present visual
//! design options in the user's browser and receive the user's picks as
//! structured events. No dependency on `phi-agent` / `phi-tui` /
//! `phi-kernel-tools`; the host embeds this crate and adapts the tools.
//!
//! Layering (design contract):
//! - Mechanism lives here: HTTP server, token/Origin/Host validation, screen
//!   persistence, lifecycle, frame template assets, tool schemas + return texts.
//! - Product semantics live in the host: click mailbox + message synthesis,
//!   consent gate, browser launch policy, status bar, waiting screens.

pub mod assets;
pub mod envelope;
pub mod event;
pub mod server;
pub mod store;
pub mod token;
pub mod tools;

pub use envelope::{EnvelopeError, OptionItem, ScreenEnvelope};
pub use event::{CanvasEvent, EventAction, EventInput};
pub use server::{Canvas, CanvasConfig, CanvasError, CloseReason, PushReceipt};
pub use store::{CanvasStore, ServerInfo, StoreError};
pub use token::SessionToken;
pub use tools::{
    BrowserLauncher, CanvasTools, LaunchDecision, TEXT_CLOSED, ToolCallError, ToolDescriptor,
    tool_descriptors,
};
