# Transport settings and response export

Date: 2026-09-28
Status: Approved design, awaiting spec review

## Goal

1. Make the request timeout, redirect policy, and response inspection limit
   application settings. Today they are hard-coded (30 s, no redirects, 4 MiB).
2. Let the user save any response body to a file, including binary bodies and
   bodies larger than the inspection limit.

## Decisions

| Topic | Decision |
|---|---|
| Export scope | Text and binary bodies, including bodies over the inspection limit. |
| Settings scope | Application-wide only, in `WorkspacePreferences`. No group or request overrides. |
| Raw byte storage | One temp file per response in the app cache directory. Not kept across restarts. |
| Body over inspection limit | Truncated read-only preview plus full body in the temp file. |
| Download cap | Fixed 1 GiB. Not a setting. |
| Save path source | Rust opens the native save dialog. The webview never supplies a file path. |
| Workspace format | No version change. New preference fields fill from defaults; new response fields are optional. |

## Settings

### Fields

Add to `WorkspacePreferences` in `src/lib/preferences.ts`:

| Field | Type | Default | Valid range |
|---|---|---|---|
| `timeoutSeconds` | integer | 30 | 1–600 |
| `connectTimeoutSeconds` | integer | 10 | 1–`timeoutSeconds` |
| `followRedirects` | boolean | `false` | — |
| `maxRedirects` | integer | 10 | 1–20 |
| `inspectionLimitMiB` | integer | 4 | 1–16 |

The 16 MiB upper bound exists because the workspace saves each tab's preview
body inside a 64 MiB file.

### Compatibility

- Add `normalizePreferences(value: unknown): WorkspacePreferences | null`.
  - It fills each missing new field with its default.
  - It returns `null` for a wrong type or an out-of-range value.
- `validPreferences` stays the strict check on a complete object.
- Workspace load (`src/lib/workspace.ts`) uses `normalizePreferences` in place
  of `validPreferences`. Existing workspaces load with default transport
  settings. The snapshot version does not change.

### UI

Add a **Requests** section to `ApplicationSettingsDialog.vue`, between
"New request defaults" and "Workspace". It uses the same label, input, and
`HelpTooltip` patterns as the existing sections.

- Timeout (s): number input.
- Connect timeout (s): number input.
- Follow redirects: checkbox.
- Max redirects: number input. Disabled when Follow redirects is off.
- Inspection limit (MiB): number input. Help text: "Larger bodies show a
  truncated preview. Save the response to get the full body."

On save, an invalid value shows an inline error under the field and blocks the
save, the same as the token definitions error.

## Transport

### Options

New module `src/lib/transport-options.ts` holds the type, the defaults, and the
range checks. `preferences.ts` and `transport.ts` both import it. It is a
separate module because seven test files mock `@/lib/transport` with only
`sendRequest`.

```ts
export type TransportOptions = {
  timeoutSeconds: number;
  connectTimeoutSeconds: number;
  followRedirects: boolean;
  maxRedirects: number;
  inspectionLimitMiB: number;
};
```

`sendRequest(request, options)` takes the options. Both callers pass the values
from the current preferences: `src/composables/useRequestRunner.ts` and
`fetchSchema` in `src/lib/graphql-schema.ts`. `fetchSchema` releases its
response after it parses the schema. If the response is truncated, it fails
with `"Schema response exceeds the inspection limit."`.

The desktop command becomes `send_request(request, options)`. Rust checks the
same ranges again and returns `"Invalid transport settings."` if a value is out
of range. The frontend is not the trust boundary for limits.

### Desktop client

`Client::builder()` in `src-tauri/src/lib.rs`:

- `.timeout(timeoutSeconds)` and `.connect_timeout(connectTimeoutSeconds)`.
- `followRedirects` false: `.redirect(Policy::none())`, as today.
- `followRedirects` true: `.redirect(Policy::custom(..))`. The closure:
  - stops with the error `"Stopped after N redirects."` when
    `attempt.previous().len() > maxRedirects`,
  - otherwise stores `attempt.previous().len()` in an `Arc<AtomicUsize>` and
    follows.

The client is built for each request, so the counter belongs to one request.
`redirectCount` is the counter value. `finalUrl` is `response.url()`, set only
when the count is above 0.

reqwest removes `Authorization`, `Cookie`, and `Proxy-Authorization` when a
redirect goes to a different host. Blink keeps that behavior.

### Browser preview

- `redirect: "follow"` when `followRedirects` is true, `"manual"` when false.
- `finalUrl` is `response.url` when `response.redirected` is true.
- The browser does not report the hop count. `redirectCount` is omitted, and
  the panel shows "redirected" without a number.
- The browser ignores `maxRedirects` and uses its own limit.
- The `AbortController` timeout uses `timeoutSeconds`. The browser has no
  separate connect timeout.

### Timeout messages

- Desktop: `"Request timed out ({connect} s connection / {total} s total limit)."`
- Browser: `"Request timed out after {total} seconds."`

### Hard-coded text to replace

- `src/App.vue:461` status bar: `"{total} s TIMEOUT · {limit} MiB LIMIT"`.
- `src/components/ResponsePanel.vue:556`: `"{elapsed} s elapsed · {total} s timeout"`.
- `README.md` request limits paragraph: describe the settings and the 1 GiB
  download cap.

## Response storage

### `src-tauri/src/response_store.rs`

A managed state that owns the temp files. It has one purpose: map a body id to
a file of raw bytes.

- Directory: `<app cache dir>/responses/`.
- `ResponseStore::new(dir)` deletes and recreates the directory at app start.
- `create() -> (id, File)`: a random id and a new file in the directory.
- `path(id) -> Option<PathBuf>`.
- `release(id)`: delete the file and remove the entry. An unknown id does
  nothing.
- `copy_body(id, dest)`: copy the file to `dest`. Tests use this function
  directly.

### Streaming in `send_request`

For every response, of any size:

1. Create a store file.
2. For each chunk:
   - If the total exceeds `DOWNLOAD_LIMIT` (1 GiB), stop, release the file, and
     return `"Response exceeds the 1 GiB download limit."`.
   - Write the chunk to the file. On a write error, release the file and
     return `"Cannot store the response. Check free disk space."`.
   - Keep bytes in the preview buffer until it holds
     `inspectionLimitMiB × 1 MiB` bytes.
3. Build the preview from the buffer (see Binary detection).

`DOWNLOAD_LIMIT` is a function parameter inside the streaming helper, so tests
can use a small value.

### Binary detection

A body is binary if the preview buffer:

- contains a NUL byte, or
- fails `std::str::from_utf8` with an error that has `error_len()` of
  `Some(_)`.

An error with `error_len()` of `None` is an incomplete character at the preview
cut. It does not make the body binary. The preview drops those trailing bytes.

For a binary body, `body` is `""`. Otherwise `body` is the decoded preview.

The browser preview uses the same rule with
`new TextDecoder("utf-8", { fatal: true })` on the preview bytes, after it
removes up to 3 trailing bytes of an incomplete character.

### Response shape

Additions to `ApiResponse` (`src/lib/request.ts`) and `ResponseOutput` (Rust):

| Field | Type | Meaning |
|---|---|---|
| `bodyId` | `string?` | Store handle. Not saved in the workspace. |
| `truncated` | `boolean` | `true` when the preview is shorter than `sizeBytes`. |
| `binary` | `boolean` | See Binary detection. |
| `finalUrl` | `string?` | Set only when a redirect happened. |
| `redirectCount` | `number?` | Desktop only. |

`sizeBytes` is the full body size, not the preview size.

### Commands

- `save_response(bodyId, suggestedName) -> bool`: Rust opens the native save
  dialog through `tauri-plugin-dialog`, then calls `copy_body`. It returns
  `false` if the user cancels.
- `save_response_text(text, suggestedName) -> bool`: the same dialog, but it
  writes `text`. Used when no temp file exists.
- `release_response(bodyId)`.

The JS capability does not get dialog permissions, because the dialog opens
from Rust.

### Suggested file name

In order:

1. The `filename` parameter of `Content-Disposition`.
2. The last non-empty path segment of `finalUrl` or the request URL.
3. `response` plus an extension from the content type: `.json`, `.xml`,
   `.html`, `.txt`, `.png`, `.jpg`, `.pdf`, `.zip`, else `.bin`.

Remove path separators and control characters from the name.

### Lifetime

- The frontend calls `release_response` when a session sends again, and when a
  request is deleted. Closing a tab does not release the body, because the
  request and its response stay in the browser tree and can open again.
- App start clears the directory, which removes files from a crash or a killed
  process.
- Workspace save removes `bodyId` from each response.
- Workspace load: `validateResponse` accepts the new optional fields and a
  missing `truncated` or `binary` is treated as `false`.

### Browser preview storage

- A module map in `src/lib/response-body.ts` holds a `Blob` for each `bodyId`.
- The browser path keeps all chunks up to the 1 GiB cap and builds the preview
  from the first `inspectionLimitMiB` MiB.
- Save creates an object URL, clicks an `<a download>`, and revokes the URL.
- Release deletes the map entry.

### Save API for components

New module `src/lib/response-body.ts` exports these. It is separate from
`transport.ts` for the same test-mock reason as `transport-options.ts`:

- `saveResponse(response: ApiResponse, requestUrl: string): Promise<boolean>`
  - With `bodyId`: `save_response`, or the Blob download in the browser.
  - Without `bodyId`, and not truncated or binary: `save_response_text`, or a
    text Blob download in the browser.
  - Otherwise it throws. The UI does not call it in this state.
- `releaseResponse(response: ApiResponse | null): void`.
- `canSaveResponse(response: ApiResponse): boolean`.
- `suggestedFileName(response: ApiResponse, requestUrl: string): string`.
- `storeBlob(blob: Blob): string`, used by the browser transport.

## Response panel UI

- **Save button:** an icon button next to Copy, tooltip "Save response body…".
  Shown for every response. When `canSaveResponse` is false it is disabled,
  with the tooltip "Body is no longer available. Send the request again."
- **Binary body:** the editor area shows a centered notice:
  "Binary response · {size} · {content type}" and a **Save…** button. Pretty,
  Wrap, JSON tree, and jq controls are hidden.
- **Truncated text:** a one-line bar above the editor:
  "Preview shows the first {limit} of {size}. Save… to get the full body."
  The editor shows plain preview text. Pretty, JSON tree, and jq are disabled,
  with the tooltip "Unavailable for truncated responses".
- **Redirect line:** under the status line, "→ {finalUrl} · {n} redirects", or
  "→ {finalUrl} · redirected" when `redirectCount` is missing. Shown only when
  `finalUrl` is set.
- Copy still copies the preview text.
- Save errors show inline in the panel: "Cannot save the file: {reason}."
  A cancelled dialog shows nothing.

Colors come from the existing theme tokens. The truncated bar uses the muted
foreground and border tokens, not the accent.

## Errors

| Case | Message |
|---|---|
| Body over 1 GiB | "Response exceeds the 1 GiB download limit." |
| Temp file write fails | "Cannot store the response. Check free disk space." |
| Redirect limit reached | "Stopped after N redirects." |
| Transport settings out of range | "Invalid transport settings." |
| Save copy fails | "Cannot save the file: {reason}." |
| Save dialog cancelled | No message |

## Dependencies

- `tauri-plugin-dialog` (Rust crate only, registered in `run()`).
- `tempfile` is already a dependency. The store creates each file with
  `tempfile::Builder::new().tempfile_in(dir)` and keeps the `NamedTempFile` in
  its map. The file name is the id. Removing the map entry drops the
  `NamedTempFile`, which deletes the file.

## Testing

### Rust (`src-tauri/src/request_tests.rs`, local test server)

- Redirects not followed when off: returns the 3xx.
- Redirects followed when on: returns the final response, `finalUrl`, and
  `redirectCount`.
- Hop limit reached: "Stopped after N redirects."
- Truncated preview: `truncated`, full `sizeBytes`, and the temp file holds all
  bytes.
- Binary detection: NUL byte, invalid UTF-8, and a multi-byte character cut at
  the preview boundary (not binary).
- Download cap with a small test value: error, and the file is deleted.
- `release` deletes the file. `ResponseStore::new` clears old files.
- `copy_body` writes identical bytes.
- Out-of-range options are rejected.

### Vitest

- `normalizePreferences`: missing fields, wrong types, out-of-range values,
  `connectTimeoutSeconds` greater than `timeoutSeconds`.
- Workspace load of a v4 snapshot without the new preference fields.
- `validateResponse` with and without the new response fields. Save removes
  `bodyId`.
- Browser transport: follow on and off, timeout from options, truncation,
  binary detection, Blob store, and release.
- `canSaveResponse` for each state.
- ResponsePanel: binary notice, truncated bar, disabled controls, disabled Save
  after restart, redirect line.
- ApplicationSettingsDialog: new fields, validation errors, disabled Max
  redirects.

### E2E (Playwright, browser preview)

- A response larger than the inspection limit shows the truncated bar, and
  Save starts a download with the full body.

## Out of scope

- Per-group or per-request transport overrides.
- Cancelling a request in progress.
- A configurable download cap.
- Rendering images or PDFs in the response panel.
- Charset decoding other than UTF-8.
