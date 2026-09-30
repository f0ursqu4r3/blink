//! Blink request engine, workspace model, and import/export logic.
//!
//! Each module ports the `src/lib` module of the same name from the Vue app;
//! `engine` ports the former Tauri backend. Nothing here depends on the UI.

pub mod engine;
pub mod ids;
pub mod model;

pub mod authorization;
pub mod checks;
pub mod codegen;
pub mod command_center;
pub mod curl_import;
pub mod definitions;
pub mod diff;
pub mod drag_drop;
pub mod environments;
pub mod ghostty;
pub mod graphql;
pub mod graphql_schema;
pub mod groups;
pub mod history;
pub mod import;
pub mod interpolation;
pub mod jq;
pub mod json;
pub mod preferences;
pub mod request;
pub mod response_body;
pub mod response_content;
pub mod session;
pub mod session_curl;
pub mod shortcut;
pub mod sse;
pub mod text_location;
pub mod theme;
pub mod timing;
pub mod token_display;
pub mod token_hints;
pub mod transport_options;
pub mod tree_guides;
pub mod websocket_log;
pub mod workspace;
