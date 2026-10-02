# Response Tokens Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A token can take its value from another request's response; a request that uses it sends the source request first when no usable value exists, and auth fields become token fields that show references.

**Architecture:** Response tokens are a separate list on groups and on the workspace. A cache keyed by `(request_id, fingerprint)` holds extracted values. `TokenSources` bundles groups, globals, response tokens, sessions, cache, and time; it builds an `InterpolationContext` that carries usable values plus per-name response token info, so `interpolate` stays synchronous. A pure `response_token_plan` lists the source requests to send; the app store runs that plan before an HTTP send or a WebSocket connect.

**Tech Stack:** Rust 2024, GPUI via `gpui-kit` 0.7 (patched `vendor/gpui-component`), serde, indexmap, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-10-01-response-tokens-design.md`

## Global Constraints

- Saved workspace stays version 4. New fields are `#[serde(default, skip_serializing_if = ...)]`. The v1–v4 fixtures in `crates/blink-core/tests/fixtures/workspace/` must round-trip byte-for-byte unchanged.
- Errors and hints never contain token values.
- A response token value never shows in any field, hint, suggestion, label, or cURL preview label. (Copy cURL and code export DO include the value, as they do for text tokens and credentials today.)
- UI copy uses the exact strings in this plan.
- Colors come from `crate::theme::colors(cx)` only (see `DESIGN.md`).
- Commits: Conventional Commits, no Claude attribution lines of any kind.
- Work on branch `feat/response-tokens` in the main checkout.
- Run `cargo test -p blink-core` and `cargo test -p blink` before each commit; `cargo clippy --workspace --all-targets` and `cargo fmt --all` must be clean.

## Spec deltas (forced by the code; recorded in the spec in Task 13)

1. Cache entries store only 2xx results, so the entry has no `status` field. A non-2xx send leaves the previous entry in place, but the dependent still fails (the plan sees the failed send; see Task 9).
2. Entries for deleted requests are pruned when the cache loads (startup), not at delete time, so Undo Delete keeps the token.
3. gpui-kit refuses inline tokens in a masked input (`check_token_mode` → `UnsupportedMode`). Secret mode therefore shows a masked display row while the field is not focused (literal text as `•`, references raw), and the normal token field while focused (references raw, literal text visible).

## Review Focus

1. **A source request deleted while tokens still read it** — the dependent must fail with `"access_token" reads a deleted request.` and the editor must flag the row; never panic. Test in Task 5 and Task 11.
2. **A source request that uses its own response token** (Login with `Authorization: Bearer {{access_token}}` reading Login) — must not recurse forever; Login builds with the token unresolved. Test in Task 5.
3. **A dependency whose send succeeds but the path is absent** (`$.access_token` missing in 200 body) — dependent fails with `Could not get "access_token": "Login" has no value at $.access_token.` and does not loop. Test in Task 9.
4. **Two dependents sent at once** — exactly one Login request reaches the server. Test in Task 9.
5. **Clock moving past max age while the app is idle** — the hint turns to `No current value · sends "Login" first` on next render and the next send refreshes. Test in Task 5 (pure, with injected `now_ms`).

---

## File map

Create:
- `crates/blink-core/src/response_tokens.rs` — max age parse/format, row validation, `TokenSources`, context building, `response_token_plan`, messages.
- `crates/blink-core/src/response_token_cache.rs` — `ResponseTokenCache` (pure, serde).
- `crates/blink-core/src/engine/response_token_state.rs` — load/save `response-tokens.json`.
- `crates/blink/src/ui/response_tokens_editor.rs` — the editor list used by group settings and Application Settings.
- `crates/blink-core/tests/fixtures/workspace/v4-response-tokens.json` and `.roundtrip.json`.

Modify:
- `crates/blink-core/src/model.rs` — `ResponseToken`, `RequestGroup.response_tokens`, `RequestSession.waiting_on`.
- `crates/blink-core/src/ids.rs` — `RESPONSE_TOKENS` sequence.
- `crates/blink-core/src/workspace.rs` — encode/decode `globalResponseTokens`, reserve ids.
- `crates/blink-core/src/workspace_state.rs` — fields, setters, `token_sources`, `refresh_stale`.
- `crates/blink-core/src/interpolation.rs` — `ResponseTokenInfo`, response-aware lookup and error.
- `crates/blink-core/src/token_hints.rs` — `TokenState::Response`, hint text, options.
- `crates/blink-core/src/token_display.rs` — response refs never become units.
- `crates/blink-core/src/runner.rs`, `session_curl.rs`, `session.rs` — take `TokenSources`.
- `crates/blink-core/src/engine/{mod.rs,paths.rs}` — cache file.
- `crates/blink-core/src/lib.rs` — new modules.
- `crates/blink/src/store.rs`, `crates/blink/src/runner.rs` — cache load/save, record, dependency step.
- `crates/blink/src/ui/token_input.rs` — `Response` state render, secret mode.
- `crates/blink/src/ui/request_pane/editor.rs`, `crates/blink/src/ui/group_settings.rs` — auth fields.
- `crates/blink/src/ui/group_settings.rs`, `crates/blink/src/ui/settings_window.rs` — editor list.
- `crates/blink/src/ui/request_pane/common.rs`, `crates/blink/src/ui/browser/menus.rs` — call sites.
- `crates/blink/src/ui/response_panel.rs` — `Sending "Login"…`.
- `crates/blink/src/ui/tabs.rs` — badge.

---

### Task 1: Model, ids, and saved data

**Files:**
- Modify: `crates/blink-core/src/model.rs`, `crates/blink-core/src/ids.rs`, `crates/blink-core/src/workspace.rs`, `crates/blink-core/src/workspace_state.rs`, `crates/blink-core/src/session.rs` (session literal), `crates/blink-core/src/workspace.rs:592` (session literal)
- Create: `crates/blink-core/tests/fixtures/workspace/v4-response-tokens.json`, `v4-response-tokens.roundtrip.json`
- Test: `crates/blink-core/src/workspace.rs` (tests module)

**Interfaces:**
- Produces:
  - `model::ResponseToken { id: u64, name: String, request_id: u64, source: CheckSource, path: String, max_age_secs: Option<u64> }` (serde camelCase; `max_age_secs` skipped when None)
  - `RequestGroup.response_tokens: Option<Vec<ResponseToken>>`
  - `RequestSession.waiting_on: Option<String>` (not saved; the source request label while a dependency sends)
  - `Workspace.global_response_tokens: Vec<ResponseToken>`
  - `Workspace::set_global_response_tokens(&mut self, tokens: Vec<ResponseToken>)`
  - `GroupSettingsChanges.response_tokens: Option<Option<Vec<ResponseToken>>>`
  - `ids::RESPONSE_TOKENS: Sequence`
  - `DecodedWorkspace.global_response_tokens: Vec<ResponseToken>`
  - `encode_workspace(..., global_response_tokens: &[ResponseToken])` (new last parameter)

- [ ] **Step 1: Write the failing round-trip test**

Create `v4-response-tokens.json` by copying `v4-no-tabs.json` and adding to its first group (create one group if it has none — read the file first) `"responseTokens":[{"id":7,"name":"access_token","requestId":1,"source":"json","path":".access_token","maxAgeSecs":900}]` and at the top level `"globalResponseTokens":[{"id":8,"name":"csrf","requestId":1,"source":"header","path":"x-csrf"}]`. Produce `.roundtrip.json` by running the existing round-trip test pattern once and saving the output only after checking by eye that the two token lists are present and unchanged.

Add to the tests in `workspace.rs`, next to the existing fixture tests (match their helper names — read them first):

```rust
#[test]
fn round_trips_v4_with_response_tokens() {
    let input = include_str!("../tests/fixtures/workspace/v4-response-tokens.json");
    let expected = include_str!("../tests/fixtures/workspace/v4-response-tokens.roundtrip.json");
    let decoded = decode_workspace(input).unwrap();
    assert_eq!(decoded.global_response_tokens[0].name, "csrf");
    assert_eq!(
        decoded.groups[0].response_tokens.as_ref().unwrap()[0].max_age_secs,
        Some(900)
    );
    let encoded = encode_workspace(
        &decoded.sessions,
        decoded.active_id,
        &decoded.groups,
        &decoded.global_definitions,
        &decoded.preferences,
        &decoded.open_ids,
        &decoded.global_response_tokens,
    );
    assert_eq!(encoded, expected.trim_end());
}
```

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -p blink-core round_trips_v4_with_response_tokens`
Expected: compile error, `global_response_tokens` / `response_tokens` not found.

- [ ] **Step 3: Implement**

In `model.rs`, after `Capture`:

```rust
/// A token whose value comes from the last 2xx response of another request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseToken {
    pub id: u64,
    /// Token name, used as {{name}}.
    pub name: String,
    /// The source request (session id).
    pub request_id: u64,
    pub source: CheckSource,
    /// jq expression or header name, as in captures.
    pub path: String,
    /// None: send again only when no usable value exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_secs: Option<u64>,
}
```

In `RequestGroup`, after `local_definitions`:

```rust
    /// Tokens read from responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_tokens: Option<Vec<ResponseToken>>,
```

Fix every `RequestGroup { .. }` literal the compiler reports (tests use a `group()` helper; add `response_tokens: None`).

In `RequestSession`, after `stale`:

```rust
    /// The source request label while a response token dependency sends. Not saved.
    pub waiting_on: Option<String>,
```

Set `waiting_on: None` in `create_session` (`session.rs`) and `read_session` (`workspace.rs`), and in test helpers the compiler reports.

In `ids.rs`:

```rust
/// Response tokens.
pub static RESPONSE_TOKENS: Sequence = Sequence::new();
```

In `workspace.rs`:
- `encode_workspace` gets `global_response_tokens: &[ResponseToken]` as the last parameter. After building `snapshot`, before `js_numbers`:

```rust
    if !global_response_tokens.is_empty() {
        snapshot["globalResponseTokens"] = to_value(global_response_tokens);
    }
```

- `DecodedWorkspace` gets `pub global_response_tokens: Vec<ResponseToken>`. In `decode_workspace`:

```rust
    let global_response_tokens: Vec<ResponseToken> = match data.get("globalResponseTokens") {
        Some(value) => read(value.clone())?,
        None => Vec::new(),
    };
```

- Reserve ids: in the `for group in &groups` loop add `group.response_tokens.iter().flatten().for_each(|t| ids::RESPONSE_TOKENS.reserve(t.id));` and after it `global_response_tokens.iter().for_each(|t| ids::RESPONSE_TOKENS.reserve(t.id));`.
- Update every other `encode_workspace` caller (tests pass `&[]`).

In `workspace_state.rs`:
- Field `pub global_response_tokens: Vec<ResponseToken>` (doc: `/// Workspace-global response tokens.`), initialised `Vec::new()` in `new`, set in `restore`, cleared in `reset`, passed in `encode` and `autosave_key`.
- Setter next to `set_global_definitions`:

```rust
    pub fn set_global_response_tokens(&mut self, tokens: Vec<ResponseToken>) {
        self.global_response_tokens = tokens;
    }
```

- `GroupSettingsChanges` gets `pub response_tokens: Option<Option<Vec<ResponseToken>>>`; where the struct is applied (find `local_definitions` handling in the apply function), add the same pattern: `if let Some(tokens) = changes.response_tokens { group.response_tokens = tokens; }`.

- [ ] **Step 4: Run all core tests**

Run: `cargo test -p blink-core`
Expected: PASS, including every existing fixture round-trip.

- [ ] **Step 5: Commit**

```bash
git add crates/blink-core
git commit -m "feat: save response tokens on groups and the workspace"
```

---

### Task 2: Max age and row validation

**Files:**
- Create: `crates/blink-core/src/response_tokens.rs`
- Modify: `crates/blink-core/src/lib.rs` (`pub mod response_tokens;`)

**Interfaces:**
- Produces:
  - `parse_max_age(text: &str) -> Result<Option<u64>, String>` — `""` → `Ok(None)`; `30s`, `15m`, `1h`, `2d` → seconds; else `Err("Enter a max age such as 30s, 15m, or 1h.")`
  - `format_max_age(secs: Option<u64>) -> String` — largest exact unit (`900` → `15m`, `3600` → `1h`, `90` → `90s`), `None` → `""`
  - `RESPONSE_TOKEN_SOURCES: [CheckSource; 4] = [Json, Header, Body, Status]`
  - `validate_response_tokens(tokens: &[ResponseToken], text_names: &[&str], sessions: &[RequestSession]) -> Vec<(u64, String)>` — per token id, first error

- [ ] **Step 1: Write the failing tests** (in `response_tokens.rs` `#[cfg(test)] mod tests`)

```rust
use super::*;
use crate::model::{CheckSource, ResponseToken};
use crate::session::create_session;

fn token(id: u64, name: &str, request_id: u64) -> ResponseToken {
    ResponseToken {
        id,
        name: name.into(),
        request_id,
        source: CheckSource::Json,
        path: ".access_token".into(),
        max_age_secs: None,
    }
}

#[test]
fn parses_and_formats_max_age() {
    assert_eq!(parse_max_age(""), Ok(None));
    assert_eq!(parse_max_age(" 15m "), Ok(Some(900)));
    assert_eq!(parse_max_age("1h"), Ok(Some(3600)));
    assert_eq!(parse_max_age("30s"), Ok(Some(30)));
    assert_eq!(parse_max_age("2d"), Ok(Some(172_800)));
    assert!(parse_max_age("15").is_err());
    assert!(parse_max_age("0m").is_err());
    assert!(parse_max_age("m").is_err());
    assert_eq!(format_max_age(Some(900)), "15m");
    assert_eq!(format_max_age(Some(3600)), "1h");
    assert_eq!(format_max_age(Some(90)), "90s");
    assert_eq!(format_max_age(None), "");
}

#[test]
fn validation_names_the_first_problem_of_each_row() {
    let login = create_session(None);
    let sessions = vec![login.clone()];
    let tokens = vec![
        token(1, "access_token", login.id),
        token(2, "access_token", login.id),
        token(3, "host", login.id),
        token(4, "orphan", 9_999),
        token(5, "", login.id),
    ];
    let errors = validate_response_tokens(&tokens, &["host"], &sessions);
    assert_eq!(
        errors,
        vec![
            (2, "Another token is named \"access_token\".".to_string()),
            (3, "Another token is named \"host\".".to_string()),
            (4, "Choose a request.".to_string()),
            (5, "Enter a token name.".to_string()),
        ]
    );
}

#[test]
fn a_path_is_required_for_json_and_header() {
    let login = create_session(None);
    let mut row = token(1, "t", login.id);
    row.path = " ".into();
    let errors = validate_response_tokens(&[row.clone()], &[], &[login.clone()]);
    assert_eq!(errors, vec![(1, "Enter a path.".to_string())]);
    row.source = CheckSource::Status;
    assert!(validate_response_tokens(&[row], &[], &[login]).is_empty());
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink-core response_tokens`
Expected: compile errors for missing functions.

- [ ] **Step 3: Implement**

```rust
//! Tokens whose values come from another request's response: max age,
//! validation, resolution, and the send plan.

use crate::checks::is_capture_name;
use crate::model::{CheckSource, RequestSession, ResponseToken};

/// Sources a response token can read.
pub const RESPONSE_TOKEN_SOURCES: [CheckSource; 4] = [
    CheckSource::Json,
    CheckSource::Header,
    CheckSource::Body,
    CheckSource::Status,
];

const UNITS: [(char, u64); 4] = [('d', 86_400), ('h', 3_600), ('m', 60), ('s', 1)];

/// `15m` to 900 seconds. Empty means no max age.
pub fn parse_max_age(text: &str) -> Result<Option<u64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let error = || "Enter a max age such as 30s, 15m, or 1h.".to_string();
    let unit = text.chars().last().ok_or_else(error)?;
    let scale = UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, scale)| *scale)
        .ok_or_else(error)?;
    let count: u64 = text[..text.len() - 1].parse().map_err(|_| error())?;
    if count == 0 {
        return Err(error());
    }
    count.checked_mul(scale).map(Some).ok_or_else(error)
}

/// 900 seconds to `15m`, in the largest exact unit.
pub fn format_max_age(secs: Option<u64>) -> String {
    let Some(secs) = secs else {
        return String::new();
    };
    let (unit, scale) = UNITS
        .iter()
        .find(|(_, scale)| secs % scale == 0)
        .copied()
        .unwrap_or(('s', 1));
    format!("{}{unit}", secs / scale)
}

/// The first problem of each row, by token id. `text_names` are the text
/// tokens of the same scope.
pub fn validate_response_tokens(
    tokens: &[ResponseToken],
    text_names: &[&str],
    sessions: &[RequestSession],
) -> Vec<(u64, String)> {
    let mut seen: Vec<&str> = text_names.to_vec();
    let mut errors = Vec::new();
    for token in tokens {
        let name = token.name.trim();
        let error = if name.is_empty() {
            Some("Enter a token name.".to_string())
        } else if !is_capture_name(name) {
            Some("Use letters, digits, _, -, or . in token names.".to_string())
        } else if seen.contains(&name) {
            Some(format!("Another token is named \"{name}\"."))
        } else if !sessions.iter().any(|s| s.id == token.request_id) {
            Some("Choose a request.".to_string())
        } else if token.source.takes_path() && token.path.trim().is_empty() {
            Some("Enter a path.".to_string())
        } else {
            None
        };
        seen.push(name);
        if let Some(error) = error {
            errors.push((token.id, error));
        }
    }
    errors
}
```

Before writing the name rule, read `checks::is_capture_name` (`crates/blink-core/src/checks.rs:14`) and copy its error text from where captures report it (grep `is_capture_name` in `crates/blink`) so the message matches captures exactly. If the capture message differs from `"Use letters, digits, _, -, or . in token names."`, use the capture message and update the test.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink-core response_tokens`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/blink-core/src/response_tokens.rs crates/blink-core/src/lib.rs
git commit -m "feat: parse max ages and validate response token rows"
```

---

### Task 3: The value cache and its file

**Files:**
- Create: `crates/blink-core/src/response_token_cache.rs`, `crates/blink-core/src/engine/response_token_state.rs`
- Modify: `crates/blink-core/src/lib.rs`, `crates/blink-core/src/engine/mod.rs`, `crates/blink-core/src/engine/paths.rs`, `crates/blink-core/src/workspace_state.rs`

**Interfaces:**
- Produces:
  - `ValueKey { source: CheckSource, path: String }` (Hash, Eq, serde)
  - `CacheEntry { request_id: u64, fingerprint: String, fetched_at_ms: u64, values: Vec<(ValueKey, String)> }`
  - `ResponseTokenCache` with:
    - `record(&mut self, request_id: u64, fingerprint: &str, fetched_at_ms: u64, values: Vec<(ValueKey, String)>)` — replaces the same `(request_id, fingerprint)` entry, keeps newest 8 per request
    - `lookup(&self, request_id: u64, fingerprint: &str, key: &ValueKey) -> Option<(&str, u64)>` — value and fetched time
    - `prune(&mut self, live_ids: &HashSet<u64>)`
    - `entries(&self) -> &[CacheEntry]`
  - `Paths::response_tokens() -> PathBuf` (`data_dir/response-tokens.json`)
  - `Engine::load_response_tokens(&self) -> ResponseTokenCache`, `Engine::save_response_tokens(&self, cache: &ResponseTokenCache) -> Result<(), String>`
  - `Workspace.response_cache: ResponseTokenCache` (not part of `encode`/`autosave_key`)

- [ ] **Step 1: Write failing tests** in `response_token_cache.rs`

```rust
use super::*;
use crate::model::CheckSource;

fn key(path: &str) -> ValueKey {
    ValueKey { source: CheckSource::Json, path: path.into() }
}

#[test]
fn record_replaces_the_same_fingerprint_and_keeps_others() {
    let mut cache = ResponseTokenCache::default();
    cache.record(1, "dev", 10, vec![(key(".t"), "a".into())]);
    cache.record(1, "prod", 20, vec![(key(".t"), "b".into())]);
    cache.record(1, "dev", 30, vec![(key(".t"), "c".into())]);
    assert_eq!(cache.lookup(1, "dev", &key(".t")), Some(("c", 30)));
    assert_eq!(cache.lookup(1, "prod", &key(".t")), Some(("b", 20)));
    assert_eq!(cache.lookup(1, "dev", &key(".other")), None);
    assert_eq!(cache.lookup(2, "dev", &key(".t")), None);
}

#[test]
fn keeps_the_newest_eight_entries_per_request() {
    let mut cache = ResponseTokenCache::default();
    for n in 0..10u64 {
        cache.record(1, &format!("f{n}"), n, vec![]);
    }
    cache.record(2, "x", 0, vec![]);
    assert_eq!(cache.entries().iter().filter(|e| e.request_id == 1).count(), 8);
    assert!(cache.entries().iter().all(|e| e.fingerprint != "f0" && e.fingerprint != "f1"));
    assert_eq!(cache.entries().iter().filter(|e| e.request_id == 2).count(), 1);
}

#[test]
fn prune_drops_entries_of_missing_requests() {
    let mut cache = ResponseTokenCache::default();
    cache.record(1, "a", 0, vec![]);
    cache.record(2, "a", 0, vec![]);
    cache.prune(&[2].into_iter().collect());
    assert_eq!(cache.entries().len(), 1);
    assert_eq!(cache.entries()[0].request_id, 2);
}

#[test]
fn round_trips_through_json() {
    let mut cache = ResponseTokenCache::default();
    cache.record(1, "a", 5, vec![(key(".t"), "v".into())]);
    let text = serde_json::to_string(&cache).unwrap();
    let back: ResponseTokenCache = serde_json::from_str(&text).unwrap();
    assert_eq!(back, cache);
}
```

In `engine/response_token_state.rs` tests, copy `update_state.rs`'s test shape:

```rust
#[test]
fn cache_round_trips_and_defaults_when_missing_or_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data").join("response-tokens.json");
    let state = ResponseTokenState::new(path.clone());
    assert_eq!(state.load(), ResponseTokenCache::default());
    let mut cache = ResponseTokenCache::default();
    cache.record(1, "f", 1, vec![]);
    state.save(&cache).unwrap();
    assert_eq!(state.load(), cache);
    std::fs::write(&path, b"{not json").unwrap();
    assert_eq!(state.load(), ResponseTokenCache::default());
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink-core response_token`
Expected: compile errors.

- [ ] **Step 3: Implement**

`response_token_cache.rs`:

```rust
//! Values read from 2xx responses of source requests, by request and by
//! the fingerprint of the resolved request that was sent. The fingerprint
//! includes environment values, so each environment keeps its own value.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::CheckSource;

/// Entries kept per request, newest first.
const ENTRIES_PER_REQUEST: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueKey {
    pub source: CheckSource,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    pub request_id: u64,
    pub fingerprint: String,
    pub fetched_at_ms: u64,
    pub values: Vec<(ValueKey, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResponseTokenCache {
    /// Newest first.
    entries: Vec<CacheEntry>,
}

impl ResponseTokenCache {
    pub fn record(
        &mut self,
        request_id: u64,
        fingerprint: &str,
        fetched_at_ms: u64,
        values: Vec<(ValueKey, String)>,
    ) {
        self.entries
            .retain(|e| !(e.request_id == request_id && e.fingerprint == fingerprint));
        self.entries.insert(
            0,
            CacheEntry {
                request_id,
                fingerprint: fingerprint.to_string(),
                fetched_at_ms,
                values,
            },
        );
        let mut kept = 0;
        self.entries.retain(|e| {
            if e.request_id != request_id {
                return true;
            }
            kept += 1;
            kept <= ENTRIES_PER_REQUEST
        });
    }

    pub fn lookup(&self, request_id: u64, fingerprint: &str, key: &ValueKey) -> Option<(&str, u64)> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.request_id == request_id && e.fingerprint == fingerprint)?;
        entry
            .values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| (value.as_str(), entry.fetched_at_ms))
    }

    pub fn prune(&mut self, live_ids: &HashSet<u64>) {
        self.entries.retain(|e| live_ids.contains(&e.request_id));
    }

    pub fn entries(&self) -> &[CacheEntry] {
        &self.entries
    }
}
```

`engine/response_token_state.rs`: copy `update_state.rs` structure exactly, replacing `UpdateChoices` with `ResponseTokenCache`, the struct name with `ResponseTokenState`, and the error text with `"Cannot save the response tokens."`.

`paths.rs`:

```rust
    pub fn response_tokens(&self) -> PathBuf {
        self.data_dir.join("response-tokens.json")
    }
```

`engine/mod.rs`: add `mod response_token_state;`, a field `response_tokens: response_token_state::ResponseTokenState` built with `ResponseTokenState::new(paths.response_tokens())`, and:

```rust
    pub fn load_response_tokens(&self) -> ResponseTokenCache {
        self.0.response_tokens.load()
    }

    pub fn save_response_tokens(&self, cache: &ResponseTokenCache) -> Result<(), String> {
        self.0.response_tokens.save(cache)
    }
```

`workspace_state.rs`: field `pub response_cache: ResponseTokenCache` with doc `/// Values read from responses. Saved by the store in its own file.`, `Default::default()` in `new`. Do NOT touch it in `restore`/`reset`/`encode`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/blink-core
git commit -m "feat: cache response token values per request and fingerprint"
```

---

### Task 4: Interpolation, hints, and display know response tokens

**Files:**
- Modify: `crates/blink-core/src/interpolation.rs`, `crates/blink-core/src/token_hints.rs`, `crates/blink-core/src/token_display.rs`, `crates/blink/src/ui/token_input.rs` (`state_id` match only)

**Interfaces:**
- Produces:
  - `interpolation::ResponseTokenInfo { request_label: String, source: CheckSource, path: String, fetched_at_ms: Option<u64>, environment: Option<String>, problem: Option<String> }` — `fetched_at_ms` is `Some` exactly when a usable value is in `definitions`/`workspace_definitions`; `problem` is the reason when there is none (`None` means "not sent yet").
  - `InterpolationContext.response_tokens: IndexMap<String, ResponseTokenInfo>` (local scopes), `InterpolationContext.workspace_response_tokens: IndexMap<String, ResponseTokenInfo>` (global)
  - `InterpolationContext::response_info(&self, name: &str) -> Option<&ResponseTokenInfo>` — `name` may start with `_.`; mirrors the lookup order of `interpolate`
  - `TokenState::Response` (a reference to a response token, usable or not)
  - `token_hint` texts:
    - usable: `From "Login" · JSON (jq) .access_token · 3 min ago · DEV` (`· DEV` only with an environment; age from `now_ms` argument)
    - not usable, no problem: `No current value · sends "Login" first`
    - not usable, with problem: `No current value · {problem}`
  - `token_hint(span, ctx, now_ms: f64)` gains `now_ms`; callers pass `blink_core::history::now_ms()`.
  - `age_label(ms: u64) -> String`: `< 60 s` → `just now`; `< 60 min` → `N min ago`; `< 24 h` → `N h ago`; else `N d ago`.
  - `TokenOption.value` for a response token is `from "Login"`.

- [ ] **Step 1: Write failing tests**

In `interpolation.rs` tests:

```rust
fn info(fetched: Option<u64>) -> ResponseTokenInfo {
    ResponseTokenInfo {
        request_label: "Login".into(),
        source: CheckSource::Json,
        path: ".access_token".into(),
        fetched_at_ms: fetched,
        environment: None,
        problem: None,
    }
}

#[test]
fn a_response_token_without_a_value_names_its_request() {
    let mut ctx = both(&[], &[("access_token", "global-text")]);
    ctx.response_tokens.insert("access_token".into(), info(None));
    assert_eq!(
        interpolate("{{access_token}}", &ctx).unwrap_err(),
        "\"access_token\" has no current value. Send \"Login\" or the request that uses it."
    );
}

#[test]
fn a_response_token_with_a_value_resolves() {
    let mut ctx = local(&[("access_token", "abc")]);
    ctx.response_tokens.insert("access_token".into(), info(Some(1)));
    assert_eq!(interpolate("Bearer {{access_token}}", &ctx).unwrap(), "Bearer abc");
}

#[test]
fn a_global_response_token_without_a_value_blocks_the_bare_name() {
    let mut ctx = both(&[], &[]);
    ctx.workspace_response_tokens.insert("csrf".into(), info(None));
    assert!(interpolate("{{csrf}}", &ctx).unwrap_err().starts_with("\"csrf\" has no current value"));
    assert!(interpolate("{{_.csrf}}", &ctx).unwrap_err().starts_with("\"csrf\" has no current value"));
}
```

In `token_hints.rs` tests:

```rust
#[test]
fn response_tokens_hint_their_source_never_their_value() {
    let mut ctx = InterpolationContext::local(defs(&[("access_token", "secret-value")]));
    ctx.response_tokens.insert(
        "access_token".into(),
        ResponseTokenInfo {
            request_label: "Login".into(),
            source: CheckSource::Json,
            path: ".access_token".into(),
            fetched_at_ms: Some(1_000),
            environment: Some("DEV".into()),
            problem: None,
        },
    );
    let span = &token_spans("{{access_token}}", Some(&ctx))[0];
    assert_eq!(span.token, Some(TokenState::Response));
    let hint = token_hint(span, Some(&ctx), 1_000.0 + 3.0 * 60_000.0);
    assert_eq!(hint, "From \"Login\" · JSON (jq) .access_token · 3 min ago · DEV");
    assert!(!hint.contains("secret-value"));
    let option = token_options(Some(&ctx)).into_iter().find(|o| o.name == "access_token").unwrap();
    assert_eq!(option.value, "from \"Login\"");
    assert_eq!(resolve_for_display("{{access_token}}", Some(&ctx)), "{{access_token}}");
}

#[test]
fn a_response_token_without_a_value_hints_the_send() {
    let mut ctx = InterpolationContext::default();
    ctx.response_tokens.insert(
        "t".into(),
        ResponseTokenInfo {
            request_label: "Login".into(),
            source: CheckSource::Json,
            path: ".t".into(),
            fetched_at_ms: None,
            environment: None,
            problem: None,
        },
    );
    let span = &token_spans("{{t}}", Some(&ctx))[0];
    assert_eq!(span.token, Some(TokenState::Response));
    assert_eq!(token_hint(span, Some(&ctx), 0.0), "No current value · sends \"Login\" first");
}

#[test]
fn ages_read_in_the_largest_unit() {
    assert_eq!(age_label(59_000), "just now");
    assert_eq!(age_label(3 * 60_000), "3 min ago");
    assert_eq!(age_label(2 * 3_600_000), "2 h ago");
    assert_eq!(age_label(3 * 86_400_000), "3 d ago");
}
```

In `token_display.rs` tests:

```rust
#[test]
fn response_tokens_never_show_their_value() {
    let mut ctx = InterpolationContext::local(defs(&[("t", "secret")]));
    ctx.response_tokens.insert("t".into(), ResponseTokenInfo {
        request_label: "Login".into(),
        source: CheckSource::Json,
        path: ".t".into(),
        fetched_at_ms: Some(0),
        environment: None,
        problem: None,
    });
    let display = token_display("Bearer {{t}}", Some(&ctx));
    assert_eq!(display.text, "Bearer {{t}}");
    assert!(display.segments.iter().all(|s| !s.unit));
}
```

(Use each test module's existing `defs` helper; add imports for `ResponseTokenInfo` and `CheckSource`.)

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink-core -- interpolation token_hints token_display`
Expected: compile errors.

- [ ] **Step 3: Implement**

`interpolation.rs`:

```rust
/// What a context knows about a response token, for errors and hints.
/// Never holds the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseTokenInfo {
    pub request_label: String,
    pub source: CheckSource,
    pub path: String,
    /// Set when a usable value is in the definitions.
    pub fetched_at_ms: Option<u64>,
    pub environment: Option<String>,
    /// Why no value can be read, when the source request cannot supply one.
    pub problem: Option<String>,
}
```

Add the two `IndexMap<String, ResponseTokenInfo>` fields to `InterpolationContext` with docs `/// Response tokens of the group scopes, by name.` and `/// Workspace-global response tokens, by name.`; `new` and `local` set them to `IndexMap::new()`.

```rust
    /// The response token `name` refers to, in the order `interpolate` looks
    /// names up. `name` may start with `_.`.
    pub fn response_info(&self, name: &str) -> Option<&ResponseTokenInfo> {
        if let Some(name) = name.strip_prefix("_.") {
            return self.workspace_response_tokens.get(name);
        }
        if self.definitions.contains_key(name) && !self.response_tokens.contains_key(name) {
            return None;
        }
        self.response_tokens.get(name).or_else(|| {
            if self.definitions.contains_key(name) {
                None
            } else {
                self.workspace_response_tokens.get(name)
            }
        })
    }
```

In `replace`, replace the `let value = ...; let Some(value) = value else {...}` block with:

```rust
    let lookup = if workspace_only { format!("_.{name}") } else { name.to_string() };
    let value = if workspace_only {
        ctx.workspace_definitions.get(name)
    } else {
        ctx.definitions.get(name).or_else(|| {
            if ctx.response_tokens.contains_key(name) {
                None
            } else {
                ctx.workspace_definitions.get(name)
            }
        })
    };
    let Some(value) = value else {
        if let Some(info) = ctx.response_info(&lookup) {
            return Err(format!(
                "\"{name}\" has no current value. Send \"{}\" or the request that uses it.",
                info.request_label
            ));
        }
        return Err(format!(
            "Undefined token reference: \"{name}\". Define it in your {} variables.",
            if workspace_only { "workspace" } else { "group or workspace" }
        ));
    };
```

A usable response token value is a literal: it must NOT be re-interpolated (a token value could contain `{{`). After the `let Some(value)` block, before `stack.push`:

```rust
    if ctx.response_info(&lookup).is_some() {
        return Ok(value.clone());
    }
```

`token_hints.rs`:
- Add `Response` to `TokenState`.
- In `token_spans`, for a name: `if ctx.and_then(|c| c.response_info(&full)).is_some() { TokenState::Response } else if is_resolved(...) { Resolved } else { Unresolved }` where `full` is the `_.`-prefixed name.
- `token_value`: return `None` when `ctx.response_info(name).is_some()` (so `resolve_for_display` keeps the reference).
- `token_options`: build the option value with `ctx.response_info(&name).map(|i| format!("from \"{}\"", i.request_label)).or_else(|| token_value(&name, Some(ctx))).unwrap_or_default()`. Also list response tokens that have no value yet: chain `ctx.response_tokens.keys()` not already in `definitions` as `Local`, and `ctx.workspace_response_tokens.keys()` as `Global` (bare when not shadowed, plus `_.` form), keeping the existing dedupe rules.
- `token_hint(span, ctx, now_ms: f64)`:

```rust
    if span.token == Some(TokenState::Response)
        && let Some(info) = ctx.and_then(|ctx| ctx.response_info(name))
    {
        return match (info.fetched_at_ms, &info.problem) {
            (Some(at), _) => {
                let age = age_label((now_ms as u64).saturating_sub(at));
                let mut hint = format!(
                    "From \"{}\" · {} {} · {age}",
                    info.request_label,
                    info.source.label(),
                    info.path.trim()
                );
                if let Some(environment) = &info.environment {
                    hint.push_str(&format!(" · {environment}"));
                }
                hint
            }
            (None, Some(problem)) => format!("No current value · {problem}"),
            (None, None) => format!("No current value · sends \"{}\" first", info.request_label),
        };
    }
```

  For `Status` and `Body` sources the path is empty; trim the trailing space: build `format!("{} {}", label, path).trim_end()`.
- `pub fn age_label(ms: u64) -> String` per the interface.
- Update every `token_hint` caller (`crates/blink/src/ui/token_input.rs` and any other grep hit) to pass `blink_core::history::now_ms()`.

`token_display.rs`: in `token_display`, the `value` match only accepts `TokenState::Resolved`, so `Response` already yields `None`. Confirm by the test; no code change expected beyond imports.

`crates/blink/src/ui/token_input.rs`: `state_id` gains `TokenState::Response => "response"`; in `render_token` add an arm:

```rust
            "response" => this.text_color(colors.info).child(
                div()
                    .rounded(px(2.))
                    .bg(colors.info.opacity(0.15))
                    .child(token.token().label().clone()),
            ),
```

Check `crate::theme::colors` has an `info` field (grep `pub info` in `crates/blink/src/theme.rs`). If it does not, use the field the environment color `Info` maps to in `environment_color_id`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink-core && cargo test -p blink token_input`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat: hint response tokens by source and never show their values"
```

---

### Task 5: Resolve response tokens from the cache (`TokenSources`)

**Files:**
- Modify: `crates/blink-core/src/response_tokens.rs`, `crates/blink-core/src/workspace_state.rs`

**Interfaces:**
- Consumes: Task 1 model, Task 3 cache, Task 4 `ResponseTokenInfo`.
- Produces:

```rust
pub struct TokenSources<'a> {
    pub groups: &'a [RequestGroup],
    pub globals: &'a Definitions,
    pub global_response_tokens: &'a [ResponseToken],
    pub sessions: &'a [RequestSession],
    pub cache: &'a ResponseTokenCache,
    pub now_ms: f64,
}
impl<'a> TokenSources<'a> {
    /// Text tokens only: no response tokens, empty cache.
    pub fn text(groups: &'a [RequestGroup], globals: &'a Definitions) -> Self;
    /// Merged group and global tokens for `group_id`, with response tokens.
    pub fn context(&self, group_id: Option<u64>) -> InterpolationContext;
    /// The resolved auth and tokens of `session`.
    pub fn request_context(&self, session: &RequestSession) -> ResolvedRequestContext;
    /// The fingerprint `session` would send now.
    pub fn fingerprint(&self, session: &RequestSession) -> String;
}
pub fn deleted_request_problem() -> String; // "reads a deleted request."
impl Workspace { pub fn token_sources(&self, now_ms: f64) -> TokenSources<'_>; }
```

`TokenSources::text` uses a `static EMPTY_CACHE: LazyLock<ResponseTokenCache>`, `&[]` for tokens and sessions, and `now_ms: 0.0`.

Resolution rules for `context(group_id)`:
1. Walk the ancestry nearest first (reuse `authorization::resolve_token_definitions` for text tokens; to interleave scopes, re-implement the walk here using `authorization::ancestry` — make that fn `pub(crate)`).
2. Per group: text tokens `or_insert` into `definitions` as today; then each response token whose name is not yet in `definitions` or `response_tokens` is claimed: insert `ResponseTokenInfo` into `response_tokens`, and when usable insert the value into `definitions`.
3. Globals: text into `workspace_definitions`; global response tokens into `workspace_response_tokens` (+ value into `workspace_definitions` when usable), skipping names already in `globals`.
4. Usable = source session exists AND `cache.lookup(request_id, &self.fingerprint(source), &ValueKey{source, path})` is `Some((value, at))` AND (`max_age_secs` is None OR `now_ms - at <= max_age_secs * 1000`).
5. `request_label` = `session_label(source, Some(LabelTokens{groups, global_definitions: globals}))`. Missing source: label `"a deleted request"`, `problem: Some("reads a deleted request.")`.
6. `environment` = `environments::request_environment(source.group_id, groups).map(|e| e.name.clone())`.
7. Recursion guard: `fingerprint(source)` builds the source's own context, which may contain response tokens reading the source itself. Keep a `RefCell<Vec<u64>>` stack of request ids on `TokenSources` (field `visiting`, private, initialised empty in both constructors). In `fingerprint`, if `session.id` is on the stack, return `String::new()` (no entry ever has an empty fingerprint, so the lookup misses and the token reads "no value"). Push before building, pop after.

- [ ] **Step 1: Write failing tests** (in `response_tokens.rs` tests; add helpers)

```rust
use crate::model::{Definitions, RequestGroup};
use crate::response_token_cache::{ResponseTokenCache, ValueKey};

fn group(id: u64, tokens: Vec<ResponseToken>) -> RequestGroup {
    RequestGroup {
        id,
        name: format!("G{id}"),
        parent_id: None,
        collapsed: false,
        local_auth: None,
        local_definitions: None,
        response_tokens: Some(tokens),
        default_method: None,
        default_url: None,
        environments: None,
        active_environment_id: None,
    }
}

fn login(group_id: u64) -> RequestSession {
    let mut session = create_session(None);
    session.group_id = Some(group_id);
    session.draft.url = "https://api.test/login".into();
    session
}

fn sources<'a>(
    groups: &'a [RequestGroup],
    globals: &'a Definitions,
    sessions: &'a [RequestSession],
    cache: &'a ResponseTokenCache,
    now_ms: f64,
) -> TokenSources<'a> {
    TokenSources::new(groups, globals, &[], sessions, cache).at(now_ms)
}
```

(Add `pub fn new(groups, globals, global_response_tokens, sessions, cache) -> Self` with `now_ms: 0.0` to the interface; tests and `Workspace::token_sources` use it.)

```rust
#[test]
fn a_cached_value_for_the_current_fingerprint_resolves() {
    let source = login(1);
    let mut token = token(5, "access_token", source.id);
    token.max_age_secs = Some(60);
    let groups = vec![group(1, vec![token])];
    let sessions = vec![source.clone()];
    let globals = Definitions::new();
    let mut cache = ResponseTokenCache::default();
    let fingerprint = TokenSources::new(&groups, &globals, &[], &sessions, &cache).fingerprint(&source);
    cache.record(source.id, &fingerprint, 1_000, vec![(
        ValueKey { source: CheckSource::Json, path: ".access_token".into() },
        "abc".into(),
    )]);

    let fresh = sources(&groups, &globals, &sessions, &cache, 1_000.0 + 60_000.0).context(Some(1));
    assert_eq!(fresh.definitions["access_token"], "abc");
    assert_eq!(fresh.response_tokens["access_token"].fetched_at_ms, Some(1_000));

    let old = sources(&groups, &globals, &sessions, &cache, 1_000.0 + 60_001.0).context(Some(1));
    assert!(!old.definitions.contains_key("access_token"));
    assert_eq!(old.response_tokens["access_token"].fetched_at_ms, None);
}

#[test]
fn another_fingerprint_does_not_resolve() {
    let source = login(1);
    let groups = vec![group(1, vec![token(5, "access_token", source.id)])];
    let sessions = vec![source.clone()];
    let globals = Definitions::new();
    let mut cache = ResponseTokenCache::default();
    cache.record(source.id, "some other request", 0, vec![(
        ValueKey { source: CheckSource::Json, path: ".access_token".into() },
        "abc".into(),
    )]);
    let ctx = sources(&groups, &globals, &sessions, &cache, 0.0).context(Some(1));
    assert!(!ctx.definitions.contains_key("access_token"));
}

#[test]
fn a_nearer_text_token_wins_over_a_farther_response_token() {
    let source = login(1);
    let mut parent = group(1, vec![token(5, "t", source.id)]);
    parent.local_definitions = None;
    let mut child = group(2, vec![]);
    child.parent_id = Some(1);
    child.local_definitions = Some([("t".to_string(), "text".to_string())].into_iter().collect());
    let groups = vec![parent, child];
    let sessions = vec![source];
    let globals = Definitions::new();
    let cache = ResponseTokenCache::default();
    let ctx = sources(&groups, &globals, &sessions, &cache, 0.0).context(Some(2));
    assert_eq!(ctx.definitions["t"], "text");
    assert!(ctx.response_info("t").is_none());
}

#[test]
fn a_deleted_source_reports_a_problem() {
    let groups = vec![group(1, vec![token(5, "t", 9_999)])];
    let globals = Definitions::new();
    let cache = ResponseTokenCache::default();
    let ctx = sources(&groups, &globals, &[], &cache, 0.0).context(Some(1));
    assert_eq!(ctx.response_tokens["t"].problem.as_deref(), Some("reads a deleted request."));
}

#[test]
fn a_source_that_uses_its_own_token_does_not_recurse() {
    let mut source = login(1);
    source.draft.headers.push(crate::request::pair("Authorization", "Bearer {{t}}"));
    let groups = vec![group(1, vec![token(5, "t", source.id)])];
    let sessions = vec![source.clone()];
    let globals = Definitions::new();
    let cache = ResponseTokenCache::default();
    let all = sources(&groups, &globals, &sessions, &cache, 0.0);
    let _ = all.context(Some(1));
    let _ = all.fingerprint(&source);
}
```

(Check `crate::request::pair`'s signature before using it; it is used in `environment_tokens.rs` as `blink_core::request::pair`.)

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink-core response_tokens`
Expected: compile errors.

- [ ] **Step 3: Implement** `TokenSources` per the rules above. Sketch of `fingerprint` and `request_context`:

```rust
    pub fn fingerprint(&self, session: &RequestSession) -> String {
        if self.visiting.borrow().contains(&session.id) {
            return String::new();
        }
        self.visiting.borrow_mut().push(session.id);
        let ctx = self.request_context(session);
        let fingerprint = crate::runner::prepare(&session.draft, Some(&ctx)).fingerprint();
        self.visiting.borrow_mut().pop();
        fingerprint
    }

    pub fn request_context(&self, session: &RequestSession) -> ResolvedRequestContext {
        ResolvedRequestContext {
            auth: resolve_authorization(session.draft.local_auth.as_ref(), session.group_id, self.groups),
            tokens: self.context(session.group_id),
        }
    }
```

Add to `Workspace`:

```rust
    /// Tokens, response tokens, and cached values at `now_ms`.
    pub fn token_sources(&self, now_ms: f64) -> TokenSources<'_> {
        TokenSources {
            now_ms,
            ..TokenSources::new(
                &self.groups,
                &self.global_definitions,
                &self.global_response_tokens,
                &self.sessions,
                &self.response_cache,
            )
        }
    }
```

(Struct update syntax with a private `visiting` field only works inside the module; outside it, add `pub fn at(mut self, now_ms: f64) -> Self` and use `TokenSources::new(...).at(now_ms)` instead. Use `.at()` everywhere, including the tests above.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/blink-core
git commit -m "feat: resolve response tokens from cached values"
```

---

### Task 6: Send, staleness, cURL, and editors use `TokenSources`

**Files:**
- Modify: `crates/blink-core/src/runner.rs`, `crates/blink-core/src/session_curl.rs`, `crates/blink-core/src/workspace_state.rs`, `crates/blink/src/runner.rs`, `crates/blink/src/ui/request_pane/common.rs`, `crates/blink/src/ui/browser/menus.rs`, any other compile error site.

**Interfaces:**
- Consumes: `TokenSources` (Task 5).
- Produces (replacing the `groups, globals` pairs):
  - `runner::request_context(session: &RequestSession, sources: &TokenSources) -> ResolvedRequestContext`
  - `runner::prepare_send(session, sources: &TokenSources, preferences) -> PreparedSend`
  - `runner::refresh_stale(session: &mut RequestSession, sources: &TokenSources) -> bool` — NOTE: callers holding `&mut` to a session inside the workspace cannot also borrow `sources`; use the workspace methods below.
  - `Workspace::refresh_stale(&mut self, id: u64) -> bool` and `Workspace::refresh_all_stale(&mut self)` — compute with `token_sources(history::now_ms())` into a `Vec<(u64, bool)>`, then write `session.stale`.
  - `session_curl(session, sources: &TokenSources, options) -> String`
- Unchanged: `authorization::resolve_token_definitions` and `build_resolved_request_context` (text only; still used by labels and the v<3 migration).

- [ ] **Step 1: Write the failing test** in `runner.rs` tests

```rust
#[test]
fn a_new_cached_value_marks_the_dependent_stale() {
    use crate::model::{CheckSource, ResponseToken};
    use crate::response_token_cache::ValueKey;
    use crate::workspace_state::Workspace;

    let mut workspace = Workspace::new();
    let login = workspace.sessions[0].id;
    let group_id = workspace.add_group("API", None);
    workspace.move_request(login, Some(group_id));
    workspace.sessions[0].draft.url = "https://api.test/login".into();
    let mut dependent = crate::session::create_session(None);
    dependent.group_id = Some(group_id);
    dependent.draft.url = "https://api.test/me?t={{t}}".into();
    let dependent_id = dependent.id;
    workspace.sessions.push(dependent);
    workspace.groups[0].response_tokens = Some(vec![ResponseToken {
        id: 1,
        name: "t".into(),
        request_id: login,
        source: CheckSource::Json,
        path: ".t".into(),
        max_age_secs: None,
    }]);
    let record = |workspace: &mut Workspace, value: &str| {
        let fingerprint = workspace
            .token_sources(0.0)
            .fingerprint(workspace.session(login).unwrap());
        workspace.response_cache.record(login, &fingerprint, 0, vec![(
            ValueKey { source: CheckSource::Json, path: ".t".into() },
            value.into(),
        )]);
    };
    record(&mut workspace, "one");
    let sources = workspace.token_sources(0.0);
    let prepared = prepare_send(workspace.session(dependent_id).unwrap(), &sources, &workspace.preferences);
    assert_eq!(prepared.http.error(), "");
    let fingerprint = prepared.http.fingerprint();
    let session = workspace.session_mut(dependent_id).unwrap();
    session.sent_fingerprint = fingerprint;
    session.response = Some(crate::model::ApiResponse::default());
    assert!(!workspace.refresh_stale(dependent_id));
    record(&mut workspace, "two");
    assert!(workspace.refresh_stale(dependent_id));
}
```

(If `ApiResponse` has no `Default`, build one the way the existing runner tests do — grep `ApiResponse {` in `runner.rs` tests.)

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -p blink-core a_new_cached_value_marks_the_dependent_stale`
Expected: compile error (signatures).

- [ ] **Step 3: Implement** the signature changes; fix existing tests by building `TokenSources::text(&groups, &globals)`. In the app:
  - `crates/blink/src/runner.rs` `send_http`/`connect`: `let sources = workspace.token_sources(now_ms()); let prepared = prepare_send(session, &sources, &workspace.preferences);`
  - `finish`: replace the manual `refresh_stale` loop with `workspace.refresh_all_stale();`
  - `common.rs::edit_draft`: after `change(...)`, call `workspace.refresh_stale(id);` (remove the index juggling).
  - `common.rs::session_context`: `workspace.token_sources(now_ms()).request_context(session)`.
  - `menus.rs:136` and any other `session_curl` / `request_context` caller: pass `&workspace.token_sources(now_ms())`.
  - `use blink_core::history::now_ms;` where needed.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink-core && cargo test -p blink`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "refactor: resolve requests through token sources"
```

---

### Task 7: The send plan

**Files:**
- Modify: `crates/blink-core/src/response_tokens.rs`

**Interfaces:**
- Consumes: `TokenSources`, `runner::prepare`, `ResolvedRequestContext`.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    /// The source request to send.
    pub request_id: u64,
    /// The token that needs it, for messages.
    pub token: String,
}
/// Source requests to send before `session_id`, in order. Empty when every
/// response token it uses has a usable value. Err for a cycle or a deleted source.
pub fn response_token_plan(session_id: u64, sources: &TokenSources) -> Result<Vec<PlanStep>, String>;
/// `Could not get "{token}": "{label}" returned {status}.` etc.
pub fn dependency_error(token: &str, label: &str, failure: &DependencyFailure) -> String;
pub enum DependencyFailure { Status(u16), Error(String), Missing { path: String }, Cancelled }
```

Messages (exact):
- `Status(401)` → `Could not get "access_token": "Login" returned 401.`
- `Error(e)` → `Could not get "access_token": "Login" failed. {e}` (the engine error text never includes token values)
- `Missing { path }` → `Could not get "access_token": "Login" has no value at {path}.` (for `Status`/`Body` sources with empty path use `has no value.`)
- `Cancelled` → `Could not get "access_token": "Login" was cancelled.`
- Cycle → `Response token cycle: Login → Refresh → Login`
- Deleted → `"access_token" reads a deleted request.`

How the plan finds needed tokens (the probe):
1. `ctx = sources.request_context(session)`.
2. Clone `ctx` into `probe`. For each name in `probe.response_tokens` / `workspace_response_tokens` with `fetched_at_ms == None`, insert a marker value `format!("\u{1}rt:{key}\u{1}")` into the matching definitions map, where `key` is the name (global: `_.name`). Remove its info from the response maps in `probe` so `interpolate` treats it as text.
3. `runner::prepare(&session.draft, Some(&probe))`; if it errors, return `Ok(vec![])` (the send reports the real error itself). Otherwise scan URL, header values, body, and multipart values of the built `RequestInput` for markers; also run `build_web_socket_request(&draft, Some(&probe))` when the URL is a WebSocket URL and scan its URL and headers.
4. For each found key in order of first appearance: get its `ResponseTokenInfo`; if `problem` is set, return `Err(format!("\"{name}\" {problem}"))`. Find the token's `request_id` (look up the claiming `ResponseToken` — keep a private `fn claimed(&self, group_id, name) -> Option<&ResponseToken>` in `TokenSources` that walks scopes the same way as `context`).
5. Recurse into the source with a `stack: Vec<u64>` of request ids (starting with `[session_id]`); if the source is already on the stack, return the cycle error built from the labels on the stack plus the repeated label. Append the source's own steps first, then `PlanStep { request_id, token }`, skipping a request id already in the result.

- [ ] **Step 1: Write failing tests**

```rust
fn workspace_with_login() -> (crate::workspace_state::Workspace, u64, u64) {
    let mut workspace = crate::workspace_state::Workspace::new();
    let login = workspace.sessions[0].id;
    let group_id = workspace.add_group("API", None);
    workspace.move_request(login, Some(group_id));
    workspace.sessions[0].draft.url = "https://api.test/login".into();
    workspace.groups[0].response_tokens = Some(vec![token(1, "access_token", login)]);
    let mut me = create_session(None);
    me.group_id = Some(group_id);
    me.draft.url = "https://api.test/me".into();
    me.draft.headers.push(crate::request::pair("Authorization", "Bearer {{access_token}}"));
    let me_id = me.id;
    workspace.sessions.push(me);
    (workspace, login, me_id)
}

#[test]
fn plans_the_source_when_no_value_exists() {
    let (workspace, login, me) = workspace_with_login();
    let plan = response_token_plan(me, &workspace.token_sources(0.0)).unwrap();
    assert_eq!(plan, vec![PlanStep { request_id: login, token: "access_token".into() }]);
}

#[test]
fn plans_nothing_when_the_value_is_usable() {
    let (mut workspace, login, me) = workspace_with_login();
    let fingerprint = workspace.token_sources(0.0).fingerprint(workspace.session(login).unwrap());
    workspace.response_cache.record(login, &fingerprint, 0, vec![(
        ValueKey { source: CheckSource::Json, path: ".access_token".into() },
        "abc".into(),
    )]);
    assert!(response_token_plan(me, &workspace.token_sources(0.0)).unwrap().is_empty());
}

#[test]
fn finds_tokens_through_text_tokens_and_ignores_disabled_headers() {
    let (mut workspace, _, me) = workspace_with_login();
    let session = workspace.session_mut(me).unwrap();
    session.draft.headers.clear();
    let mut disabled = crate::request::pair("X-Unused", "{{access_token}}");
    disabled.enabled = false;
    session.draft.headers.push(disabled);
    assert!(response_token_plan(me, &workspace.token_sources(0.0)).unwrap().is_empty());
    workspace.groups[0].local_definitions =
        Some([("auth".to_string(), "Bearer {{access_token}}".to_string())].into_iter().collect());
    workspace.session_mut(me).unwrap().draft.headers.push(crate::request::pair("Authorization", "{{auth}}"));
    assert_eq!(response_token_plan(me, &workspace.token_sources(0.0)).unwrap().len(), 1);
}

#[test]
fn orders_chains_and_reports_cycles() {
    let (mut workspace, login, me) = workspace_with_login();
    let mut refresh = create_session(None);
    refresh.group_id = workspace.sessions[0].group_id;
    refresh.draft.url = "https://api.test/refresh?r={{refresh_token}}".into();
    let refresh_id = refresh.id;
    workspace.sessions.push(refresh);
    let tokens = workspace.groups[0].response_tokens.as_mut().unwrap();
    tokens[0].request_id = refresh_id; // access_token now comes from Refresh
    tokens.push(token(2, "refresh_token", login));
    let plan = response_token_plan(me, &workspace.token_sources(0.0)).unwrap();
    assert_eq!(
        plan.iter().map(|s| s.request_id).collect::<Vec<_>>(),
        vec![login, refresh_id]
    );
    // Login now needs access_token: Login → Refresh → Login.
    workspace.session_mut(login).unwrap().draft.url = "https://api.test/login?a={{access_token}}".into();
    let error = response_token_plan(me, &workspace.token_sources(0.0)).unwrap_err();
    assert!(error.starts_with("Response token cycle: "), "{error}");
}

#[test]
fn a_deleted_source_fails_the_plan() {
    let (mut workspace, _, me) = workspace_with_login();
    workspace.groups[0].response_tokens.as_mut().unwrap()[0].request_id = 9_999;
    assert_eq!(
        response_token_plan(me, &workspace.token_sources(0.0)).unwrap_err(),
        "\"access_token\" reads a deleted request."
    );
}

#[test]
fn dependency_errors_name_the_token_and_request_only() {
    assert_eq!(
        dependency_error("access_token", "Login", &DependencyFailure::Status(401)),
        "Could not get \"access_token\": \"Login\" returned 401."
    );
    assert_eq!(
        dependency_error("t", "Login", &DependencyFailure::Missing { path: ".t".into() }),
        "Could not get \"t\": \"Login\" has no value at .t."
    );
}
```

(`Pair` has an `enabled` field — check `crates/blink-core/src/model.rs:16`.)

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink-core response_tokens`
Expected: compile errors.

- [ ] **Step 3: Implement** per the probe description.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/blink-core
git commit -m "feat: plan the source requests a send needs"
```

---

### Task 8: Store loads, records, and saves the cache

**Files:**
- Modify: `crates/blink-core/src/response_tokens.rs`, `crates/blink/src/store.rs`, `crates/blink/src/runner.rs`
- Test: `crates/blink/src/ui/app/ui_tests.rs` (or the store test module the existing send tests live in — grep `fn .*send` in `crates/blink/src/**/ui_tests.rs` and put it next to them)

**Interfaces:**
- Produces:
  - `response_tokens::values_for(request_id: u64, workspace_tokens: impl Iterator<Item = &ResponseToken>, response: &ApiResponse) -> Vec<(ValueKey, String)>` — reads each distinct `(source, path)` of tokens with `request_id` using `checks::read_source`; skips `Ok(None)`/`Err`.
  - `Workspace::all_response_tokens(&self) -> impl Iterator<Item = &ResponseToken>` (groups then global).
  - `Store::save_response_cache(&self)` — writes on the engine runtime via `self.engine` (blocking file write is small; call `save_response_tokens` directly, ignore the error except to log it via `eprintln!` as other non-critical saves do — check how `save_window_state` errors are handled and match it).

- [ ] **Step 1: Write the failing UI test**

```rust
#[gpui_kit::test]
fn a_2xx_send_of_a_source_request_records_its_token_values(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, _requests) = serve(|_| Reply::ok("application/json", r#"{"access_token":"abc"}"#));
    let login = harness.active_id(cx);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.set_global_response_tokens(vec![blink_core::model::ResponseToken {
                id: 1,
                name: "access_token".into(),
                request_id: login,
                source: blink_core::model::CheckSource::Json,
                path: ".access_token".into(),
                max_age_secs: None,
            }]);
        })
    });
    harness.send(cx, &format!("{url}/login"));
    let value = cx.read(|cx| {
        let workspace = &harness.store.read(cx).workspace;
        workspace
            .token_sources(blink_core::history::now_ms())
            .context(None)
            .workspace_definitions
            .get("access_token")
            .cloned()
    });
    assert_eq!(value.as_deref(), Some("abc"));
    let saved = engine.load_response_tokens();
    assert_eq!(saved.entries().len(), 1);
}
```

Copy the attribute and imports from the existing UI test files (they may use `#[gpui::test]` re-exported as `gpui_kit::test`; read one before writing).

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -p blink a_2xx_send_of_a_source_request_records_its_token_values`
Expected: FAIL — value is `None`.

- [ ] **Step 3: Implement**
  - `Store::restore` success branch, after `this.workspace.restore(...)` succeeds: `this.workspace.response_cache = this.engine.load_response_tokens(); let live = this.workspace.sessions.iter().map(|s| s.id).collect(); this.workspace.response_cache.prune(&live);`
  - `Store::reset`: clear `response_cache` and save.
  - In `runner.rs::finish`, inside `update_workspace`, BEFORE `finish_send` consumes the result, keep `let fingerprint = session.sent_fingerprint.clone();`; after `finish_send`, if the session's `response` is `Some` with `200..=299` status:

```rust
            let values = values_for(session_id, workspace.all_response_tokens(), response);
            if workspace.all_response_tokens().any(|t| t.request_id == session_id) {
                workspace.response_cache.record(session_id, &fingerprint, now_ms() as u64, values);
                recorded = true;
            }
```

    then after `update_workspace` returns, `if recorded { self.save_response_cache(); }`.
  - Read `finish_send` to confirm where the response lands (`session.response`) and that `sent_fingerprint` is not reset by it.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink && cargo test -p blink-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat: record response token values after a 2xx send"
```

---

### Task 9: The dependency step before send and connect

**Files:**
- Modify: `crates/blink/src/runner.rs`, `crates/blink/src/store.rs`, `crates/blink/src/ui/response_panel.rs`
- Test: same UI test file as Task 8

**Interfaces:**
- Consumes: `response_token_plan`, `PlanStep`, `dependency_error`, `DependencyFailure`, `Workspace::environment_confirmed`, `request_environment`.
- Produces:
  - `Store.waiting: HashMap<u64, Waiting>` where `pub(crate) struct Waiting { dependency: u64, token: String, started: bool, then: PrimaryAction }`.
  - Behavior:
    1. `Store::send` → for `PrimaryAction::Send` and `PrimaryAction::Connect`, call `self.run_or_wait(session_id, action, cx)` instead of sending directly. `ConfirmEnvironment` and `Disconnect` are unchanged.
    2. `run_or_wait`: `plan = response_token_plan(session_id, &workspace.token_sources(now_ms()))`.
       - `Err(message)` → set `session.error = message`, clear `waiting_on`, notify. Do not send.
       - `Ok(empty)` → clear `waiting_on`; `send_http` or `connect` per `then`.
       - `Ok(steps)` → `step = steps[0]`. If the step's environment is protected and not confirmed (`request_environment(dep.group_id, groups)` with `protected == Some(true)` and `!workspace.environment_confirmed(dep.group_id)`), set `session.error = format!("Send \"{label}\" once to confirm {env}.")` and stop. Otherwise set `session.waiting_on = Some(label)`, `session.error.clear()`, insert `Waiting { dependency: step.request_id, token: step.token, started: !self.in_flight.contains_key(&dep), then }`, and if not already in flight call `self.send_http(step.request_id, cx)`.
    3. In `finish(session_id, ..)` after the workspace update, collect `waiters = self.waiting.iter().filter(|(_, w)| w.dependency == session_id)`, remove them, and for each:
       - Compute the failure from the dependency session: `error` non-empty → `Error(error)`; response status not 2xx → `Status(status)`; else re-plan — if the new plan's first step is the same `(request_id, token)` → `Missing { path }` (path from the token); cancelled (see 4) → `Cancelled`.
       - On failure: `session.error = dependency_error(token, label, failure)`; clear `waiting_on`.
       - Else `self.run_or_wait(waiter_id, then, cx)` (re-plans; chains continue).
    4. `Store::cancel(session_id)`: if `self.waiting.remove(&session_id)` is `Some(w)`: clear `waiting_on`, set `error` to `dependency_error(.., Cancelled)`; if `w.started` and no other waiter has `dependency == w.dependency`, `self.cancel(w.dependency, cx)`. Then return.
    5. A waiting dependent counts as busy for the Send button: find how `primary_action` and the response panel decide busy (`session.busy`), and treat `waiting_on.is_some()` the same way in the Send/Cancel button state (grep `busy` in `crates/blink/src/ui/request_pane*.rs`).
  - Response panel: when `session.waiting_on` is `Some(label)`, show `Sending "{label}"…` in the same place and style the panel uses for the running-request status (grep `elapsed` in `response_panel.rs`).

- [ ] **Step 1: Write failing UI tests**

```rust
/// A Login request at `/login` and a dependent at `/me` that sends
/// `Authorization: Bearer {{access_token}}`. Returns (login, me).
fn login_and_me(harness: &Harness, cx: &mut TestAppContext, url: &str) -> (u64, u64) {
    let login = harness.active_id(cx);
    let url = url.to_string();
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.session_mut(login).unwrap().draft.url = format!("{url}/login");
            workspace.set_global_response_tokens(vec![blink_core::model::ResponseToken {
                id: 1,
                name: "access_token".into(),
                request_id: login,
                source: blink_core::model::CheckSource::Json,
                path: ".access_token".into(),
                max_age_secs: None,
            }]);
            let mut me = blink_core::session::create_session(None);
            me.draft.url = format!("{url}/me");
            me.draft.local_auth = Some(blink_core::model::AuthorizationConfig::Bearer {
                token: "{{access_token}}".into(),
            });
            let id = me.id;
            workspace.sessions.push(me);
            workspace.open_request(id);
        })
    });
    cx.run_until_parked();
    (login, harness.active_id(cx))
}

fn paths(requests: &std::sync::mpsc::Receiver<Vec<u8>>) -> Vec<String> {
    requests
        .try_iter()
        .map(|raw| String::from_utf8_lossy(&raw).split(' ').nth(1).unwrap_or_default().to_string())
        .collect()
}

#[gpui_kit::test]
fn sends_the_source_request_first_and_uses_its_token(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|raw| {
        if raw.starts_with(b"POST /login") || raw.starts_with(b"GET /login") {
            Reply::ok("application/json", r#"{"access_token":"abc"}"#)
        } else {
            Reply::ok("text/plain", "me")
        }
    });
    let (_, me) = login_and_me(&harness, cx, &url);
    harness.send_draft(cx);
    let seen = paths(&requests);
    assert_eq!(seen, vec!["/login", "/me"]);
    // The second send reuses the cached token.
    harness.send_draft(cx);
    assert_eq!(paths(&requests), vec!["/me"]);
    let raw = harness.session(cx, |s| s.response.as_ref().map(|r| r.status));
    assert_eq!(raw, Some(200));
    let _ = me;
}

#[gpui_kit::test]
fn a_failed_source_request_stops_the_dependent(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|raw| {
        if raw.windows(6).any(|w| w == b"/login") {
            Reply { status: "401 Unauthorized", headers: vec![], body: b"no".to_vec() }
        } else {
            Reply::ok("text/plain", "me")
        }
    });
    let (_, me) = login_and_me(&harness, cx, &url);
    harness.dispatch(cx, crate::actions::SendRequest);
    wait(cx, "dependent failed", |cx| {
        let session = harness.store.read(cx).workspace.session(me).unwrap().clone();
        session.waiting_on.is_none() && !session.error.is_empty()
    });
    assert_eq!(paths(&requests), vec!["/login"]);
    assert_eq!(
        harness.session(cx, |s| s.error.clone()),
        "Could not get \"access_token\": \"/login\" returned 401."
    );
}

#[gpui_kit::test]
fn a_missing_path_stops_the_dependent_without_a_loop(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|_| Reply::ok("application/json", r#"{"other":1}"#));
    let (_, me) = login_and_me(&harness, cx, &url);
    harness.dispatch(cx, crate::actions::SendRequest);
    wait(cx, "dependent failed", |cx| {
        let session = harness.store.read(cx).workspace.session(me).unwrap().clone();
        session.waiting_on.is_none() && !session.error.is_empty()
    });
    assert_eq!(paths(&requests), vec!["/login"]);
    assert!(harness.session(cx, |s| s.error.contains("has no value at .access_token")));
}

#[gpui_kit::test]
fn two_dependents_share_one_source_send(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    init(cx, &engine);
    let harness = open(cx, &engine);
    let (url, requests) = serve(|raw| {
        if raw.windows(6).any(|w| w == b"/login") {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Reply::ok("application/json", r#"{"access_token":"abc"}"#)
        } else {
            Reply::ok("text/plain", "ok")
        }
    });
    let (_, me) = login_and_me(&harness, cx, &url);
    let other = harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            let mut copy = workspace.session(me).unwrap().clone();
            copy.id = blink_core::ids::SESSIONS.next();
            let id = copy.id;
            workspace.sessions.push(copy);
            id
        })
    });
    harness.update(cx, |window, cx| {
        harness.store.update(cx, |store, cx| {
            store.send(me, window, cx);
            store.send(other, window, cx);
        })
    });
    wait(cx, "both sent", |cx| {
        let ws = &harness.store.read(cx).workspace;
        [me, other].iter().all(|id| ws.session(*id).unwrap().response.is_some())
    });
    let seen = paths(&requests);
    assert_eq!(seen.iter().filter(|p| p.as_str() == "/login").count(), 1, "{seen:?}");
}
```

The label in the 401 test is `session_label` of the Login request: for `http://127.0.0.1:PORT/login` it is the path, `/login`.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink -- sends_the_source_request_first a_failed_source a_missing_path two_dependents`
Expected: FAIL — `/login` never sent.

- [ ] **Step 3: Implement** per the behavior list.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink && cargo test -p blink-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat: send source requests before a request that needs their tokens"
```

---

### Task 10: Auth fields become token fields with a secret mode

**Files:**
- Modify: `crates/blink/src/ui/token_input.rs`, `crates/blink/src/ui/request_pane/editor.rs` (around lines 340–410 and 1391), `crates/blink/src/ui/group_settings.rs` (around lines 194–340, 407, 590)
- Test: `crates/blink/src/ui/request_pane/editor/ui_tests.rs`

**Interfaces:**
- Produces:
  - `TokenInput::set_secret(&mut self, secret: bool, cx: &mut Context<Self>)`
  - `pub fn secret_display(raw: &str, ctx: Option<&InterpolationContext>) -> Vec<(String, Option<TokenState>)>` in `token_input.rs` — plain runs become `"•"` repeated per char with `None`; references keep raw text with their state.
  - In secret mode, `token_ranges` labels every reference with its raw text (never the value), for all states.

Behavior:
- Not focused and secret: render a row (same height, padding, font, border as the input; reuse `Input`'s appearance by rendering the input with `opacity(0.)` absolutely behind it, or — simpler — render the display row in place of the input and focus the input on mouse down) showing `secret_display` segments: bullets in `colors.foreground`, references in their state color (same colors as `render_token`). Clicking it focuses the input.
- Focused and secret: the normal input, references labeled raw.
- Empty secret value: show the placeholder, as the input does.

- [ ] **Step 1: Write failing tests**

Pure test in `token_input.rs` tests:

```rust
#[test]
fn secret_display_masks_text_and_shows_references() {
    let segments = secret_display("ab{{host}}c{{nope}}", Some(&ctx()));
    let text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(text, "••{{host}}•{{nope}}");
    assert_eq!(segments[1].1, Some(TokenState::Resolved));
    assert_eq!(segments[3].1, Some(TokenState::Unresolved));
}
```

(`ctx()` in that module defines `host`; check and adjust names.)

UI test in `editor/ui_tests.rs` (copy the setup of an existing auth test there):

```rust
#[gpui_kit::test]
fn the_bearer_field_shows_an_undefined_reference(cx: &mut TestAppContext) {
    // setup: open harness, set draft.local_auth = Bearer { token: "{{acess_token}}" },
    // select the Auth tab (copy how existing tests switch request tabs), draw.
    // Assert: the rendered window text contains "{{acess_token}}" and no "•".
    // Then set token to "abc{{acess_token}}" and assert it renders "•••{{acess_token}}".
}
```

Write the body using the helpers the existing editor UI tests use to read rendered text (grep for a helper such as `rendered_text` or `window_text` in `crates/blink/src/ui/**/ui_tests.rs`). If no such helper exists, assert on `TokenInput` state instead: read the bearer `TokenInput` entity from the editor and check `secret_display(&value, ctx)`.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink secret_display the_bearer_field`
Expected: FAIL.

- [ ] **Step 3: Implement**
  - `TokenInput`: field `secret: bool`; `set_secret` sets it, calls `retokenize_all`, notifies. `token_ranges(raw, ctx)` gets a `raw_labels: bool` parameter (true in secret mode) that forces `label = text.clone()`.
  - In `render`, when `self.secret && !self.focused`, return the display row instead of the `Input` child (keep the same outer `div` and actions).
  - `editor.rs`: replace the masked `InputState` for the bearer token with `cx.new(|cx| { let mut input = TokenInput::new("Bearer token", window, cx); input.set_secret(true, cx); input })`; the basic username becomes a `TokenInput` (not secret), the password a secret `TokenInput`. Subscribe to `TokenInputEvent::Change` where the old code subscribed to `InputEvent::Change`, and call `set_context` wherever the URL field's `TokenInput` gets its context (grep `set_context(` in `editor.rs`).
  - `group_settings.rs`: same change for its bearer/basic fields; the context is the group's own context — build it with `store.workspace.token_sources(now_ms()).context(Some(group_id))`.
  - Remove the now-unused masked `InputState` code. Keep the "Credentials are saved locally in plaintext. Copy cURL includes them." note.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/blink
git commit -m "feat: show token references in auth fields and mask the rest"
```

---

### Task 11: Response tokens editor

**Files:**
- Create: `crates/blink/src/ui/response_tokens_editor.rs`
- Modify: `crates/blink/src/ui/mod.rs`, `crates/blink/src/ui/group_settings.rs`, `crates/blink/src/ui/settings_window.rs`
- Test: `crates/blink/src/ui/group_settings/ui_tests.rs` if it exists, else a `#[cfg(test)] mod ui_tests` beside the editor, following `crates/blink/src/ui/settings_window/ui_tests.rs`.

**Interfaces:**
- Consumes: `RESPONSE_TOKEN_SOURCES`, `parse_max_age`, `format_max_age`, `validate_response_tokens`, `ids::RESPONSE_TOKENS`, `session_label`.
- Produces:
  - `ResponseTokensEditor::new(tokens: Vec<ResponseToken>, requests: Vec<(u64, String)>, window, cx) -> Self` — `requests` are `(id, "Group / Sub / label")` choices.
  - `ResponseTokensEditor::tokens(&self, cx: &App) -> Result<Vec<ResponseToken>, String>` — parses max ages; returns the first max-age error.
  - `ResponseTokensEditor::show_errors(&mut self, errors: Vec<(u64, String)>, cx)` — inline per row.
  - `pub fn request_choices(workspace: &Workspace) -> Vec<(u64, String)>` — each session with its group path, sorted as the Browser shows them (reuse the Browser's ordering helper if there is one; otherwise workspace order).

UI (copy exact strings):
- Section title: `Response tokens`
- Help text under it: `Read a value from another request's last 2xx response. Blink sends that request first when the value is missing, too old, or from other settings.`
- Columns: `Name`, `Request`, `Source`, `Path`, `Max age`
- Path placeholder: `.access_token` for JSON, `Header name` for Header, disabled for Body text and Status.
- Max age placeholder: `None`
- Add button: `Add response token`
- Request picker placeholder: `Choose a request`
- Build rows from the same widgets the captures editor uses (grep the Tests tab captures editor in `crates/blink/src/ui/request_pane/` for its source select and path input; reuse `choice_select` from `common.rs` for Source and Request).

Wiring:
- Group settings: below the token table. On save, run `validate_response_tokens(tokens, &text_names_of_this_group, &workspace.sessions)`; on errors call `show_errors` and do not save; otherwise pass `response_tokens: Some((!tokens.is_empty()).then_some(tokens))` in `GroupSettingsChanges`.
- Application Settings: below the global token table, same validation with the global text token names; save with `workspace.set_global_response_tokens(tokens)` inside the existing `update_workspace` call.

- [ ] **Step 1: Write failing tests**

```rust
#[gpui_kit::test]
fn saving_group_settings_keeps_response_tokens(cx: &mut TestAppContext) {
    // Open the harness; add a group via store.update_workspace(|w| w.add_group("API", None)).
    // Open group settings the way existing group settings tests do.
    // Add a row: name "access_token", request = active request, source JSON, path ".access_token", max age "15m".
    // Save. Assert workspace.groups[0].response_tokens == Some(vec![ResponseToken { name: "access_token", max_age_secs: Some(900), .. }]).
}

#[gpui_kit::test]
fn a_duplicate_name_blocks_the_save(cx: &mut TestAppContext) {
    // Same setup; group has text token "access_token"; add a response token "access_token".
    // Save. Assert the dialog stays open and shows "Another token is named \"access_token\"."
}
```

Write these with the exact helpers the existing group settings tests use (read `crates/blink/src/ui/group_settings.rs` tests near line 681 and any `ui_tests.rs` for the dialog first).

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p blink saving_group_settings_keeps_response_tokens a_duplicate_name_blocks_the_save`
Expected: FAIL.

- [ ] **Step 3: Implement** the editor and wiring.

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/blink
git commit -m "feat: edit response tokens in group and application settings"
```

---

### Task 12: Badge on source request tabs

**Files:**
- Modify: `crates/blink/src/ui/tabs.rs`
- Test: `crates/blink/src/ui/tabs.rs` tests or the app UI tests

**Interfaces:**
- Produces: `pub fn response_token_readers(workspace: &Workspace, request_id: u64) -> usize` in `response_tokens.rs` (core) — the count of tokens in `all_response_tokens()` with that `request_id`.

UI: when the count is > 0, the tab shows the count after the label, in `text_xs`, `colors.muted_foreground`, with tooltip `Read by N response token(s)` → exact strings: `Read by 1 response token` / `Read by {n} response tokens`.

- [ ] **Step 1: Write the failing core test**

```rust
#[test]
fn counts_the_tokens_that_read_a_request() {
    let (mut workspace, login, me) = workspace_with_login();
    workspace.set_global_response_tokens(vec![token(9, "csrf", login)]);
    assert_eq!(response_token_readers(&workspace, login), 2);
    assert_eq!(response_token_readers(&workspace, me), 0);
}
```

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test -p blink-core counts_the_tokens_that_read_a_request`
Expected: compile error.

- [ ] **Step 3: Implement** the function and the tab badge (find where the tab label renders in `tabs.rs`; add the badge as a sibling after the label).

- [ ] **Step 4: Run tests**

Run: `cargo test -p blink-core && cargo test -p blink`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat: show how many response tokens read a request"
```

---

### Task 13: Spec deltas, full checks, manual run

**Files:**
- Modify: `docs/superpowers/specs/2026-10-01-response-tokens-design.md`, `README.md` (only if it lists token features — check first)

- [ ] **Step 1: Record the three spec deltas** from the top of this plan in the spec (Cache: no status, prune on load; UI/Auth fields: secret mode focus behavior).

- [ ] **Step 2: Run every check**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: no diffs from fmt, no warnings, all tests pass.

- [ ] **Step 3: Manual check on the dev build** (`cargo run -p blink`):
  1. Make a Login request to a real login API and a response token `access_token` (JSON, `.access_token`, max age `1m`) on its root group.
  2. Set another request's bearer token to `{{access_token}}`. Confirm the field shows `{{access_token}}` in the response color and the hover says `No current value · sends "…" first`.
  3. Send it. Confirm Login sends first, then the request with the token.
  4. Switch the environment. Send again. Confirm Login sends again. Switch back. Send. Confirm Login does NOT send.
  5. Wait over 1 minute. Confirm the hover changes, and the next send refreshes.
  6. Type `{{acess_token}}`. Confirm it shows in the warning style.

- [ ] **Step 4: Commit**

```bash
git add docs README.md
git commit -m "docs: record response token design changes"
```
