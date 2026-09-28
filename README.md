# Blink

Blink is a fast, private REST API client for one-off requests. It runs as a
Tauri desktop app. It does not require an account or cloud service. The local
Browser sidebar organizes the request tabs in the current session.

## Included

- Open independent request tabs with the `+` button. Each tab retains its
  draft, response, editor selection, wrapping, scroll position, and Browser
  group.
- Use the Browser sidebar to create, nest, collapse, rename, and delete groups.
  Move the active request to any group or back to Ungrouped.
- Run requests in the background while working in another tab.
- Duplicate a complete draft without copying its response or sending it.
- See methods, endpoint paths, hosts, and request status in compact tabs.
- Send `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS` requests.
- Edit query parameters, headers, and JSON, text, or GraphQL request bodies.
  GraphQL bodies take a query and optional JSON variables. Format tidies JSON
  bodies, GraphQL queries, and GraphQL variables.
- Use Bearer tokens or Basic authentication.
- Export shell-quoted cURL commands. These include any entered credentials.
- Inspect response status, duration, headers, and body, including error responses.
- Switch between pretty JSON and raw text. Large JSON numbers stay exact.
- Copy response bodies and toggle line wrapping.
- Use the Tauri backend for requests without browser CORS limits.
- Restore open tabs and application state after restart. Nothing is sent until
  you press Send.
- Distinguish a previous response from an edited, unsent draft.

Requests have a 30-second timeout and a 4 MiB response limit. Redirects are
not followed, so the desktop app shows the original 3xx response. The
response inspector is a UTF-8 text viewer, not a binary file downloader.

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
| Focus URL                | `Cmd/Ctrl+L`                  |
| Application settings     | `Cmd/Ctrl+,`                  |

In the request tab strip, use Left/Right, Home/End, and Delete to select or
close tabs. Escape dismisses the cURL preview. Browser
hosts can reserve shortcuts; the native Tauri app is the primary target.

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

Secondary help appears in tooltips. Errors, effective authorization, storage
failures, and destructive confirmations remain visible.
Settings dialogs keep Save and Cancel visible while their contents scroll.
Escape closes a tooltip first, then the dialog. Cancel discards unsaved edits.

## Local state and privacy

Blink saves tab order, groups, group hierarchy, request membership, the active
tab, complete request drafts, responses, errors, editor tabs, Pretty/Raw mode,
wrapping, and response scroll position.
The desktop app also restores window size and position. Closing the window
or quitting waits for the latest workspace save. Interrupted requests restore
as idle tabs with an explanation; Blink never replays them automatically.
Transient confirmations and the cURL preview start closed.

**Saved data includes credentials, request bodies, and response content.
It is local plaintext, not encrypted.** Desktop saves use owner-only file
permissions on macOS and Linux. Protect your device and backups accordingly.
There is no cloud synchronization or request history beyond the open tabs.
Discarding a tab removes it from the next saved snapshot; this is not secure
erasure of filesystem blocks or backups.

On macOS, the snapshot is stored at
`~/Library/Application Support/com.kyle.blink/workspace-v1.json`.
Other desktop systems use Tauri's application data directory. Native saves
write a temporary file, sync it, then replace the snapshot atomically.
The versioned format supports up to 128 tabs, 128 groups, and a 64 MiB total
snapshot. Existing version 1 snapshots restore into Ungrouped. Versions 1, 2,
and 3 remain readable; the next save writes version 3 with preferences.

The footer reports saving and failure states. Failed saves preserve the
previous snapshot and expose Retry. A corrupt or unsupported snapshot is
never silently replaced: retry loading or confirm Start fresh. If quitting
cannot save, Blink stays open; Quit without saving explicitly discards
unsaved changes.

Browser preview stores a separate snapshot in this origin's local storage.
Its smaller browser-defined quota can reject large responses; this is shown
as a save error. Use the desktop app for large sessions.

## Stack

- Tauri 2 and Rust
- Vue 3 and TypeScript
- Tailwind CSS 4
- shadcn-vue source configuration with Reka UI primitives
- Lucide icons

## Develop

```sh
bun install
bun run desktop
```

This starts Vite and the native Tauri window. Vue, TypeScript, and CSS edits
hot-reload through Vite. Rust edits rebuild and restart the native app.
Frontend state can survive template and style edits; script edits can remount
components. Saved request state restores after full page and app restarts.

Vite uses port `1420` for the app and `1421` for its HMR WebSocket. Keep both
ports available. Do not run a separate Vite server before `bun run desktop`.
The dev server binds to localhost unless Tauri provides `TAURI_DEV_HOST`.

For web-only UI work, run:

```sh
bun run dev
```

Browser preview uses `fetch`, sends no ambient cookies, and remains subject
to CORS. Browser response headers can be restricted by CORS, and browsers
cannot expose manual redirect responses. Use the desktop app for full HTTP
inspection. Requests never use a relay or cloud proxy.

## Verify

```sh
bun run lint
bun run test
bunx playwright install chromium
bun run test:e2e
bun run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Browser tests use intercepted fixture responses, not a public API. Rust
tests use a local TCP server. Screenshots are written to `artifacts/`.
