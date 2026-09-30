# Blink

Blink is a fast, private HTTP client for professional API work. It runs as a
native desktop app built with GPUI. It does not require an account or cloud
service. The local
Browser sidebar organizes the request tabs in the current session.

## Included

- Open independent request tabs with the `+` button. Each tab retains its
  draft, response, editor selection, wrapping, scroll position, and Browser
  group.
- Use the Browser sidebar to create, nest, collapse, rename, and delete groups.
  Move the active request to any group or back to Ungrouped.
- Choose **Focus** in a group's menu to show only that group, its child
  groups, and their tabs. Other tabs stay open but hidden. Press Escape or the
  × in the focus bar to show everything again. Opening a request from outside
  the group also ends the focus.
- Run requests in the background while working in another tab.
- Duplicate a complete draft without copying its response or sending it.
- See methods, endpoint paths, hosts, and request status in compact tabs.
- Send any HTTP method. The method field suggests `GET`, `POST`, `PUT`,
  `PATCH`, `DELETE`, `HEAD`, and `OPTIONS`, and accepts others such as `PURGE`.
- Edit query parameters, headers, and JSON, text, GraphQL, URL-encoded form,
  multipart form, or file request bodies. Multipart parts can be text or
  files.
  GraphQL bodies take a query and optional JSON variables. Format tidies JSON
  bodies, GraphQL queries, and GraphQL variables.
- Use Bearer tokens or Basic authentication.
- Export the request as cURL, JavaScript `fetch`, Python `requests`, Go
  `net/http`, HTTPie, or Rust `reqwest` code. The code includes any entered
  credentials and the timeout, redirect, TLS, and proxy settings.
- Paste a cURL command into the URL field to import it. Blink lists the
  options it ignored.
- Import OpenAPI 3 or Swagger 2 (JSON or YAML), Postman collections, and
  `.http` files from the Browser header or the command center. Each import
  becomes a group.
- Cancel a running request. The draft stays editable while it runs.
- Inspect response status, duration, headers, and body, including error responses.
  Hover the duration to see DNS, connect, wait, and download times.
- Find text in a response with a match count. Enter and Shift+Enter move
  between matches. The filter button shows only matching lines.
- Compare responses in the History view. Blink keeps the last 25 sends of each
  request.
- Give a root group environments, such as DEV and PROD. Group settings shows
  a token table with a column per environment; an empty cell uses the base
  value. Switch the environment from the badge beside the group name, the
  status bar shows it, and a protected environment asks before the first
  send after you switch to it. Nested groups follow their root group.
- Add assertions on status, time, size, headers, body text, or jq values in
  the request Tests tab. Captures save a response value as a token in the
  active environment of the request's root group, so values never cross
  environments. Without an environment they go to the root group's tokens,
  or to the global tokens for ungrouped requests.
- Watch `text/event-stream` responses as they arrive. Cancel stops the stream
  and keeps the events.
- Open a WebSocket with a `ws://` or `wss://` URL. Send and receive messages
  in the WebSocket panel.
- Keep cookies in one jar and send them with later requests. Manage them
  from Application Settings or the command center.
- Switch between pretty JSON and raw text. Large JSON numbers stay exact.
- Copy response bodies and toggle line wrapping.
- Send requests from the native engine, without browser CORS limits.
- Restore open tabs and application state after restart. Nothing is sent until
  you press Send.
- Distinguish a previous response from an edited, unsent draft.
- Place the request and response side by side or stack them. Use the layout
  button beside Settings in the title bar, or press `Cmd/Ctrl+\`. Blink
  restores the layout after a restart.
- Type `>` in the command center to run a command, such as send, copy as
  code, tab, response, zoom, and Browser commands. Matching is fuzzy.
- Zoom with `Cmd/Ctrl+=`, `Cmd/Ctrl+-`, and `Cmd/Ctrl+0`.
- Reopen a closed tab with `Cmd/Ctrl+Shift+T`. Undo a deleted request or group
  from the status bar or with `Cmd/Ctrl+Z` for 10 seconds.

Application Settings → Requests controls the timeout (default 30 s total,
10 s to connect), whether redirects are followed (default off; up to 20 hops),
and the inspection limit (default 4 MiB, up to 16 MiB). A body larger than the
inspection limit shows a truncated preview. Binary bodies show a summary in
place of the text. Use **Save response body…** to write the full body to a
file, up to 1 GiB. Stored bodies do not survive a restart. After a restart,
only complete text responses can be saved.

The same settings control TLS and proxies. Blink trusts the system
certificate store, so company and local development CAs work. **Verify TLS
certificates** is on by default; while it is off, the status bar shows TLS
VERIFY OFF. **Proxy URL** accepts `http`, `https`, or `socks5` URLs. Empty uses
the system proxy settings.

Blink reads a file for a file body or multipart file part only after you
pick it in its open dialog. It remembers picked files so saved
requests keep working after a restart. Uploads are limited to 1 GiB per file.

## Request tabs

The Browser tree holds every saved request. Tabs show only the requests that
are open. Closing a tab does not delete its request: select the request in the
Browser to open it again. An in-flight request keeps running after its tab
closes. To remove a request, choose **Delete** in its Browser context menu.
Deleting a request with content asks for confirmation by default. New tabs
never send automatically.

| Action                   | Shortcut                      |
| ------------------------ | ----------------------------- |
| New request tab          | `Cmd/Ctrl+T`                  |
| Duplicate active request | `Cmd/Ctrl+Shift+D`            |
| Close active tab         | `Cmd/Ctrl+W`                  |
| Next or previous request | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Send active request      | `Cmd/Ctrl+Enter`              |
| Cancel running request   | `Cmd/Ctrl+.`                  |
| Toggle pane layout       | `Cmd/Ctrl+\`                  |
| Search requests          | `Cmd/Ctrl+P`                  |
| Run a command            | `Cmd/Ctrl+Shift+P`            |
| Focus URL                | `Cmd/Ctrl+L`                  |
| Application settings     | `Cmd/Ctrl+,`                  |

In the request tab strip, use Left/Right, Home/End, and Delete to select or
close tabs. Escape dismisses the cURL preview.

## Browser groups

The Browser sidebar is the local tree of all requests. Select a request in the
tree to open it as a tab and make it active. Open a group's overflow menu and choose
**Move selection here** to move the selected requests. Use the Ungrouped move
action to remove group membership. On narrow windows, open the Browser with the
header's sidebar button.

Groups can nest to any depth. The Browser indents the first levels and keeps
later levels usable in the same compact tree. Use a group row to create a child,
rename it, collapse it, or delete it. Deleting a group does not discard requests:
its direct requests move to the parent, and its direct child groups are promoted
to that parent.

New and duplicated request tabs start in the active request's group. This makes
related endpoint work stay together without creating request history or sharing
it outside this device.

## Settings

Application settings control the default method, body mode, response formatting,
and line wrapping for new requests. Existing requests and duplicates keep their
values. Draft-close confirmation is a separate application-wide preference.

Group settings include a parent location, authorization, local tokens, and
defaults for a new request's method and initial URL. The nearest group override
wins; unset values inherit from parent groups, then application defaults. Use
the new-request action beside a group to start a request there. Moving an
existing request does not rewrite its method or URL.

Request fields can reference tokens:

| Syntax       | Resolves from                                      |
| ------------ | -------------------------------------------------- |
| `{{name}}`   | The nearest group token, then the workspace token  |
| `{{_.name}}` | The workspace token only                           |
| `{{!NAME}}`  | The `NAME` environment variable of the Blink process |

Environment values resolve in the desktop backend when a request is sent.
They are not shown in the editor or included in cURL exports. Token names
cannot start with `_` or `!`.

Secondary help appears in tooltips. Errors, effective authorization, storage
failures, and destructive confirmations remain visible.
Settings dialogs keep Save and Cancel visible while their contents scroll.
Escape closes a tooltip first, then the dialog. Cancel discards unsaved edits.

## Local state and privacy

Blink saves tab order, groups, group hierarchy, request membership, the active
tab, complete request drafts, responses, errors, editor tabs, Pretty/Raw mode,
wrapping, response scroll position, and request history. History keeps the
last 25 sends of each request, with bodies up to 64 KiB.
Blink also restores window size and position. Closing the window
or quitting waits for the latest workspace save. Interrupted requests restore
as idle tabs with an explanation; Blink never replays them automatically.
Transient confirmations and the cURL preview start closed.

**Saved data includes credentials, request bodies, and response content.
It is local plaintext, not encrypted.** Desktop saves use owner-only file
permissions on macOS and Linux. Protect your device and backups accordingly.
There is no cloud synchronization. Clear a request's history from its History
view.
Discarding a tab removes it from the next saved snapshot; this is not secure
erasure of filesystem blocks or backups.

On macOS, the snapshot is stored at
`~/Library/Application Support/com.kyle.blink.gpui/workspace-v1.json`.
The paths of files you picked for request bodies are stored beside it in
`file-grants.json`. The cookie jar is stored beside it in `cookies.json`,
including session cookies. The theme is stored in `theme.json` and the
window size and position in `window-state.json`.
Other desktop systems use the platform application data directory. Set
`BLINK_DATA_DIR` to use another directory. Saves write a temporary file,
sync it, then replace the snapshot atomically.

On first launch, when there is no snapshot yet, Blink copies
`workspace-v1.json`, `file-grants.json`, and `cookies.json` once from the
earlier Tauri app's `com.kyle.blink` directory. It never changes those files.

The versioned format supports up to 10,000 requests, 10,000 groups, and a 64 MiB total
snapshot. Existing version 1 snapshots restore into Ungrouped. Versions 1, 2,
3, and 4 remain readable; the next save writes version 4.

The footer reports saving and failure states. Failed saves preserve the
previous snapshot and expose Retry. A corrupt or unsupported snapshot is
never silently replaced: retry loading or confirm Start fresh. If quitting
cannot save, Blink stays open; Quit without saving explicitly discards
unsaved changes.

## Stack

- Rust
- GPUI and GPUI Kit (`gpui-kit`) for the interface
- reqwest, tokio, and tokio-tungstenite in the request engine
- Lucide icons

The workspace has two crates. `crates/blink-core` holds the request engine
and every piece of logic that does not draw: requests, tokens, groups,
environments, the snapshot format, import and code export, checks, and
themes. `crates/blink` is the GPUI app.

## Develop

```sh
cargo run -p blink
```

Set `BLINK_DATA_DIR` to run against a separate data directory:

```sh
BLINK_DATA_DIR=/tmp/blink-dev cargo run -p blink
```

Build the macOS app bundle at `target/release/Blink.app`:

```sh
script/bundle-macos
```

## Verify

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Engine tests use a local TCP server. Snapshot compatibility tests read
fixtures written by the earlier TypeScript encoder in
`crates/blink-core/tests/fixtures/workspace`. UI tests run headless and open
no window.
