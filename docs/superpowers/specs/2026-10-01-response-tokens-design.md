# Response tokens design

## Goal

A token can take its value from the response of another request. A request
that uses the token sends the source request first when the token has no
usable value. Example: `{{access_token}}` reads `$.access_token` from the
response of "Login", and the bearer token field of every request in the
group is `{{access_token}}`.

The bearer and basic-auth fields also become token fields, so a typo such as
`{{acess_token}}` shows in the field instead of hiding behind a mask.

## Decisions

- Response tokens are token definitions (group or global scope), not an
  auth mode and not a capture option. Captures stay unchanged.
- Refresh: Blink sends the source request when no usable value exists or
  the value is older than the token's max age.
- A stored value is usable only when its response was 2xx and the source
  request still resolves to the same fingerprint. The cache stores only 2xx
  results. Each fingerprint keeps its
  own value, so DEV and PROD each keep a token.
- Auth fields mask literal text and show references. Response token values
  never show in any field. Text tokens that reference a response token
  show the reference, never the value.
- Response tokens are a separate list next to the text tokens.
  `Definitions` (`IndexMap<String, String>`) does not change.

## Model (blink-core)

New type in `model.rs`:

```rust
pub struct ResponseToken {
    pub id: u64,
    /// Token name, used as {{name}}.
    pub name: String,
    /// The source request (session id).
    pub request_id: u64,
    pub source: CheckSource,
    /// JSON path or header name, as in captures.
    pub path: String,
    /// None: send again only when no usable value exists.
    pub max_age_secs: Option<u64>,
}
```

New fields:

- `RequestGroup.response_tokens: Option<Vec<ResponseToken>>`
- `global_response_tokens: Vec<ResponseToken>` in the workspace snapshot

Both are optional in the saved data and skipped when empty. The workspace
version stays 4; the v1–v4 fixtures load and round-trip unchanged.

Environments do not get response tokens. The cache keeps a value per
fingerprint, and the fingerprint includes the environment values, so a
group response token already gives each environment its own value.

Name rules: the nearest scope wins, as for text tokens. Within one scope,
a response token cannot use the name of a text token or of another
response token; the editor rejects it.

A source request's fingerprint includes the values of the response tokens
it reads. Blink solves the fingerprints of all sources together for each
dependency cycle. A source request that really uses its own token in a
loop gets no value.

Values are read with `checks::read_source`, so paths work as in captures
and assertions. Max age is typed as `30s`, `15m`, `1h`, or empty.

## Cache

New module `response_token_cache.rs`.

- Key: `(request_id, fingerprint)`. The fingerprint is `Prepared::fingerprint`
  of the resolved source request.
- Entry: `fetched_at` and the extracted values keyed by `(source, path)`.
  The cache stores only 2xx results, so an entry has no status field.
- A value is usable when the entry for the source request's current
  fingerprint exists, the `(source, path)` value is in
  the entry, and `now - fetched_at` is within the max age.
- Every 2xx send of a request that a response token reads updates its
  entry, both manual sends and automatic sends. Blink extracts the values
  of all response tokens that read that request.
- A 2xx response without the path of a token records an entry with no
  value for that token. This entry replaces the previous value for that
  fingerprint.
- The cache saves to `response_tokens.json` in the app data directory,
  next to the workspace, in plaintext (the same as saved credentials). It
  keeps the newest 8 entries per request. The file saves off the UI thread,
  in order.
- Entries for deleted requests are removed when the cache loads, not when
  the request is deleted. Undo Delete therefore keeps the tokens.

## Resolution

`prepare` stays synchronous. `resolve_token_definitions` takes the cache
and the current time. For each scope it adds the usable values of that
scope's response tokens next to the text tokens. Interpolation, field
hints, Copy cURL, code export, and the stale banner all read these values
without network access.

When a response token has no usable value, interpolation fails with:
`"access_token" has no current value. Send "Login" or the request that
uses it.`

History rows and the response panel's final URL show what was sent. This
includes a response token value used in the URL, the same as for text
tokens.

Staleness ignores max age. An expired value does not mark a request
EDITED. The next send still sends the source request again.

## Send flow

Before an HTTP send or a WebSocket connect, the store runs a dependency
step.

1. `response_token_plan(session, workspace, cache, now)` (core, pure)
   collects the token names the request actually sends, following references through
   text token values. For each name that resolves to a response token
   without a usable value, the source request becomes a dependency. The
   source request's own dependencies come first. Disabled rows, an unsent
   body, and overridden auth do not count. A token used as the whole URL
   host is not sent automatically; the send shows "no current value".
2. A cycle fails before any send. The message starts at the repeated
   request: `Response token cycle: /refresh → /login → /refresh`.
   A request that sends a token it supplies itself fails with a message
   that names the fix. When the reference comes from the request's auth
   (its own or inherited), the message is
   `"/login" uses "{{access_token}}", which it supplies. Set its auth to No auth.`
   Otherwise it is
   `"/login" uses "{{access_token}}", which it supplies. Remove that reference from "/login".`
   Blink decides by building the request again with No auth. A manual send
   of the source request shows the same message.
3. The store sends each dependency in order through the normal send path.
   The source request's tab updates its response and history as for a
   manual send, and the cache updates.
4. The store sends the original request.

Rules:

- Single flight: when a source request is already sending (manually or for
  another request), the step waits for that send instead of starting one.
- While waiting, the dependent's response panel shows `Sending "Login"…`.
  A waiting request counts as running: it cannot be deleted, and Cancel
  ends the wait.
- If a chain needs a source that it already sent in that chain, the
  dependent fails with the "has no value" message. Blink does not send
  the source again.
- Cancel on the dependent cancels a dependency send it started. It does not
  cancel a send the user started.
- Protected environments: the confirmation of the dependent covers
  dependencies in the same environment. When a dependency is in a different
  protected environment that is not confirmed, the step stops with
  `Send "Login" once to confirm PROD.`
- When a dependency fails to start, the dependent fails at once. When a
  dependency was deleted, the error is `"access_token" reads a deleted
  request.`
- When a dependency fails (network error, non-2xx status, missing path),
  the dependent is not sent. The error names the token and the request:
  `Could not get "access_token": "Login" returned 401.`
- Errors never include token values.

## UI

### Response tokens editor

Group settings and the global token settings get a "Response tokens" list
below the existing token table. Each row: name, request picker (searchable,
shows the group path), source (Body / Header / Status), path, max age, and
a delete button. The list shows inline errors for a duplicate name, a
missing request, and an invalid max age.

When a row's source request sends that same token, the row shows the
self-supplying source message from the send flow as a warning when the
editor opens. The warning does not block the save. A row error replaces
the warning.

### Auth fields

The bearer token, basic-auth username, and basic-auth password fields
become `TokenInput`. `TokenInput` gets a `secret` mode, used for the bearer
token and the password:

- While unfocused, the field shows a masked row: literal text as `•`
  characters, references as their raw `{{name}}` text, never as values.
  While focused, the field is the normal token field.
- Undefined references show in the warning color.
- `{{` opens token suggestions.
- In secret fields, hints and suggestions show no text token values. The
  accessibility value is hidden. This uses a vendored gpui-component patch
  (`Input::hide_accessibility_value`, in `BLINK-PATCH.md`).

### Response tokens in other fields

In every token field, a response token reference shows as its raw
`{{name}}` text in a distinct color, never as its value. Suggestions list
response tokens as `from "Login"` with no value.

Hint for a response token:

- Usable: `From "Login" · body $.access_token · 3 min ago · DEV`
- Not usable: `No current value · sends "Login" first`

Hover hints refresh when the app state changes. When a max age passes
while the app is idle, the hint can show the old age until the next
change. The next send still refreshes the value.

### Source request

The tab of a request that response tokens read shows a small badge with
the number of those tokens. Editing a source request refreshes the stale flags
of every request.

## Testing

blink-core unit tests:

- Cache: usable, too old, other fingerprint, non-2xx, missing path; DEV and
  PROD entries kept apart; 8-entry limit; entries removed with the request.
- Resolution: nearest scope wins; usable values enter the context; an
  unusable value gives the "no current value" error.
- Plan: references through text tokens; chain order; cycle error; no
  dependency when the value is usable.
- Workspace: v1–v4 fixtures round-trip unchanged; a new fixture with
  response tokens round-trips.
- No error message contains a token value.

Store tests (fake engine, as in the existing send tests):

- Dependency sent before the dependent; dependent not sent after a failed
  dependency.
- Single flight with two dependents.
- Cancel scope.
- Protected environment rules.
- A manual 2xx send updates the cache.

Headless UI tests (`ui_tests.rs`):

- Secret mode: literal text masked, references shown, undefined references
  in the warning color.
- Response tokens editor validation.
- Hint text.

Manual check on the dev build: a real login API, an environment switch,
and an expired max age.
