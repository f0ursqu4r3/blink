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
- Add a response token in group or application settings. It reads a value from
  another request's response, and Blink sends that request first when the
  value is missing or older than the max age.
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

Application Settings → Network controls the timeout (default 30 s total,
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
Application Settings opens in the center of the display under the pointer on
macOS. General, Appearance, Network, and Tokens organize the controls. Valid edits apply
and save immediately. Invalid fields keep their last valid values and show an
error. The theme menu groups themes into Dark and Light. Hover over a theme
to preview it; click to select it. Leaving the theme restores the selected
theme. Closing settings keeps applied changes.

Group settings keep Save and Cancel visible while their contents scroll.
Escape closes a tooltip first, then the dialog. Cancel discards unsaved group edits.

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
3, 4, and 5 remain readable. Local-only snapshots use version 4; snapshots
with projects or closed-project archives use version 5.

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

Build a signed and notarized macOS release:

```sh
script/bundle-macos
```

The script selects your Developer ID Application certificate when exactly
one is available. It enables the hardened runtime, requests a secure
timestamp, submits the app to Apple, and waits for acceptance. It then
staples the notarization ticket, verifies the signature and ticket, and
runs the Gatekeeper assessment. Only then does it replace the previous app
and create `target/release/Blink-VERSION-macos-ARCH.zip` for distribution.
`ARCH` is `arm64`, `x86_64`, or `universal`.

The certificate and its private key must be available in your unlocked
keychain. Signing and notarization require internet access. Store your
notarization credentials once with this interactive command:

```sh
xcrun notarytool store-credentials blink-notary
```

Enter your Apple Developer account email, Team ID, and an app-specific
password when prompted. The command validates and stores them in Keychain.
Do not put passwords in source files or shell command arguments. For an
existing profile, use `--notary-profile NAME` or set `NOTARY_PROFILE`.

To select a certificate, use its exact name or SHA-1 fingerprint:

```sh
security find-identity -v -p codesigning
script/bundle-macos --identity 'Developer ID Application: Your Name (TEAMID)'
```

`MACOS_SIGN_IDENTITY` also selects the certificate. If none is available,
the script stops. Use `--sign-only` to skip notarization for a development
build. For a local build without a certificate, use
`script/bundle-macos --ad-hoc`. An ad-hoc signature does not establish
developer identity or satisfy Gatekeeper for downloaded apps.

To build for both Apple silicon and Intel:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
script/bundle-macos --universal
```

`BLINK_UNIVERSAL=1` also enables this mode. The script always builds into
the repository's `target` directory. Run `open target/release/Blink.app`
to launch the result.

Local signing modes produce only the app, not a new release ZIP. The
release ZIP contains the app with its attached ticket. Notarization logs
remain in `target/release/notarization.*` for diagnosis. A submission waits
up to 30 minutes; Apple can continue processing after that timeout. Use
the submission ID in `submission.plist` with `xcrun notarytool info` or
`xcrun notarytool log` to check a delayed or rejected submission.

The tagged release workflow still uses Velopack for update packages. This
standalone ZIP does not include automatic updates. See
[Apple's notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

## Verify

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 script/test_bundle_macos.py # macOS bundle and signing checks
```

Engine tests use local TCP and HTTPS servers. Authentication integration
tests cover PKCE exchange, refresh rotation, concurrent refresh, cancellation,
credential removal, token endpoint errors, and client-certificate origin
restrictions. These tests use a private test CA and an in-memory credential
store. They do not change Keychain or OS trust settings. Real provider login
and OS credential authorization still need manual checks.

Snapshot compatibility tests read
fixtures written by the earlier TypeScript encoder in
`crates/blink-core/tests/fixtures/workspace`. UI tests run headless and open
no window.

## Projects

A project is a Browser root group backed by a folder on disk. The workspace
is the full Blink session. One workspace can contain several projects beside
local groups. Child groups use their root's storage location. Each project
keeps its own request IDs, so separate copies can be open at the same time.

Use **Open Project Folder…** in the Browser header, the File menu, or the
command center to attach an existing project. To convert a local root, use
**Save Group to Folder…** in its group menu. Blink explains which data will be
shared before you select an empty folder.

Use **Close Project** to remove a project from the Browser. Its files stay on
disk. Blink keeps a private local archive of its session data for reopening.
Closing a project does not erase its credentials or history. Project roots
cannot be nested or removed with the ordinary Delete Group action.
Resolve project file or save errors before closing. Blink blocks Close Project
while those errors could leave edits unsaved. Restore an unavailable folder, or
use Reload Project to discard conflicting definition edits explicitly.

Ordinary moves cannot cross storage locations. Use **Move to Another
Project…** in the request menu for an explicit transfer. Blink checks token
dependencies and inherited request settings before asking you to confirm.
A transfer that would break those dependencies or change that context is
rejected. Each project has a separate cookie jar. Project requests do not
inherit workspace-global tokens or workspace-global response tokens;
response-token references cannot cross projects.

### Project files

A project contains readable JSON files:

```text
blink.json
nested-group-2/
  group.json
  get-request-3.json
```

`blink.json` is the versioned manifest. It stores the root group and lists
managed group and request files. Nested group folders contain `group.json`;
each request has its own JSON file. Numeric IDs identify definitions across
renames. Names in paths are sanitized for the filesystem.

You can edit these definitions outside Blink. For manual file additions or
moves, update the manifest's `groups` and `requests` path lists. Update
`parentId` or `groupId` when membership changes. Unlisted files are unrelated
files; Blink does not load or remove them. Project loading does not send
requests. Project-relative upload paths still require a file-picker grant
before Blink can read the selected file.

Blink checks attached projects for external changes every three seconds.
It reloads changed definitions automatically when there are no local definition
edits. It refuses conflicting saves. **Reload Project…** asks you to confirm
before replacing definitions and discarding local definition edits. Local
credentials and history stay in Blink. Missing or invalid project folders
remain visible with an error; autosave does not recreate them.

### Shared and private data

Project files contain request definitions, group settings, and ordinary token
and environment values. Blink replaces authorization credentials with local
token references. Credential values, captured response values, responses,
history, cookies, selected environments, and tab state stay in application
storage. Capture definitions remain part of the shared request definition.
Known credential values remain private after their authorization references
are removed. Project GraphQL schema requests also use the project's cookie jar
and a separate schema cache.

**Request bodies, custom headers, and ordinary token values are shared.**
Blink does not scan arbitrary content for secrets. Use token references for
secrets and review project files before sharing or committing them to Git.
A reference alone does not make an ordinary shared token value private.

Named secrets, OAuth credentials, and client identities use macOS Keychain.
Existing literal credentials and other private application data remain
plaintext; they are not migrated automatically. Closed-project archives also
contain private data. Protect the application data directory
and its backups.

Local snapshots with project attachments or closed-project archives use
version 5. Blink reads workspace versions 1 through 5. Local-only snapshots
without attachments or archives continue to use version 4.

### Interrupted saves

Each file replacement is atomic, but a whole project save spans several files.
Before changing managed files, Blink writes synced backups and a recovery
journal at `.blink-save-*/transaction.json`. The journal records file paths,
backup names, and hashes of the old and new contents.

After an interrupted save, Blink refuses to open or save the project and
shows the journal path. Recovery is manual. Preserve the journal directory
and its backups until recovery is complete. Compare current files with the
recorded hashes before restoring backups so that later external edits are
preserved. Blink does not perform automatic recovery or team synchronization.


## Professional workflows

- **Request inspection:** Send a request, then select **Request** in the response panel. Inspect the initial prepared request, token/auth sources, cookies, redirect destinations, HTTP version, remote address and peer-certificate SHA-256 when available. Values are hidden by default; **Reveal locally** shows sensitive values in the app. **Copy redacted** masks credential headers, query values, known named/process secrets and the body. Request-body previews stop at 64 KiB. TLS version/cipher, protocol framing and per-hop request headers are not exposed by the transport.
- **Collection runs:** Choose **Run collection…** from a group menu. Set request order, load CSV or a JSON array of token objects, choose stop-on-failure and run. Assertions, captures and attached contracts determine pass/fail. Each dataset row has fresh captures and cookies; later requests in that row can use earlier captures. Runs use a snapshot and do not edit saved drafts. Failed-case retries keep prior setup captures in memory. Export a JSON report for CI. Protected environments require an explicit run confirmation.
- **Credentials:** In a request's **Auth** tab, open **Secrets, OAuth and client certificates…**. Store a named secret and use `{{@name}}` in HTTP request fields. Values resolve only when sending. Each project has its own Keychain scope; local groups share a scope. Basic and Bearer auth, headers, query rows, form fields and text bodies can use references. Code exports redact literal credential headers by default. Arbitrary literal secrets in bodies/custom fields still require review.
- **OAuth 2:** The credentials window supports native public-client authorization-code login with S256 PKCE, a browser and a loopback callback. Register `http://127.0.0.1` with a dynamic port and `/oauth/callback` with the provider. Use the chosen secret name as a Bearer token. Expiring tokens refresh on send when the provider returns a refresh token. Endpoints require HTTPS. Provider-specific client-secret flows are not supported. The flow follows [RFC 7636](https://www.rfc-editor.org/rfc/rfc7636) and [RFC 8252](https://www.rfc-editor.org/rfc/rfc8252).
- **Client certificates:** Choose a PEM certificate chain and private key for an exact HTTPS origin in the credentials window. Blink stores the identity in Keychain. A client-certificate request cannot redirect to another origin. Credential storage requires macOS; live provider login and OS authorization prompts need manual validation.
- **Response comparisons:** Pin a history response, then compare another history response from any request or environment. JSON comparisons show field changes and ignore object key order. Add JSON pointer paths such as `/updatedAt` to ignore fields or subtrees. Other responses retain text diffs. The pinned snapshot and ignore rules stay in local application storage and survive history pruning and project close/reopen.
- **OpenAPI contracts:** Imported OpenAPI 3.x requests retain their operation, referenced schemas and source file path. Source reload updates only the selected request. Use the **Contract** tab for parameter suggestions, examples, request checks, response checks and source reload. Local references are supported. Unsupported schema rules, remote references and incomplete response bodies produce an incomplete result; collection runs do not count them as passes. Source snapshots may contain examples and defaults, so review imported documents before sharing a project.

The headless runner uses project files and reports failures through its exit code:

```sh
cargo run -p blink-core --bin blink-run -- /path/to/project \
  --dataset cases.csv --stop-on-failure --report results.json
```

Use `--group NAME`, `--environment NAME`, and `--allow-protected` when needed.
Exit codes are `0` for all passed, `1` for failed/canceled/skipped cases, and `2`
for input or file errors. Reports omit response bodies and captured values.
Use process-environment references such as `{{!API_TOKEN}}` for CI secrets.
