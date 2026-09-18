# Blink

Blink is a fast, private REST API client for one-off requests. It runs as a
Tauri desktop app. It does not require an account, workspace, collection, or
cloud service.

## Included

- Open independent request tabs with the `+` button. Each tab retains its
  draft, response, editor selection, wrapping, and scroll position.
- Run requests in the background while working in another tab.
- Duplicate a complete draft without copying its response or sending it.
- See methods, endpoint paths, hosts, and request status in compact tabs.
- Send `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS` requests.
- Edit query parameters, headers, and JSON or text request bodies.
- Use Bearer tokens or Basic authentication. Credentials stay in memory.
- Export shell-quoted cURL commands. These include any entered credentials.
- Inspect response status, duration, headers, and body, including error responses.
- Switch between pretty JSON and raw text. Large JSON numbers stay exact.
- Copy response bodies and toggle line wrapping.
- Use the Tauri backend for requests without browser CORS limits.
- Keep request state in memory only. Nothing is sent until you press Send.
- Distinguish a previous response from an edited, unsent draft.

Requests have a 30-second timeout and a 4 MiB response limit. Redirects are
not followed, so the desktop app shows the original 3xx response. The
response inspector is a UTF-8 text viewer, not a binary file downloader.

## Request tabs

Tabs stay in memory only. Closing an edited tab asks for confirmation.
An in-flight tab cannot close until its request finishes or times out.
Closing the final tab opens a new blank tab. New tabs never send automatically.

| Action                   | Shortcut                      |
| ------------------------ | ----------------------------- |
| New request tab          | `Cmd/Ctrl+T`                  |
| Duplicate active request | `Cmd/Ctrl+Shift+D`            |
| Close active request     | `Cmd/Ctrl+W`                  |
| Next or previous request | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Send active request      | `Cmd/Ctrl+Enter`              |
| Focus URL                | `Cmd/Ctrl+L`                  |

In the request tab strip, use Left/Right, Home/End, and Delete to select or
close tabs. Escape dismisses the close confirmation or cURL preview. Browser
hosts can reserve shortcuts; the native Tauri app is the primary target.

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
components. Request state does not survive a full page or app restart.

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
