# GPUI conversion design

## Goal

Rebuild Blink as a pure-Rust desktop app on GPUI + GPUI Kit (`gpui-kit`
0.7) with 1-to-1 feature parity with the Tauri + Vue app on `main`. The two
apps must be comparable side by side: same features, same product copy, same
layout, same keyboard shortcuts, same saved data format.

## Constraints

- One UI dependency: `gpui-kit`. GPUI is `use gpui_kit::*;`, components are
  `gpui_kit::component::*`. Never invent an API: read the source in
  `~/.cargo/registry/src/index.crates.io-*/gpui-component-0.7.0/src` and
  `gpui-base-0.7.0/src`, and the guides in the upstream checkout
  (`skills/gpui-kit/references/*.md`, `skills/gpui-kit-design-guides`).
- Product copy (labels, tooltips, errors, status text) is copied word for
  word from the Vue components.
- `DESIGN.md` still applies: tokens only, amber accent used sparingly,
  40 px title bar, 24 px status bar, 6 px gaps on a darker frame, 8 px card
  radius, 4 px control radius, system sans for chrome, monospace for data.
- Saved data: the GPUI app reads and writes the same `workspace-v1.json`
  format (version 4). It stores data in
  `~/Library/Application Support/com.kyle.blink.gpui` and copies the Tauri
  app's files from `com.kyle.blink` once on first launch. It never modifies
  the Tauri app's files.

## Crates

```
crates/
  blink-core/   UI-free logic. One module per src/lib module, plus:
    engine/     Former Tauri backend: HTTP, SSE, WebSocket, cookies,
                file grants, response store, snapshot storage, paths,
                window state, Ghostty themes. Owns a tokio runtime; its
                futures run on any executor.
    workspace_state.rs  Pure port of useWorkspaceState (+ setters).
    runner.rs   Pure parts of useRequestRunner: apply a result to a
                session, history, checks, captures, staleness.
  blink/        The GPUI app.
```

Tests from `src/lib/__test__` and `src/composables/__test__` move to Rust
unit tests next to the code they cover.

## App structure (`crates/blink/src`)

| File                       | Replaces                                  |
| -------------------------- | ----------------------------------------- |
| `main.rs`                  | `main.ts`, Tauri `run()`                  |
| `actions.rs`               | global shortcuts in `App.vue`             |
| `theme.rs`                 | `style.css`, `useTheme`, `theme.ts`       |
| `store.rs`                 | `useWorkspaceState` + persistence + undo  |
| `runner.rs`                | `useRequestRunner`, `useWebSocket`        |
| `ui/app.rs`                | `App.vue` shell                           |
| `ui/title_bar.rs`          | App.vue header                            |
| `ui/command_center.rs`     | `CommandCenter.vue`                       |
| `ui/browser.rs`            | `RequestBrowser.vue`, `GroupMenu*.vue`, `TreeGuides.vue`, `DragPreview.vue` |
| `ui/tabs.rs`               | `RequestTabs.vue`                         |
| `ui/request_pane.rs`       | `RequestWorkspace.vue`                    |
| `ui/request_editor.rs`     | `RequestEditor.vue`                       |
| `ui/key_value_editor.rs`   | `KeyValueEditor.vue`                      |
| `ui/token_input.rs`        | `TokenInput.vue`                          |
| `ui/checks_editor.rs`      | `ChecksEditor.vue`                        |
| `ui/response_panel.rs`     | `ResponsePanel.vue`, `EventList.vue`, `TimingCard.vue` |
| `ui/json_tree.rs`          | `JsonTreeView.vue`                        |
| `ui/code_view.rs`          | `CodeView.vue`, `CodeEditor.vue`          |
| `ui/history_view.rs`       | `HistoryView.vue`                         |
| `ui/websocket_panel.rs`    | `WebSocketPanel.vue`                      |
| `ui/settings_window.rs`    | `ApplicationSettingsDialog.vue`, `ThemeSettings.vue` (own window) |
| `ui/group_settings.rs`     | `GroupSettingsDialog.vue`, `EnvironmentTokensEditor.vue` |
| `ui/environment_badge.rs`  | `EnvironmentBadge.vue`                    |
| `ui/cookies_dialog.rs`     | `CookiesDialog.vue`                       |
| `ui/status_bar.rs`         | App.vue footer                            |
| `ui/storage_notice.rs`     | `WorkspaceStorageNotice.vue`              |
| `ui/widgets.rs`            | `HelpTooltip.vue`, method label, shared bits |

## State ownership

- `Store` (`Entity<Store>`) is the single owner of domain state: the
  `blink_core::workspace_state::Workspace`, the `Engine`, the save pipeline
  (debounced, revisioned, atomic), the deletion undo window, and in-flight
  sends. Views mutate it only through `Store` methods and re-render by
  observing it. `Store` emits `StoreEvent`s for things views react to (for
  example, a draft replaced by cURL import or format).
- Each request has one `RequestPane` entity for its whole lifetime (created
  when the session appears, dropped when it is deleted). It owns the input
  states (URL, body editor, key/value rows) and view-only state (scroll,
  find). Inactive panes stay alive and keep their state, matching "keep
  inactive and closed panels mounted".
- Text inputs sync one way at a time: user edits write to the draft through
  `Store`; programmatic draft replacements set the input value and are not
  echoed back.
- App chrome state (Browser visibility, focused group, selection, open
  dialogs) lives on the root `BlinkApp` view.

## Mapping notes

- Tooltips: `.tooltip(...)`. Context menus: gpui-component `context_menu`.
  Dropdowns: `DropdownMenu`/`PopupMenu`. Dialogs: `window.open_dialog`,
  confirmations: `window.open_alert_dialog`.
- Code and body editors: `Editor`/`EditorState` with tree-sitter (JSON,
  GraphQL, bash, JavaScript, Python, Go, Rust).
- Large responses: virtualized lists (`v_virtual_list`) so a 16 MiB body
  stays responsive.
- File pickers: `cx.prompt_for_paths` / `cx.prompt_for_new_path`, then
  `Engine::grant_file` for picked request files.
- Window size and position: `engine::window_state`, restored at launch and
  saved on move, resize, blur, and quit.
- Quit: `on_window_should_close` / `on_app_quit` wait for the latest save,
  as the Tauri app does.
- Zoom: scale the root rem size (`window.set_rem_size`).

## Verification

1. `cargo test --workspace` passes, including every ported TS test.
2. `cargo clippy --workspace -- -D warnings` passes.
3. A README feature checklist is run by hand against both apps.
