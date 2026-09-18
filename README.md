# Blink

Blink is a fast, private REST API client for one-off requests. It runs as a
Tauri desktop app. It does not require an account, workspace, collection, or
cloud service.

## Included

- Send `GET`, `POST`, `PUT`, `PATCH`, and `DELETE` requests.
- Edit request URL, JSON body, and headers.
- Inspect response status, duration, headers, and body.
- Use the Tauri backend for requests without browser CORS limits.
- Keep request state in memory for the current session only.

## Stack

- Tauri 2 and Rust
- Vue 3 and TypeScript
- Tailwind CSS 4
- shadcn-vue source configuration with Reka UI primitives
- Lucide icons

## Develop

```sh
bun install
bun run tauri dev
```

For web-only UI work, run:

```sh
bun run dev
```

## Verify

```sh
bun run build
cargo check --manifest-path src-tauri/Cargo.toml
```
