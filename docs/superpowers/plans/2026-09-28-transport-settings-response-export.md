# Transport Settings and Response Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make timeout, redirect, and inspection-limit values application settings, and let the user save any response body (text, binary, or larger than the inspection limit) to a file.

**Architecture:** Five new integer/boolean fields in `WorkspacePreferences` become a `TransportOptions` object that goes to both transports. The desktop transport streams every body to a temp file owned by a Rust `ResponseStore` and returns a bounded preview plus a `bodyId`. The browser transport keeps the body as a `Blob`. A new `response-body.ts` module saves or releases a body by id. The response panel shows binary and truncated states and a Save button.

**Tech Stack:** Vue 3 + TypeScript, Vitest (jsdom), Playwright, Tauri 2, Rust with reqwest 0.12, `tempfile`, `tauri-plugin-dialog`.

**Spec:** `docs/superpowers/specs/2026-09-28-transport-settings-response-export-design.md`

## Global Constraints

- Defaults: `timeoutSeconds` 30, `connectTimeoutSeconds` 10, `followRedirects` false, `maxRedirects` 10, `inspectionLimitMiB` 4.
- Ranges: `timeoutSeconds` 1–600; `connectTimeoutSeconds` 1–`timeoutSeconds`; `maxRedirects` 1–20; `inspectionLimitMiB` 1–16. All integers.
- Download cap: fixed 1 GiB (`1024 * 1024 * 1024` bytes). Not a setting.
- Workspace snapshot version stays 4. Missing preference fields fill from defaults. New response fields are optional.
- Rust re-validates options and returns `"Invalid transport settings."`.
- The webview never supplies a file path. Rust opens the save dialog. The JS capability gets no dialog permission.
- Error text, verbatim:
  - `"Response exceeds the 1 GiB download limit."`
  - `"Cannot store the response. Check free disk space."`
  - `"Stopped after N redirects."`
  - `"Invalid transport settings."`
  - `"Cannot save the file: {reason}."`
  - `"Schema response exceeds the inspection limit."`
  - `"Request timed out ({connect} s connection / {total} s total limit)."` (desktop)
  - `"Request timed out after {total} seconds."` (browser)
- UI text, verbatim: `"Save response body…"`, `"Body is no longer available. Send the request again."`, `"Binary response · {size} · {content type}"`, `"Preview shows the first {size} of {size}."`, `"Unavailable for truncated responses"`.
- Colors come from existing theme tokens only. The truncated bar uses muted foreground and border tokens, not the accent.
- Seven test files mock `@/lib/transport` with only `sendRequest` (and `nativeTransport`). Do not add new runtime exports to `transport.ts` that other modules import. New helpers go in `transport-options.ts` and `response-body.ts`.
- Run checks with: `bun run test`, `bun run lint`, `bun run build`, `cd src-tauri && cargo test`, `bun run test:e2e`.

## Review Focus

1. **Old workspace with no new preference fields** — it must load, and the settings show defaults. Test in Task 1.
2. **Multi-byte UTF-8 character cut at the preview boundary** — it must stay text, not become binary. Test in Task 4 (Rust) and Task 6 (browser).
3. **Send again while a body is stored** — the old temp file must be released, and a result that arrives after unmount must also be released. Test in Task 7.
4. **Hostile `Content-Disposition` filename (`../../etc/passwd`, control characters)** — the suggested name must have no path separators. Test in Task 6.
5. **Settings value typed as empty or decimal** — the dialog must block the save with an inline error, not send `NaN` to Rust. Test in Task 2.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/lib/transport-options.ts` | Create | `TransportOptions` type, defaults, ranges, field errors, byte constants |
| `src/lib/preferences.ts` | Modify | New fields, `normalizePreferences`, `transportOptions()` |
| `src/lib/workspace.ts` | Modify | Load via `normalizePreferences`; optional response fields; strip `bodyId` |
| `src/components/ApplicationSettingsDialog.vue` | Modify | "Requests" section |
| `src-tauri/src/lib.rs` | Modify | `TransportOptions`, client builder, redirect policy, streaming, preview decode |
| `src-tauri/src/response_store.rs` | Create | Temp-file store and save/release commands |
| `src-tauri/Cargo.toml` | Modify | Add `tauri-plugin-dialog` |
| `src-tauri/src/request_tests.rs` | Modify | Tests for the new transport behavior |
| `src/lib/request.ts` | Modify | Optional `ApiResponse` fields |
| `src/lib/transport.ts` | Modify | Options, browser redirects, preview, Blob store |
| `src/lib/response-body.ts` | Create | `storeBlob`, `releaseResponse`, `canSaveResponse`, `saveResponse`, `suggestedFileName` |
| `src/composables/useRequestRunner.ts` | Modify | Pass options, release old bodies |
| `src/composables/useWorkspaceState.ts` | Modify | Release body on request delete |
| `src/lib/graphql-schema.ts` | Modify | Options, truncation error, release |
| `src/components/RequestWorkspace.vue`, `RequestEditor.vue`, `src/App.vue` | Modify | Wire `transport` prop and status-bar text |
| `src/components/ResponsePanel.vue` | Modify | Save button, binary notice, truncated bar, redirect line |
| `e2e/response-export.spec.ts` | Create | Truncated preview and download |
| `README.md` | Modify | Request limits paragraph |

---

### Task 1: Transport preferences and workspace compatibility

**Files:**
- Create: `src/lib/transport-options.ts`
- Modify: `src/lib/preferences.ts`
- Modify: `src/lib/workspace.ts:280-281` (parse), `src/lib/workspace.ts:401-404` (decode)
- Test: `src/lib/__test__/transport-options.test.ts` (create), `src/lib/__test__/preferences.test.ts`

**Interfaces:**
- Produces:
  - `type TransportOptions = { timeoutSeconds: number; connectTimeoutSeconds: number; followRedirects: boolean; maxRedirects: number; inspectionLimitMiB: number }`
  - `type TransportField = "timeoutSeconds" | "connectTimeoutSeconds" | "maxRedirects" | "inspectionLimitMiB"`
  - `const MIB = 1024 * 1024`, `const DOWNLOAD_LIMIT = 1024 * MIB`
  - `defaultTransportOptions(): TransportOptions`
  - `transportRanges: Record<TransportField, readonly [number, number]>`
  - `transportFieldErrors(options: TransportOptions): Partial<Record<TransportField, string>>`
  - `WorkspacePreferences` now includes all `TransportOptions` fields.
  - `normalizePreferences(value: unknown): WorkspacePreferences | null`
  - `transportOptions(preferences: WorkspacePreferences): TransportOptions`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__test__/transport-options.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  defaultTransportOptions,
  transportFieldErrors,
} from "../transport-options";

describe("transport options", () => {
  it("accepts the defaults", () => {
    expect(defaultTransportOptions()).toEqual({
      timeoutSeconds: 30,
      connectTimeoutSeconds: 10,
      followRedirects: false,
      maxRedirects: 10,
      inspectionLimitMiB: 4,
    });
    expect(transportFieldErrors(defaultTransportOptions())).toEqual({});
  });
  it("rejects values out of range, decimals and NaN", () => {
    const errors = transportFieldErrors({
      timeoutSeconds: 601,
      connectTimeoutSeconds: 1.5,
      followRedirects: true,
      maxRedirects: 0,
      inspectionLimitMiB: Number.NaN,
    });
    expect(errors).toEqual({
      timeoutSeconds: "Enter a whole number from 1 to 600.",
      connectTimeoutSeconds: "Enter a whole number from 1 to 600.",
      maxRedirects: "Enter a whole number from 1 to 20.",
      inspectionLimitMiB: "Enter a whole number from 1 to 16.",
    });
  });
  it("limits the connect timeout to the total timeout", () => {
    expect(
      transportFieldErrors({
        ...defaultTransportOptions(),
        timeoutSeconds: 5,
        connectTimeoutSeconds: 6,
      }),
    ).toEqual({ connectTimeoutSeconds: "Enter a whole number from 1 to 5." });
  });
});
```

Append to `src/lib/__test__/preferences.test.ts` (add `normalizePreferences` and `transportOptions` to the existing `../preferences` import):

```ts
describe("transport preferences", () => {
  it("fills missing transport fields from defaults", () => {
    expect(
      normalizePreferences({
        defaultMethod: "POST",
        defaultBodyMode: "json",
        pretty: false,
        wrap: true,
        confirmCloseDrafts: false,
      }),
    ).toEqual({
      ...defaultPreferences(),
      defaultMethod: "POST",
      defaultBodyMode: "json",
      pretty: false,
      wrap: true,
      confirmCloseDrafts: false,
    });
  });
  it("rejects wrong types and out-of-range values", () => {
    expect(normalizePreferences(null)).toBeNull();
    expect(normalizePreferences([])).toBeNull();
    expect(
      normalizePreferences({ ...defaultPreferences(), timeoutSeconds: "30" }),
    ).toBeNull();
    expect(
      normalizePreferences({ ...defaultPreferences(), maxRedirects: 21 }),
    ).toBeNull();
    expect(
      normalizePreferences({ ...defaultPreferences(), followRedirects: 1 }),
    ).toBeNull();
  });
  it("drops unknown keys", () => {
    expect(
      normalizePreferences({ ...defaultPreferences(), extra: true }),
    ).toEqual(defaultPreferences());
  });
  it("strict validation needs every field", () => {
    const partial: Record<string, unknown> = { ...defaultPreferences() };
    delete partial.timeoutSeconds;
    expect(validPreferences(partial)).toBe(false);
    expect(validPreferences(defaultPreferences())).toBe(true);
  });
  it("extracts transport options", () => {
    expect(transportOptions(defaultPreferences())).toEqual({
      timeoutSeconds: 30,
      connectTimeoutSeconds: 10,
      followRedirects: false,
      maxRedirects: 10,
      inspectionLimitMiB: 4,
    });
  });
  it("loads a v4 workspace saved before the transport fields", () => {
    const session = createSession(createDraft());
    const encoded = JSON.parse(
      encodeWorkspace([session], session.id, [], {}, defaultPreferences()),
    );
    for (const key of [
      "timeoutSeconds",
      "connectTimeoutSeconds",
      "followRedirects",
      "maxRedirects",
      "inspectionLimitMiB",
    ])
      delete encoded.preferences[key];
    expect(decodeWorkspace(JSON.stringify(encoded)).preferences).toEqual(
      defaultPreferences(),
    );
  });
});
```

If `createSession` in this file takes different arguments, copy the call used by the existing tests at the top of `preferences.test.ts`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun run test src/lib/__test__/transport-options.test.ts src/lib/__test__/preferences.test.ts`
Expected: FAIL. `../transport-options` does not exist, and `normalizePreferences` is not exported.

- [ ] **Step 3: Create `src/lib/transport-options.ts`**

```ts
export const MIB = 1024 * 1024;
export const DOWNLOAD_LIMIT = 1024 * MIB;

export type TransportOptions = {
  timeoutSeconds: number;
  connectTimeoutSeconds: number;
  followRedirects: boolean;
  maxRedirects: number;
  inspectionLimitMiB: number;
};

export type TransportField = Exclude<keyof TransportOptions, "followRedirects">;

export const defaultTransportOptions = (): TransportOptions => ({
  timeoutSeconds: 30,
  connectTimeoutSeconds: 10,
  followRedirects: false,
  maxRedirects: 10,
  inspectionLimitMiB: 4,
});

export const transportRanges: Record<TransportField, readonly [number, number]> =
  {
    timeoutSeconds: [1, 600],
    connectTimeoutSeconds: [1, 600],
    maxRedirects: [1, 20],
    inspectionLimitMiB: [1, 16],
  };

/** One message per invalid field. An empty object means the options are valid. */
export function transportFieldErrors(options: TransportOptions) {
  const errors: Partial<Record<TransportField, string>> = {};
  for (const field of Object.keys(transportRanges) as TransportField[]) {
    const [min, rangeMax] = transportRanges[field];
    // The connect timeout cannot be longer than the total timeout.
    const max =
      field === "connectTimeoutSeconds" &&
      Number.isInteger(options.timeoutSeconds)
        ? Math.min(rangeMax, options.timeoutSeconds)
        : rangeMax;
    const value = options[field];
    if (!Number.isInteger(value) || value < min || value > max)
      errors[field] = `Enter a whole number from ${min} to ${max}.`;
  }
  return errors;
}
```

- [ ] **Step 4: Update `src/lib/preferences.ts`**

Replace the type, defaults, and `validPreferences` with:

```ts
import { bodyModes, methods, type BodyMode, type Method } from "./request";
import type { RequestSession } from "./session";
import type { RequestGroup } from "./groups";
import {
  defaultTransportOptions,
  transportFieldErrors,
  type TransportOptions,
} from "./transport-options";

export type WorkspacePreferences = {
  defaultMethod: Method;
  defaultBodyMode: BodyMode;
  pretty: boolean;
  wrap: boolean;
  confirmCloseDrafts: boolean;
} & TransportOptions;

export const defaultPreferences = (): WorkspacePreferences => ({
  defaultMethod: "GET",
  defaultBodyMode: "none",
  pretty: true,
  wrap: false,
  confirmCloseDrafts: true,
  ...defaultTransportOptions(),
});

export function validPreferences(
  value: unknown,
): value is WorkspacePreferences {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const p = value as Record<string, unknown>;
  return (
    methods.includes(p.defaultMethod as Method) &&
    bodyModes.includes(p.defaultBodyMode as BodyMode) &&
    typeof p.pretty === "boolean" &&
    typeof p.wrap === "boolean" &&
    typeof p.confirmCloseDrafts === "boolean" &&
    typeof p.followRedirects === "boolean" &&
    Object.keys(transportFieldErrors(p as TransportOptions)).length === 0
  );
}

/**
 * Fill fields added after a workspace was saved with their defaults. Returns
 * null for a wrong type or an out-of-range value. Unknown keys are dropped.
 */
export function normalizePreferences(
  value: unknown,
): WorkspacePreferences | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const merged: Record<string, unknown> = { ...defaultPreferences() };
  for (const key of Object.keys(merged))
    if (key in value) merged[key] = (value as Record<string, unknown>)[key];
  return validPreferences(merged) ? merged : null;
}

export const transportOptions = (
  preferences: WorkspacePreferences,
): TransportOptions => ({
  timeoutSeconds: preferences.timeoutSeconds,
  connectTimeoutSeconds: preferences.connectTimeoutSeconds,
  followRedirects: preferences.followRedirects,
  maxRedirects: preferences.maxRedirects,
  inspectionLimitMiB: preferences.inspectionLimitMiB,
});
```

Keep `resolveNewRequestDefaults` and `applyNewRequestDefaults` unchanged.

- [ ] **Step 5: Use `normalizePreferences` in `src/lib/workspace.ts`**

Change the import from `./preferences` to include `normalizePreferences`. In `parseSnapshot` replace:

```ts
    if (data.preferences !== undefined)
      check(validPreferences(data.preferences));
```

with:

```ts
    if (data.preferences !== undefined)
      check(normalizePreferences(data.preferences) !== null);
```

In `decodeWorkspace` replace the `const preferences = ...` block with:

```ts
  const saved =
    version >= 3 ? (data as SnapshotV3 | SnapshotV4).preferences : undefined;
  const preferences =
    saved === undefined
      ? defaultPreferences()
      : (normalizePreferences(saved) ?? defaultPreferences());
```

Remove the `validPreferences` import from `workspace.ts` if it is now unused.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `bun run test src/lib/__test__/`
Expected: PASS for all files, including the existing workspace tests.

- [ ] **Step 7: Type-check and commit**

Run: `bun run build`
Expected: no type errors. If a test fixture builds a full `WorkspacePreferences` literal and fails the type check, spread `...defaultPreferences()` into it.

```bash
git add src/lib/transport-options.ts src/lib/preferences.ts src/lib/workspace.ts src/lib/__test__/transport-options.test.ts src/lib/__test__/preferences.test.ts
git commit -m "feat: add transport preferences"
```

---

### Task 2: Requests section in Application Settings

**Files:**
- Modify: `src/components/ApplicationSettingsDialog.vue`
- Test: `src/components/__test__/ApplicationSettingsDialog.test.ts`

**Interfaces:**
- Consumes: `transportFieldErrors`, `TransportField` from `@/lib/transport-options`; `WorkspacePreferences` with transport fields (Task 1).
- Produces: the dialog emits `save` with transport fields included. Inputs have ids `app-timeout`, `app-connect-timeout`, `app-follow-redirects`, `app-max-redirects`, `app-inspection-limit`.

- [ ] **Step 1: Write the failing tests**

Append to `src/components/__test__/ApplicationSettingsDialog.test.ts`:

```ts
describe("request settings", () => {
  it("saves transport settings", async () => {
    const wrapper = renderOpen();
    await nextTick();
    await wrapper.get("#app-timeout").setValue("90");
    await wrapper.get("#app-connect-timeout").setValue("5");
    await wrapper.get("#app-follow-redirects").setValue(true);
    await wrapper.get("#app-max-redirects").setValue("3");
    await wrapper.get("#app-inspection-limit").setValue("8");
    await wrapper.get("form").trigger("submit");
    const [, preferences] = wrapper.emitted("save")![0] as [unknown, object];
    expect(preferences).toMatchObject({
      timeoutSeconds: 90,
      connectTimeoutSeconds: 5,
      followRedirects: true,
      maxRedirects: 3,
      inspectionLimitMiB: 8,
    });
    wrapper.unmount();
  });

  it("disables max redirects while redirects are not followed", async () => {
    const wrapper = renderOpen();
    await nextTick();
    expect(
      (wrapper.get("#app-max-redirects").element as HTMLInputElement).disabled,
    ).toBe(true);
    await wrapper.get("#app-follow-redirects").setValue(true);
    expect(
      (wrapper.get("#app-max-redirects").element as HTMLInputElement).disabled,
    ).toBe(false);
    wrapper.unmount();
  });

  it.each(["", "2.5", "0", "601"])(
    "blocks the save for timeout %j",
    async (value) => {
      const wrapper = renderOpen();
      await nextTick();
      await wrapper.get("#app-timeout").setValue(value);
      await wrapper.get("form").trigger("submit");
      expect(wrapper.emitted("save")).toBeUndefined();
      expect(wrapper.get("#app-timeout-error").text()).toBe(
        "Enter a whole number from 1 to 600.",
      );
      expect(wrapper.get("#app-timeout").attributes("aria-invalid")).toBe(
        "true",
      );
      wrapper.unmount();
    },
  );
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun run test src/components/__test__/ApplicationSettingsDialog.test.ts`
Expected: FAIL. `#app-timeout` is not found.

- [ ] **Step 3: Add the script logic**

In `ApplicationSettingsDialog.vue` `<script setup>`, change the `vue` import to `import { computed, ref, watch } from "vue";`, add the import below, and add the code after `const preferences = ref(defaultPreferences());`:

```ts
import {
  transportFieldErrors,
  type TransportField,
} from "../lib/transport-options";
```

```ts
const numberFields: {
  key: TransportField;
  id: string;
  label: string;
  help?: string;
}[] = [
  { key: "timeoutSeconds", id: "app-timeout", label: "Timeout (s)" },
  {
    key: "connectTimeoutSeconds",
    id: "app-connect-timeout",
    label: "Connect timeout (s)",
  },
  { key: "maxRedirects", id: "app-max-redirects", label: "Max redirects" },
  {
    key: "inspectionLimitMiB",
    id: "app-inspection-limit",
    label: "Inspection limit (MiB)",
    help: "Larger bodies show a truncated preview. Save the response to get the full body.",
  },
];
// Errors show only after a save attempt, so typing does not flash errors.
const submitted = ref(false);
const fieldErrors = computed(() =>
  submitted.value ? transportFieldErrors(preferences.value) : {},
);
```

In the existing `watch(() => [props.open, props.definitions] ...)` callback, add `submitted.value = false;` next to `error.value = "";`.

Change `save()` to:

```ts
function save() {
  submitted.value = true;
  const definitions = parseDefinitions();
  if (
    !definitions ||
    theme.error.value ||
    Object.keys(transportFieldErrors(preferences.value)).length
  )
    return;
  emit("save", definitions, { ...preferences.value });
  if (theme.commit()) return;
  committed = true;
  emit("update:open", false);
}
```

- [ ] **Step 4: Add the template section**

Insert this between the closing `</fieldset>` of "New request defaults" and the `<section>` for "Workspace":

```vue
            <fieldset
              class="grid grid-cols-2 gap-3 border-t border-border pt-3 text-xs max-[440px]:grid-cols-1"
            >
              <div
                class="col-span-full flex items-center gap-1.5 text-muted-foreground"
              >
                <span class="font-semibold uppercase tracking-[0.07em]"
                  >Requests</span
                >
                <HelpTooltip
                  text="These limits apply to every request you send. The download limit is 1 GiB."
                >
                  <button
                    type="button"
                    aria-label="Request limits help"
                    class="help-trigger"
                  >
                    ?
                  </button>
                </HelpTooltip>
              </div>
              <label class="col-span-full flex items-center gap-2"
                ><input
                  id="app-follow-redirects"
                  v-model="preferences.followRedirects"
                  type="checkbox"
                  class="accent-primary"
                />
                Follow redirects</label
              >
              <div
                v-for="field in numberFields"
                :key="field.key"
                class="grid content-start gap-1.5 text-muted-foreground"
              >
                <label :for="field.id">{{ field.label }}</label>
                <input
                  :id="field.id"
                  v-model.number="preferences[field.key]"
                  type="number"
                  step="1"
                  inputmode="numeric"
                  :disabled="
                    field.key === 'maxRedirects' && !preferences.followRedirects
                  "
                  :aria-invalid="fieldErrors[field.key] ? 'true' : undefined"
                  :aria-describedby="
                    fieldErrors[field.key] ? `${field.id}-error` : undefined
                  "
                  class="h-8 w-full min-w-0 border border-input rounded-sm px-2 bg-background text-foreground font-mono disabled:opacity-50 aria-invalid:border-destructive"
                />
                <p
                  v-if="fieldErrors[field.key]"
                  :id="`${field.id}-error`"
                  class="text-destructive"
                >
                  {{ fieldErrors[field.key] }}
                </p>
                <p v-else-if="field.help" class="text-[0.6875rem]">
                  {{ field.help }}
                </p>
              </div>
            </fieldset>
```

`v-model.number` on an empty input gives `""`. `Number.isInteger("")` is false, so the empty-string case is an error.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bun run test src/components/__test__/ApplicationSettingsDialog.test.ts`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

Run: `bun run lint`
Expected: no errors. If Prettier reports formatting, run `bunx prettier --write src/components/ApplicationSettingsDialog.vue`.

```bash
git add src/components/ApplicationSettingsDialog.vue src/components/__test__/ApplicationSettingsDialog.test.ts
git commit -m "feat: add request settings to application settings"
```

---

### Task 3: Rust transport options and redirects

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/request_tests.rs`

**Interfaces:**
- Produces (Rust, used by Task 4):
  - `struct TransportOptions { timeout_seconds: u64, connect_timeout_seconds: u64, follow_redirects: bool, max_redirects: usize, inspection_limit_mib: usize }` with `Deserialize` (JSON names match the TS type; `inspection_limit_mib` is renamed to `inspectionLimitMiB`) and `Default`.
  - `impl TransportOptions { fn validate(&self) -> Result<(), String> }`
  - `fn build_client(options: &TransportOptions, redirects: Arc<AtomicUsize>) -> Result<Client, String>`
  - `fn network_error(error: reqwest::Error, options: &TransportOptions) -> String`
  - `ResponseOutput` gains `final_url: Option<String>` and `redirect_count: Option<usize>` (both skipped when `None`).
  - `async fn execute(request: RequestInput, options: TransportOptions) -> Result<ResponseOutput, String>` holds the body of the old command. Task 4 changes this signature.
  - The command is `send_request(request: RequestInput, options: TransportOptions)`.

- [ ] **Step 1: Add a sequence fixture and write the failing tests**

In `src-tauri/src/request_tests.rs`, add after `fn input(...)`:

```rust
/// Serve each response on its own connection, in order. `{base}` in a
/// response's headers is replaced with the server's base URL.
fn serve(responses: Vec<(&'static str, String, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fixture_base = base.clone();
    thread::spawn(move || {
        for (status, headers, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            while !String::from_utf8_lossy(&bytes).contains("\r\n\r\n") {
                let read = stream.read(&mut chunk).unwrap_or(0);
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..read]);
            }
            let headers = headers.replace("{base}", &fixture_base);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    base
}

fn options() -> TransportOptions {
    TransportOptions::default()
}

fn following(max_redirects: usize) -> TransportOptions {
    TransportOptions {
        follow_redirects: true,
        max_redirects,
        ..TransportOptions::default()
    }
}
```

Change every existing `send_request(input(url))` call in the file to `execute(input(url), options())`. Leave the existing `rejects_oversized_response` test as is for now. Task 4 replaces it.

Add these tests:

```rust
#[tokio::test]
async fn follows_redirects_and_reports_the_final_url() {
    let base = serve(vec![
        ("302 Found", "Location: {base}/b\r\n".into(), vec![]),
        ("301 Moved Permanently", "Location: {base}/c\r\n".into(), vec![]),
        ("200 OK", String::new(), b"done".to_vec()),
    ]);
    let result = execute(input(format!("{base}/a")), following(5))
        .await
        .unwrap();
    assert_eq!(result.status, 200);
    assert_eq!(result.body, "done");
    assert_eq!(result.final_url.as_deref(), Some(format!("{base}/c").as_str()));
    assert_eq!(result.redirect_count, Some(2));
}

#[tokio::test]
async fn stops_at_the_redirect_limit() {
    let base = serve(vec![
        ("302 Found", "Location: {base}/b\r\n".into(), vec![]),
        ("302 Found", "Location: {base}/c\r\n".into(), vec![]),
        ("200 OK", String::new(), b"done".to_vec()),
    ]);
    assert_eq!(
        execute(input(format!("{base}/a")), following(1))
            .await
            .unwrap_err(),
        "Stopped after 1 redirects."
    );
}

#[tokio::test]
async fn omits_redirect_fields_without_a_redirect() {
    let base = serve(vec![("200 OK", String::new(), b"ok".to_vec())]);
    let result = execute(input(base), following(5)).await.unwrap();
    assert_eq!(result.final_url, None);
    assert_eq!(result.redirect_count, None);
}

#[tokio::test]
async fn rejects_out_of_range_transport_options() {
    for invalid in [
        TransportOptions { timeout_seconds: 0, ..options() },
        TransportOptions { timeout_seconds: 601, ..options() },
        TransportOptions { connect_timeout_seconds: 31, ..options() },
        TransportOptions { max_redirects: 21, ..options() },
        TransportOptions { inspection_limit_mib: 17, ..options() },
    ] {
        assert_eq!(
            execute(input("http://127.0.0.1:9/".into()), invalid)
                .await
                .unwrap_err(),
            "Invalid transport settings."
        );
    }
}

#[test]
fn transport_options_use_the_frontend_field_names() {
    let parsed: TransportOptions = serde_json::from_str(
        r#"{"timeoutSeconds":5,"connectTimeoutSeconds":2,"followRedirects":true,"maxRedirects":3,"inspectionLimitMiB":8}"#,
    )
    .unwrap();
    assert_eq!(parsed.timeout_seconds, 5);
    assert_eq!(parsed.inspection_limit_mib, 8);
}

#[tokio::test]
async fn timeout_message_uses_the_configured_limits() {
    let (url, _received) = fixture("200 OK", "", "late".into(), Duration::from_secs(3));
    let error = execute(
        input(url),
        TransportOptions { timeout_seconds: 1, connect_timeout_seconds: 1, ..options() },
    )
    .await
    .unwrap_err();
    assert_eq!(error, "Request timed out (1 s connection / 1 s total limit).");
}
```

The redirect error text for one hop is "Stopped after 1 redirects." This is the exact format `"Stopped after N redirects."` from the spec.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test`
Expected: compile errors. `execute` and `TransportOptions` do not exist.

- [ ] **Step 3: Implement the options, the client, and the redirect policy**

In `src-tauri/src/lib.rs`:

Change the imports at the top:

```rust
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use reqwest::{header::HeaderName, header::HeaderValue, redirect::Policy, Client, Method};
use serde::{Deserialize, Serialize};
```

Add after `struct HeaderInput`:

```rust
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransportOptions {
    timeout_seconds: u64,
    connect_timeout_seconds: u64,
    follow_redirects: bool,
    max_redirects: usize,
    #[serde(rename = "inspectionLimitMiB")]
    inspection_limit_mib: usize,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            connect_timeout_seconds: 10,
            follow_redirects: false,
            max_redirects: 10,
            inspection_limit_mib: 4,
        }
    }
}

impl TransportOptions {
    /// The frontend checks the same ranges. Rust checks again because the
    /// webview is not the trust boundary for limits.
    fn validate(&self) -> Result<(), String> {
        let valid = (1..=600).contains(&self.timeout_seconds)
            && (1..=self.timeout_seconds).contains(&self.connect_timeout_seconds)
            && (1..=20).contains(&self.max_redirects)
            && (1..=16).contains(&self.inspection_limit_mib);
        if valid {
            Ok(())
        } else {
            Err("Invalid transport settings.".to_string())
        }
    }
}
```

Add two fields to `ResponseOutput`, after `size_bytes`:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    final_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_count: Option<usize>,
```

Add the client builder:

```rust
/// Build a client for one request. `redirects` receives the hop count, so the
/// client must not be shared between requests.
fn build_client(options: &TransportOptions, redirects: Arc<AtomicUsize>) -> Result<Client, String> {
    let policy = if options.follow_redirects {
        let max = options.max_redirects;
        Policy::custom(move |attempt| {
            // `previous` holds every URL already requested, so its length is
            // the number of this hop.
            let hops = attempt.previous().len();
            if hops > max {
                attempt.error(format!("Stopped after {max} redirects."))
            } else {
                redirects.store(hops, Ordering::Relaxed);
                attempt.follow()
            }
        })
    } else {
        Policy::none()
    };
    Client::builder()
        .timeout(Duration::from_secs(options.timeout_seconds))
        .connect_timeout(Duration::from_secs(options.connect_timeout_seconds))
        .redirect(policy)
        .build()
        .map_err(|error| error.without_url().to_string())
}
```

Rename `async fn send_request(mut request: RequestInput)` to `async fn execute(mut request: RequestInput, options: TransportOptions)` and remove its `#[tauri::command]` attribute. At the start of `execute`, add `options.validate()?;`. Replace the old `let client = Client::builder()...;` block with:

```rust
    let redirects = Arc::new(AtomicUsize::new(0));
    let client = build_client(&options, redirects.clone())?;
```

Replace `.map_err(network_error)` with `.map_err(|error| network_error(error, &options))` in both places. Right after the `let mut response = builder.send()...;` line, add:

```rust
    // The policy has run for every hop once `send` returns.
    let redirect_count = redirects.load(Ordering::Relaxed);
    let final_url = (redirect_count > 0).then(|| response.url().to_string());
```

Add to the `ResponseOutput` literal:

```rust
        final_url,
        redirect_count: (redirect_count > 0).then_some(redirect_count),
```

Add the command wrapper above `execute`:

```rust
#[tauri::command]
async fn send_request(
    request: RequestInput,
    options: TransportOptions,
) -> Result<ResponseOutput, String> {
    execute(request, options).await
}
```

Replace `network_error`:

```rust
fn network_error(error: reqwest::Error, options: &TransportOptions) -> String {
    if error.is_timeout() {
        format!(
            "Request timed out ({} s connection / {} s total limit).",
            options.connect_timeout_seconds, options.timeout_seconds
        )
    } else if error.is_redirect() {
        // The policy's own message, such as "Stopped after N redirects."
        std::error::Error::source(&error)
            .map(|source| source.to_string())
            .unwrap_or_else(|| format!("Network request failed: {}", error.without_url()))
    } else {
        format!("Network request failed: {}", error.without_url())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test`
Expected: PASS. The existing `returns_redirect_without_following_it` test also passes, because the defaults do not follow redirects.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/request_tests.rs
git commit -m "feat: configurable timeouts and redirects in native transport"
```

---

### Task 4: Rust response store, streaming, and preview

**Files:**
- Create: `src-tauri/src/response_store.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/request_tests.rs`, `src-tauri/src/response_store.rs` (unit tests)

**Interfaces:**
- Consumes: `TransportOptions`, `execute`, `network_error` (Task 3).
- Produces:
  - `pub struct ResponseStore` with `pub fn new(dir: PathBuf) -> Self`, `pub fn create(&self) -> Result<NamedTempFile, String>`, `pub fn insert(&self, file: NamedTempFile) -> String`, `pub fn release(&self, id: &str)`, `pub fn copy_body(&self, id: &str, dest: &Path) -> Result<(), String>`, `#[cfg(test)] pub fn path(&self, id: &str) -> Option<PathBuf>`.
  - `const DOWNLOAD_LIMIT: u64 = 1024 * 1024 * 1024;`
  - `fn decode_preview(preview: Vec<u8>) -> (String, bool)` returns `(text, binary)`.
  - `execute(request, options, store: &ResponseStore, download_limit: u64)`.
  - `ResponseOutput` gains `body_id: Option<String>`, `truncated: bool`, `binary: bool`.
  - The Tauri command `release_response(body_id: String)`.

- [ ] **Step 1: Write the store unit tests**

Create `src-tauri/src/response_store.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn new_clears_files_left_by_an_earlier_run() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("responses");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old"), b"stale").unwrap();
        let _store = ResponseStore::new(dir.clone());
        assert!(!dir.join("old").exists());
        assert!(dir.is_dir());
    }

    #[test]
    fn release_deletes_the_file_and_copy_keeps_bytes() {
        let root = tempfile::tempdir().unwrap();
        let store = ResponseStore::new(root.path().join("responses"));
        let mut file = store.create().unwrap();
        file.write_all(&[0, 159, 146, 150, 255]).unwrap();
        let id = store.insert(file);
        let path = store.path(&id).unwrap();
        let dest = root.path().join("saved.bin");
        store.copy_body(&id, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), vec![0, 159, 146, 150, 255]);
        store.release(&id);
        assert!(!path.exists());
        assert!(store.copy_body(&id, &dest).is_err());
        store.release("unknown");
    }
}
```

- [ ] **Step 2: Write the transport tests**

In `src-tauri/src/request_tests.rs`, add helpers after `following`:

```rust
fn store() -> (tempfile::TempDir, ResponseStore) {
    let root = tempfile::tempdir().unwrap();
    let store = ResponseStore::new(root.path().join("responses"));
    (root, store)
}

async fn run(request: RequestInput, options: TransportOptions) -> Result<ResponseOutput, String> {
    let (_root, store) = store();
    execute(request, options, &store, DOWNLOAD_LIMIT).await
}
```

Change every `execute(input(..), X)` call from Task 3 to `run(input(..), X)`. Delete `rejects_oversized_response` and add:

```rust
#[tokio::test]
async fn truncates_the_preview_and_stores_the_full_body() {
    let body = "x".repeat(1024 * 1024 + 10).into_bytes();
    let base = serve(vec![("200 OK", String::new(), body.clone())]);
    let (_root, store) = store();
    let result = execute(
        input(base),
        TransportOptions { inspection_limit_mib: 1, ..options() },
        &store,
        DOWNLOAD_LIMIT,
    )
    .await
    .unwrap();
    assert!(result.truncated);
    assert!(!result.binary);
    assert_eq!(result.size_bytes, body.len());
    assert_eq!(result.body.len(), 1024 * 1024);
    let path = store.path(result.body_id.as_deref().unwrap()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
}

#[tokio::test]
async fn detects_binary_bodies() {
    for body in [vec![b'a', 0, b'b'], vec![b'a', 0xff, b'b']] {
        let base = serve(vec![("200 OK", String::new(), body)]);
        let result = run(input(base), options()).await.unwrap();
        assert!(result.binary);
        assert_eq!(result.body, "");
    }
}

#[tokio::test]
async fn a_character_cut_at_the_preview_limit_stays_text() {
    // "é" is two bytes. Put its first byte at the last preview position.
    let mut body = "a".repeat(1024 * 1024 - 1).into_bytes();
    body.extend_from_slice("é tail".as_bytes());
    let base = serve(vec![("200 OK", String::new(), body)]);
    let result = run(
        input(base),
        TransportOptions { inspection_limit_mib: 1, ..options() },
    )
    .await
    .unwrap();
    assert!(!result.binary);
    assert!(result.truncated);
    assert_eq!(result.body.len(), 1024 * 1024 - 1);
}

#[tokio::test]
async fn rejects_a_body_over_the_download_limit_and_keeps_no_file() {
    let base = serve(vec![("200 OK", String::new(), vec![b'x'; 64])]);
    let (root, store) = store();
    let error = execute(input(base), options(), &store, 32).await.unwrap_err();
    assert_eq!(error, "Response exceeds the 1 GiB download limit.");
    assert_eq!(
        std::fs::read_dir(root.path().join("responses")).unwrap().count(),
        0
    );
}
```

Update `error_status_retains_body` and any other test that reads `result.body` if its assertions fail. They should still pass, because small text bodies return the full text.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test`
Expected: compile errors. `ResponseStore`, `DOWNLOAD_LIMIT`, and the new `execute` arguments do not exist.

- [ ] **Step 4: Implement `ResponseStore`**

Put this above the tests module in `src-tauri/src/response_store.rs`:

```rust
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tempfile::NamedTempFile;

pub const STORE_ERROR: &str = "Cannot store the response. Check free disk space.";

/// Raw response bodies on disk, one temp file per response. Files do not
/// survive a restart: `new` clears the directory.
pub struct ResponseStore {
    dir: PathBuf,
    files: Mutex<HashMap<String, NamedTempFile>>,
}

impl ResponseStore {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        Self {
            dir,
            files: Mutex::new(HashMap::new()),
        }
    }

    /// A new file in the store directory. Dropping it without `insert`
    /// deletes it.
    pub fn create(&self) -> Result<NamedTempFile, String> {
        fs::create_dir_all(&self.dir).map_err(|_| STORE_ERROR.to_string())?;
        tempfile::Builder::new()
            .prefix("body-")
            .tempfile_in(&self.dir)
            .map_err(|_| STORE_ERROR.to_string())
    }

    /// Keep a completed file. The file name is the id.
    pub fn insert(&self, file: NamedTempFile) -> String {
        let id = file
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.files.lock().unwrap().insert(id.clone(), file);
        id
    }

    pub fn release(&self, id: &str) {
        self.files.lock().unwrap().remove(id);
    }

    pub fn copy_body(&self, id: &str, dest: &Path) -> Result<(), String> {
        // Do not hold the lock while copying a large file.
        let source = self
            .files
            .lock()
            .unwrap()
            .get(id)
            .map(|file| file.path().to_path_buf())
            .ok_or_else(|| "The response body is no longer available".to_string())?;
        fs::copy(source, dest).map(|_| ()).map_err(|error| error.to_string())
    }

    #[cfg(test)]
    pub fn path(&self, id: &str) -> Option<PathBuf> {
        self.files
            .lock()
            .unwrap()
            .get(id)
            .map(|file| file.path().to_path_buf())
    }
}

#[tauri::command]
pub fn release_response(store: tauri::State<'_, ResponseStore>, body_id: String) {
    store.release(&body_id);
}
```

- [ ] **Step 5: Stream bodies into the store in `lib.rs`**

In `src-tauri/src/lib.rs`:

1. Replace `const RESPONSE_LIMIT: usize = 4 * 1024 * 1024;` with:

```rust
const MIB: usize = 1024 * 1024;
const DOWNLOAD_LIMIT: u64 = 1024 * 1024 * 1024;
```

2. Add `mod response_store;` next to `mod app_state;`, and add `use response_store::{ResponseStore, STORE_ERROR};` and `use std::io::Write;` to the imports.

3. Add three fields to `ResponseOutput`, after `size_bytes`:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    body_id: Option<String>,
    truncated: bool,
    binary: bool,
```

4. Add the preview decoder:

```rust
/// Decode the preview as UTF-8 text. Returns `(text, binary)`. A NUL byte or
/// an invalid sequence makes the body binary. An incomplete character at the
/// end is the preview cut, not binary data, so it is dropped.
fn decode_preview(mut preview: Vec<u8>) -> (String, bool) {
    if preview.contains(&0) {
        return (String::new(), true);
    }
    match std::str::from_utf8(&preview) {
        Ok(_) => (String::from_utf8(preview).unwrap_or_default(), false),
        Err(error) if error.error_len().is_none() => {
            preview.truncate(error.valid_up_to());
            (String::from_utf8(preview).unwrap_or_default(), false)
        }
        Err(_) => (String::new(), true),
    }
}
```

5. Change `execute` to take the store and the limit:

```rust
async fn execute(
    mut request: RequestInput,
    options: TransportOptions,
    store: &ResponseStore,
    download_limit: u64,
) -> Result<ResponseOutput, String> {
```

Replace the body loop and everything from `let mut bytes = Vec::new();` to the end of the function with:

```rust
    let preview_limit = options.inspection_limit_mib * MIB;
    let mut file = store.create()?;
    let mut size: u64 = 0;
    let mut preview = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| network_error(error, &options))?
    {
        size += chunk.len() as u64;
        if size > download_limit {
            // Dropping `file` deletes it.
            return Err("Response exceeds the 1 GiB download limit.".to_string());
        }
        file.write_all(&chunk).map_err(|_| STORE_ERROR.to_string())?;
        if preview.len() < preview_limit {
            let take = (preview_limit - preview.len()).min(chunk.len());
            preview.extend_from_slice(&chunk[..take]);
        }
    }
    let duration_ms = started_at.elapsed().as_millis();
    let truncated = size > preview.len() as u64;
    let (body, binary) = decode_preview(preview);
    let body_id = store.insert(file);

    Ok(ResponseOutput {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
        duration_ms,
        headers,
        body,
        size_bytes: size as usize,
        final_url,
        redirect_count: (redirect_count > 0).then_some(redirect_count),
        body_id: Some(body_id),
        truncated,
        binary,
    })
}
```

Keep the `redirect_count` and `final_url` lines from Task 3 where they are, right after `send()`.

6. Update the command:

```rust
#[tauri::command]
async fn send_request(
    request: RequestInput,
    options: TransportOptions,
    store: tauri::State<'_, ResponseStore>,
) -> Result<ResponseOutput, String> {
    execute(request, options, &store, DOWNLOAD_LIMIT).await
}
```

7. In `run()`, in `.setup`, add after the `app_state` line:

```rust
            app.manage(ResponseStore::new(app.path().app_cache_dir()?.join("responses")));
```

Add `response_store::release_response` to `generate_handler!`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/response_store.rs src-tauri/src/lib.rs src-tauri/src/request_tests.rs
git commit -m "feat: store native response bodies in temp files"
```

---

### Task 5: Rust save commands

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/response_store.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `ResponseStore::copy_body` (Task 4).
- Produces: the Tauri commands `save_response(body_id: String, suggested_name: String) -> Result<bool, String>` and `save_response_text(text: String, suggested_name: String) -> Result<bool, String>`. The JS names are `bodyId`, `suggestedName`, and `text`. `false` means the user cancelled. An `Err` holds only the reason, without the "Cannot save the file:" prefix.

The dialog cannot run in `cargo test`. `copy_body` already has tests from Task 4. This task has no new automated test. Step 4 is the manual check.

- [ ] **Step 1: Add the dependency**

Run: `cd src-tauri && cargo add tauri-plugin-dialog@2`
Expected: `tauri-plugin-dialog = "2..."` under `[dependencies]` in `Cargo.toml`.

- [ ] **Step 2: Add the commands to `response_store.rs`**

Above the tests module:

```rust
/// Rust opens the dialog, so the webview never supplies a file path.
/// `blocking_save_file` must not run on the main thread. Async commands run
/// on the async runtime, not the main thread.
fn pick_save_path(app: &tauri::AppHandle, suggested_name: &str) -> Option<PathBuf> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .set_file_name(suggested_name)
        .blocking_save_file()?
        .into_path()
        .ok()
}

#[tauri::command]
pub async fn save_response(
    app: tauri::AppHandle,
    store: tauri::State<'_, ResponseStore>,
    body_id: String,
    suggested_name: String,
) -> Result<bool, String> {
    let Some(dest) = pick_save_path(&app, &suggested_name) else {
        return Ok(false);
    };
    store.copy_body(&body_id, &dest)?;
    Ok(true)
}

#[tauri::command]
pub async fn save_response_text(
    app: tauri::AppHandle,
    text: String,
    suggested_name: String,
) -> Result<bool, String> {
    let Some(dest) = pick_save_path(&app, &suggested_name) else {
        return Ok(false);
    };
    fs::write(dest, text).map_err(|error| error.to_string())?;
    Ok(true)
}
```

- [ ] **Step 3: Register the plugin and the commands in `lib.rs`**

In `run()`, add `.plugin(tauri_plugin_dialog::init())` after the opener plugin. Add `response_store::save_response` and `response_store::save_response_text` to `generate_handler!`. Do not change `src-tauri/capabilities/default.json`.

- [ ] **Step 4: Build and run the tests**

Run: `cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS with no warnings. If `clippy` is not installed, run `cargo build` instead.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/response_store.rs src-tauri/src/lib.rs
git commit -m "feat: native save dialog for response bodies"
```

---

### Task 6: Browser transport, preview, and response-body module

**Files:**
- Modify: `src/lib/request.ts:41-48`, `src/lib/transport.ts`
- Create: `src/lib/response-body.ts`
- Test: `src/lib/__test__/transport.test.ts`, `src/lib/__test__/response-body.test.ts` (create)

**Interfaces:**
- Consumes: `TransportOptions`, `defaultTransportOptions`, `MIB`, `DOWNLOAD_LIMIT` (Task 1). The Rust commands from Tasks 4–5.
- Produces:
  - `ApiResponse` gains `bodyId?: string; truncated?: boolean; binary?: boolean; finalUrl?: string; redirectCount?: number`. They are optional so existing fixtures stay valid. A missing flag means `false`.
  - `sendRequest(request: RequestInput, options?: TransportOptions, downloadLimit?: number): Promise<ApiResponse>`
  - `decodePreview(bytes: Uint8Array): { text: string; binary: boolean }` (exported from `transport.ts` for tests)
  - `response-body.ts`: `storeBlob(blob: Blob): string`, `releaseResponse(response: ApiResponse | null | undefined): void`, `canSaveResponse(response: ApiResponse): boolean`, `saveResponse(response: ApiResponse, requestUrl: string): Promise<boolean>`, `suggestedFileName(response: ApiResponse, requestUrl: string): string`, `BODY_UNAVAILABLE = "The response body is no longer available"`.

- [ ] **Step 1: Write the failing `response-body` tests**

Create `src/lib/__test__/response-body.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  canSaveResponse,
  releaseResponse,
  saveResponse,
  storeBlob,
  suggestedFileName,
} from "../response-body";
import type { ApiResponse } from "../request";

const base: ApiResponse = {
  status: 200,
  statusText: "OK",
  durationMs: 1,
  sizeBytes: 2,
  headers: [],
  body: "{}",
};
const withHeader = (key: string, value: string): ApiResponse => ({
  ...base,
  headers: [{ key, value }],
});
afterEach(() => vi.restoreAllMocks());

describe("suggestedFileName", () => {
  it("prefers Content-Disposition", () => {
    expect(
      suggestedFileName(
        withHeader("Content-Disposition", 'attachment; filename="report.csv"'),
        "https://x.test/a/b",
      ),
    ).toBe("report.csv");
    expect(
      suggestedFileName(
        withHeader(
          "content-disposition",
          "attachment; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf",
        ),
        "https://x.test/",
      ),
    ).toBe("résumé.pdf");
  });
  it("removes path separators and control characters", () => {
    expect(
      suggestedFileName(
        withHeader(
          "Content-Disposition",
          'attachment; filename="../../etc/pass\u0007wd"',
        ),
        "https://x.test/",
      ),
    ).toBe("....etcpasswd");
  });
  it("uses the last URL segment, then the content type", () => {
    expect(suggestedFileName(base, "https://x.test/files/logo.png?x=1")).toBe(
      "logo.png",
    );
    expect(
      suggestedFileName(
        { ...base, finalUrl: "https://cdn.test/real.zip" },
        "https://x.test/download",
      ),
    ).toBe("real.zip");
    expect(
      suggestedFileName(
        withHeader("Content-Type", "application/json; charset=utf-8"),
        "https://x.test/",
      ),
    ).toBe("response.json");
    expect(suggestedFileName(base, "not a url")).toBe("response.bin");
  });
});

describe("canSaveResponse", () => {
  it("needs a stored body unless the preview is complete text", () => {
    expect(canSaveResponse(base)).toBe(true);
    expect(canSaveResponse({ ...base, truncated: true })).toBe(false);
    expect(canSaveResponse({ ...base, binary: true })).toBe(false);
    expect(canSaveResponse({ ...base, binary: true, bodyId: "b" })).toBe(true);
  });
});

describe("browser saves", () => {
  it("downloads a stored blob and forgets it after release", async () => {
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => {});
    URL.createObjectURL = vi.fn(() => "blob:test");
    URL.revokeObjectURL = vi.fn();
    const bodyId = storeBlob(new Blob([new Uint8Array([0, 1])]));
    const response = { ...base, bodyId, binary: true };
    expect(await saveResponse(response, "https://x.test/f.bin")).toBe(true);
    expect(click).toHaveBeenCalledOnce();
    releaseResponse(response);
    await expect(saveResponse(response, "https://x.test/f.bin")).rejects.toThrow(
      "no longer available",
    );
  });
  it("saves complete preview text without a stored body", async () => {
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    URL.createObjectURL = vi.fn(() => "blob:test");
    URL.revokeObjectURL = vi.fn();
    expect(await saveResponse(base, "https://x.test/")).toBe(true);
    await expect(
      saveResponse({ ...base, truncated: true }, "https://x.test/"),
    ).rejects.toThrow("no longer available");
  });
});
```

- [ ] **Step 2: Write the failing transport tests**

In `src/lib/__test__/transport.test.ts`:

1. Change the import to:

```ts
import { decodePreview, sendRequest } from "../transport";
import { defaultTransportOptions, MIB } from "../transport-options";
```

2. Replace the `"bounds response memory while reading the stream"` test with:

```ts
  it("stops reading at the download limit", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response(new Uint8Array(65))),
    );
    await expect(
      sendRequest(request, defaultTransportOptions(), 64),
    ).rejects.toThrow("1 GiB download limit");
  });
  it("truncates the preview at the inspection limit", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("x".repeat(MIB + 5))),
    );
    const result = await sendRequest(request, {
      ...defaultTransportOptions(),
      inspectionLimitMiB: 1,
    });
    expect(result).toMatchObject({
      truncated: true,
      binary: false,
      sizeBytes: MIB + 5,
    });
    expect(result.body.length).toBe(MIB);
    expect(result.bodyId).toMatch(/^browser-/);
  });
  it("follows redirects when the setting is on", async () => {
    const response = new Response("done");
    Object.defineProperty(response, "redirected", { value: true });
    Object.defineProperty(response, "url", { value: "https://final.test/" });
    const fetch = vi.fn().mockResolvedValue(response);
    vi.stubGlobal("fetch", fetch);
    const result = await sendRequest(request, {
      ...defaultTransportOptions(),
      followRedirects: true,
    });
    expect(fetch.mock.calls[0][1]).toMatchObject({ redirect: "follow" });
    expect(result.finalUrl).toBe("https://final.test/");
    expect(result.redirectCount).toBeUndefined();
  });
```

3. In the timeout test, change `"30 seconds"` to `"after 5 seconds"`, call `sendRequest(request, { ...defaultTransportOptions(), timeoutSeconds: 5 })`, and advance timers by `5_000`.

4. Add a `decodePreview` block:

```ts
describe("decodePreview", () => {
  const bytes = (...values: number[]) => new Uint8Array(values);
  it("treats NUL and invalid sequences as binary", () => {
    expect(decodePreview(bytes(0x61, 0x00))).toEqual({ text: "", binary: true });
    expect(decodePreview(bytes(0x61, 0xff, 0x62))).toEqual({
      text: "",
      binary: true,
    });
    expect(decodePreview(bytes(0x61, 0xff))).toEqual({ text: "", binary: true });
  });
  it("drops an incomplete character at the cut", () => {
    // "é" is C3 A9; "€" is E2 82 AC.
    expect(decodePreview(bytes(0x61, 0xc3))).toEqual({
      text: "a",
      binary: false,
    });
    expect(decodePreview(bytes(0x61, 0xe2, 0x82))).toEqual({
      text: "a",
      binary: false,
    });
    expect(decodePreview(bytes(0x61, 0xc3, 0xa9))).toEqual({
      text: "aé",
      binary: false,
    });
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `bun run test src/lib/__test__/transport.test.ts src/lib/__test__/response-body.test.ts`
Expected: FAIL. `../response-body` does not exist, and `decodePreview` is not exported.

- [ ] **Step 4: Extend `ApiResponse` in `src/lib/request.ts`**

```ts
export type ApiResponse = {
  status: number;
  statusText: string;
  durationMs: number;
  headers: Header[];
  /** Preview text: complete unless `truncated`; empty when `binary`. */
  body: string;
  /** Full body size, not the preview size. */
  sizeBytes: number;
  /** Stored raw body. Not saved in the workspace. */
  bodyId?: string;
  truncated?: boolean;
  binary?: boolean;
  /** Set only when a redirect happened. */
  finalUrl?: string;
  /** Desktop only. */
  redirectCount?: number;
};
```

- [ ] **Step 5: Create `src/lib/response-body.ts`**

```ts
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ApiResponse } from "./request";

export const BODY_UNAVAILABLE = "The response body is no longer available";

const native = isTauri();
// Browser preview only: raw bodies by id.
const blobs = new Map<string, Blob>();
let nextBlobId = 0;

export function storeBlob(blob: Blob) {
  const id = `browser-${++nextBlobId}`;
  blobs.set(id, blob);
  return id;
}

export function releaseResponse(response: ApiResponse | null | undefined) {
  const id = response?.bodyId;
  if (!id) return;
  if (native) void invoke("release_response", { bodyId: id }).catch(() => {});
  else blobs.delete(id);
}

/** After a restart only complete text previews can be saved. */
export const canSaveResponse = (response: ApiResponse) =>
  Boolean(response.bodyId) || (!response.truncated && !response.binary);

/** Resolves false when the user cancels the save dialog. */
export async function saveResponse(
  response: ApiResponse,
  requestUrl: string,
): Promise<boolean> {
  const suggestedName = suggestedFileName(response, requestUrl);
  if (response.bodyId) {
    if (native)
      return invoke<boolean>("save_response", {
        bodyId: response.bodyId,
        suggestedName,
      });
    const blob = blobs.get(response.bodyId);
    if (!blob) throw new Error(BODY_UNAVAILABLE);
    download(blob, suggestedName);
    return true;
  }
  if (!canSaveResponse(response)) throw new Error(BODY_UNAVAILABLE);
  if (native)
    return invoke<boolean>("save_response_text", {
      text: response.body,
      suggestedName,
    });
  download(new Blob([response.body], { type: "text/plain" }), suggestedName);
  return true;
}

function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  // Revoking at once can cancel the download in some browsers.
  setTimeout(() => URL.revokeObjectURL(url), 40_000);
}

const extensions: Record<string, string> = {
  "application/json": ".json",
  "application/xml": ".xml",
  "text/xml": ".xml",
  "text/html": ".html",
  "text/plain": ".txt",
  "image/png": ".png",
  "image/jpeg": ".jpg",
  "application/pdf": ".pdf",
  "application/zip": ".zip",
};

function header(response: ApiResponse, name: string) {
  return response.headers.find(({ key }) => key.toLowerCase() === name)?.value;
}

function decode(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

// No path separators, control characters, or names that mean a directory.
function clean(name: string) {
  // eslint-disable-next-line no-control-regex
  const safe = name.replace(/[/\\\u0000-\u001f\u007f]/g, "").trim();
  return safe === "." || safe === ".." ? "" : safe;
}

export function suggestedFileName(response: ApiResponse, requestUrl: string) {
  const disposition = header(response, "content-disposition") ?? "";
  const encoded = disposition.match(/filename\*\s*=\s*UTF-8''([^;]+)/i);
  const plain = disposition.match(/filename\s*=\s*"?([^";]+)"?/i);
  const fromHeader = clean(
    encoded ? decode(encoded[1].trim()) : (plain?.[1].trim() ?? ""),
  );
  if (fromHeader) return fromHeader;
  try {
    const segment = new URL(response.finalUrl ?? requestUrl).pathname
      .split("/")
      .filter(Boolean)
      .pop();
    const fromUrl = clean(decode(segment ?? ""));
    if (fromUrl) return fromUrl;
  } catch {
    // Not an absolute URL: fall through to the content type.
  }
  const type =
    header(response, "content-type")?.split(";")[0].trim().toLowerCase() ?? "";
  return `response${extensions[type] ?? ".bin"}`;
}
```

If `oxlint` does not accept the `eslint-disable-next-line` comment, change it to `// oxlint-disable-next-line no-control-regex`.

- [ ] **Step 6: Update `src/lib/transport.ts`**

Replace the file with:

```ts
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ApiResponse, RequestInput } from "./request";
import { storeBlob } from "./response-body";
import {
  defaultTransportOptions,
  DOWNLOAD_LIMIT,
  MIB,
  type TransportOptions,
} from "./transport-options";
export const nativeTransport = isTauri();

// Bytes of a UTF-8 character that the preview cut in two. 0 when the preview
// ends on a character boundary.
function incompleteTail(bytes: Uint8Array) {
  for (let cut = 1; cut <= Math.min(3, bytes.length); cut++) {
    const byte = bytes[bytes.length - cut];
    if (byte >= 0x80 && byte <= 0xbf) continue;
    // Not a lead byte: the decoder reports it as invalid.
    if (byte < 0xc2 || byte > 0xf4) return 0;
    const length = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1;
    return length > cut ? cut : 0;
  }
  return 0;
}

/** Same rule as the desktop transport: NUL or invalid UTF-8 is binary. */
export function decodePreview(bytes: Uint8Array) {
  if (bytes.includes(0)) return { text: "", binary: true };
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(
      bytes.subarray(0, bytes.length - incompleteTail(bytes)),
    );
    return { text, binary: false };
  } catch {
    return { text: "", binary: true };
  }
}

export async function sendRequest(
  request: RequestInput,
  options: TransportOptions = defaultTransportOptions(),
  downloadLimit = DOWNLOAD_LIMIT,
): Promise<ApiResponse> {
  if (nativeTransport)
    return invoke<ApiResponse>("send_request", { request, options });
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    options.timeoutSeconds * 1000,
  );
  const start = performance.now();
  try {
    const headers = new Headers();
    request.headers.forEach(({ key, value }) => headers.append(key, value));
    const result = await fetch(request.url, {
      method: request.method,
      headers,
      body: request.body,
      signal: controller.signal,
      credentials: "omit",
      cache: "no-store",
      redirect: options.followRedirects ? "follow" : "manual",
      referrerPolicy: "no-referrer",
    });
    if (result.type === "opaqueredirect")
      throw new Error(
        "Browser preview cannot inspect redirects. Use the desktop app.",
      );
    const previewLimit = options.inspectionLimitMiB * MIB;
    const reader = result.body?.getReader();
    let sizeBytes = 0;
    const chunks: Uint8Array[] = [];
    if (reader) {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        sizeBytes += value.byteLength;
        if (sizeBytes > downloadLimit) {
          await reader.cancel();
          throw new Error("Response exceeds the 1 GiB download limit.");
        }
        chunks.push(value);
      }
    }
    const preview = new Uint8Array(Math.min(sizeBytes, previewLimit));
    let offset = 0;
    for (const chunk of chunks) {
      if (offset >= preview.length) break;
      const part = chunk.subarray(0, preview.length - offset);
      preview.set(part, offset);
      offset += part.byteLength;
    }
    const { text, binary } = decodePreview(preview);
    const contentType = result.headers.get("content-type") ?? "";
    return {
      status: result.status,
      statusText: result.statusText,
      durationMs: Math.round(performance.now() - start),
      sizeBytes,
      headers: Array.from(result.headers, ([key, value]) => ({ key, value })),
      body: text,
      bodyId: storeBlob(new Blob(chunks, { type: contentType })),
      truncated: sizeBytes > preview.length,
      binary,
      ...(result.redirected ? { finalUrl: result.url } : {}),
    };
  } catch (error) {
    if (controller.signal.aborted)
      throw new Error(
        `Request timed out after ${options.timeoutSeconds} seconds.`,
      );
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}
```

`RESPONSE_LIMIT` is removed. Search for other imports: `grep -rn RESPONSE_LIMIT src e2e`. Remove each one or replace it with `MIB * 4`.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `bun run test src/lib/__test__/`
Expected: PASS. The first existing test (`"omits ambient credentials..."`) still expects `redirect: "manual"`, which is the default.

- [ ] **Step 8: Type-check, lint, and commit**

Run: `bun run build && bun run lint`
Expected: no errors.

```bash
git add src/lib/request.ts src/lib/transport.ts src/lib/response-body.ts src/lib/__test__/transport.test.ts src/lib/__test__/response-body.test.ts
git commit -m "feat: browser response preview, blob store and save"
```

---

### Task 7: Wire options and release through the app

**Files:**
- Modify: `src/composables/useRequestRunner.ts`, `src/composables/useWorkspaceState.ts:262-272`, `src/lib/graphql-schema.ts:38-54`, `src/lib/workspace.ts` (validate, encode, decode), `src/components/RequestWorkspace.vue`, `src/components/RequestEditor.vue`, `src/App.vue`
- Test: `src/composables/__test__/useRequestRunner.test.ts`, `src/lib/__test__/graphql-schema.test.ts`, `src/lib/__test__/workspace-v4.test.ts`

**Interfaces:**
- Consumes: `sendRequest(request, options)` (Task 6), `releaseResponse` (Task 6), `transportOptions(preferences)` (Task 1).
- Produces:
  - `useRequestRunner(session, contextSource?, optionsSource?: MaybeRefOrGetter<TransportOptions | undefined>)`
  - `fetchSchema(draft, ctx?, options?: TransportOptions)`
  - `RequestWorkspace` prop `transport?: TransportOptions`; `RequestEditor` prop `transport?: TransportOptions`
  - `ResponsePanel` gets new props `requestUrl` and `timeoutSeconds` from `RequestWorkspace`. Task 8 declares them. Pass them in this task. Vue ignores unknown props until then, and the attributes fall through harmlessly.

- [ ] **Step 1: Write the failing tests**

In `src/composables/__test__/useRequestRunner.test.ts`, add below the existing `vi.mock`:

```ts
vi.mock("@/lib/response-body", () => ({ releaseResponse: vi.fn() }));
```

Add these imports: `import { sendRequest } from "@/lib/transport";`, `import { releaseResponse } from "@/lib/response-body";`, and `import { defaultTransportOptions } from "@/lib/transport-options";`. Then add:

```ts
describe("transport options and body release", () => {
  it("sends with the given options and releases the previous body", async () => {
    const session = createSession();
    session.draft.url = "https://example.test/";
    const previous = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 0,
      headers: [],
      body: "",
      bodyId: "old",
    };
    session.response = previous;
    const options = { ...defaultTransportOptions(), timeoutSeconds: 7 };
    const { send } = scope.run(() =>
      useRequestRunner(session, undefined, () => options),
    )!;
    await send();
    expect(vi.mocked(releaseResponse)).toHaveBeenCalledWith(previous);
    expect(vi.mocked(sendRequest).mock.calls.at(-1)![1]).toEqual(options);
  });

  it("releases a result that arrives after the scope stops", async () => {
    let resolve!: (value: unknown) => void;
    vi.mocked(sendRequest).mockImplementationOnce(
      () => new Promise((done) => (resolve = done)) as never,
    );
    const session = createSession();
    session.draft.url = "https://example.test/";
    const local = effectScope();
    const { send } = local.run(() => useRequestRunner(session))!;
    const pending = send();
    local.stop();
    const late = { status: 200, statusText: "OK", durationMs: 1, sizeBytes: 0, headers: [], body: "", bodyId: "late" };
    resolve(late);
    await pending;
    expect(vi.mocked(releaseResponse)).toHaveBeenCalledWith(late);
    expect(session.response).toBeNull();
  });
});
```

Use the same `scope` setup and `createSession` call as the existing tests in this file. If `createSession` needs a draft argument there, pass it the same way.

In `src/lib/__test__/graphql-schema.test.ts`, change the mock to:

```ts
vi.mock("../transport", () => ({ sendRequest: vi.fn() }));
vi.mock("../response-body", () => ({ releaseResponse: vi.fn() }));
```

Import `releaseResponse` from `../response-body`, and add:

```ts
  it("rejects a truncated schema response and releases it", async () => {
    send.mockResolvedValueOnce({
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 10,
      headers: [],
      body: "{",
      truncated: true,
      bodyId: "b",
    });
    await expect(fetchSchema(draft)).rejects.toThrow(
      "Schema response exceeds the inspection limit.",
    );
    expect(vi.mocked(releaseResponse)).toHaveBeenCalled();
  });
```

Use the `draft` value that the existing tests in this file pass to `fetchSchema`.

In `src/lib/__test__/workspace-v4.test.ts`, add:

```ts
it("keeps optional response fields but never saves the body id", () => {
  const session = createSession();
  session.response = {
    status: 200,
    statusText: "OK",
    durationMs: 1,
    sizeBytes: 9,
    headers: [],
    body: "",
    bodyId: "body-1",
    truncated: true,
    binary: true,
    finalUrl: "https://final.test/",
    redirectCount: 2,
  };
  const encoded = encodeWorkspace([session], session.id);
  expect(encoded).not.toContain("body-1");
  const restored = decodeWorkspace(encoded).sessions[0].response!;
  expect(restored).toMatchObject({
    truncated: true,
    binary: true,
    finalUrl: "https://final.test/",
    redirectCount: 2,
  });
  expect(restored.bodyId).toBeUndefined();
});

it("drops a body id found in a saved workspace", () => {
  const session = createSession();
  session.response = { status: 200, statusText: "OK", durationMs: 1, sizeBytes: 0, headers: [], body: "" };
  const data = JSON.parse(encodeWorkspace([session], session.id));
  data.tabs[0].response.bodyId = "stale";
  expect(
    decodeWorkspace(JSON.stringify(data)).sessions[0].response!.bodyId,
  ).toBeUndefined();
});
```

Match the imports already used in `workspace-v4.test.ts`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun run test src/composables/__test__/useRequestRunner.test.ts src/lib/__test__/graphql-schema.test.ts src/lib/__test__/workspace-v4.test.ts`
Expected: FAIL. The release is not called, and `bodyId` is saved.

- [ ] **Step 3: Update `useRequestRunner.ts`**

Add the imports:

```ts
import { releaseResponse } from "@/lib/response-body";
import {
  defaultTransportOptions,
  type TransportOptions,
} from "@/lib/transport-options";
```

Add the parameter and its doc line:

```ts
 * @param optionsSource  Optional reactive source for the transport settings.
 *   Defaults apply when omitted.
 */
export function useRequestRunner(
  session: RequestSession,
  contextSource?: MaybeRefOrGetter<ResolvedRequestContext | undefined>,
  optionsSource?: MaybeRefOrGetter<TransportOptions | undefined>,
) {
```

In `send()`, replace `session.response = null;` with:

```ts
    releaseResponse(session.response);
    session.response = null;
```

Replace the `try` body with:

```ts
      const options = toValue(optionsSource) ?? defaultTransportOptions();
      const result = await sendRequest(request, options);
      // A result for an unmounted view has no owner, so free its body.
      if (alive) session.response = result;
      else releaseResponse(result);
```

- [ ] **Step 4: Update `graphql-schema.ts`**

Add the imports `import { releaseResponse } from "./response-body";` and `import type { TransportOptions } from "./transport-options";`. Change `fetchSchema`:

```ts
export async function fetchSchema(
  draft: Draft,
  ctx?: ResolvedRequestContext,
  options?: TransportOptions,
): Promise<GraphQLSchema> {
  const { buildClientSchema, getIntrospectionQuery } = await import("graphql");
  const request = buildRequest(
    introspectionDraft(draft, getIntrospectionQuery()),
    ctx,
  );
  const response = await sendRequest(request, options);
  try {
    if (response.status < 200 || response.status >= 300)
      throw new Error(
        `Schema request failed: ${response.status} ${response.statusText}`.trim(),
      );
    if (response.truncated)
      throw new Error("Schema response exceeds the inspection limit.");
    const schema = markRaw(
      parseIntrospection(response.body, buildClientSchema),
    );
    cache.set(request.url, { schema, fetchedAt: Date.now() });
    return schema;
  } finally {
    releaseResponse(response);
  }
}
```

The existing test that checks `send` was called with the request (`send.mock.calls[0][0]`) still passes. If a test uses `toHaveBeenCalledWith(request)`, add `undefined` as the second argument.

- [ ] **Step 5: Update `workspace.ts`**

At the end of `validateResponse`, before the headers loop, add:

```ts
  check(
    (response.bodyId === undefined || text(response.bodyId)) &&
      (response.truncated === undefined ||
        typeof response.truncated === "boolean") &&
      (response.binary === undefined || typeof response.binary === "boolean") &&
      (response.finalUrl === undefined || text(response.finalUrl)) &&
      (response.redirectCount === undefined ||
        numeric(response.redirectCount)),
  );
```

In `encodeWorkspace`, in the `tabs` mapping, replace the shorthand `response,` in the returned object with:

```ts
        // The body file does not survive a restart.
        response: response && { ...response, bodyId: undefined },
```

In `decodeWorkspace`, after the `data.tabs.map(...)` that builds `sessions`, add to the existing `for (const session of sessions)` loop:

```ts
    if (session.response) delete session.response.bodyId;
```

- [ ] **Step 6: Release on delete in `useWorkspaceState.ts`**

Add `import { releaseResponse } from "@/lib/response-body";`. In `deleteRequest`, before `sessions.value.splice(index, 1);`, add:

```ts
    releaseResponse(sessions.value[index].response);
```

- [ ] **Step 7: Wire the props**

`src/App.vue`:
- Add `transportOptions` to the import from `@/lib/preferences`, and `computed` to the `vue` import if it is missing.
- Add `const transport = computed(() => transportOptions(preferences.value));`.
- On `<RequestWorkspace>` add `:transport="transport"`.
- Replace the status-bar text `30 s TIMEOUT · 4 MiB LIMIT` with `{{ transport.timeoutSeconds }} s TIMEOUT · {{ transport.inspectionLimitMiB }} MiB LIMIT`.

`src/components/RequestWorkspace.vue`:
- Add `import type { TransportOptions } from "@/lib/transport-options";`.
- Add `transport?: TransportOptions;` to `defineProps`.
- Change the runner call to `useRequestRunner(props.session, resolvedCtx, () => props.transport)`.
- On `<RequestEditor>` add `:transport="transport"`.
- On `<ResponsePanel>` add `:request-url="session.draft.url"` and `:timeout-seconds="transport?.timeoutSeconds"`.

`src/components/RequestEditor.vue`:
- Add `transport?: TransportOptions;` to `defineProps`, with the same type import.
- Change `await fetchSchema(draft.value, props.ctx);` to `await fetchSchema(draft.value, props.ctx, props.transport);`.

- [ ] **Step 8: Run all unit tests**

Run: `bun run test`
Expected: PASS. If `src/__test__/App.test.ts` checks the text `30 s TIMEOUT · 4 MiB LIMIT`, it still passes with the default preferences.

- [ ] **Step 9: Type-check, lint, and commit**

Run: `bun run build && bun run lint`
Expected: no errors.

```bash
git add src/composables src/lib src/components/RequestWorkspace.vue src/components/RequestEditor.vue src/App.vue
git commit -m "feat: pass transport settings and release stored bodies"
```

---

### Task 8: Response panel states and Save

**Files:**
- Modify: `src/components/ResponsePanel.vue`
- Test: `src/components/__test__/ResponsePanel.test.ts`

**Interfaces:**
- Consumes: `canSaveResponse`, `saveResponse` from `@/lib/response-body` (Task 6); `ApiResponse` optional fields (Task 6).
- Produces: new props `requestUrl?: string` and `timeoutSeconds?: number` (default 30). Test hooks: `[data-save-response]`, `[data-response-binary]`, `[data-response-truncated]`, `[data-response-redirect]`, `[data-save-error]`.

- [ ] **Step 1: Write the failing tests**

Add to the top of `src/components/__test__/ResponsePanel.test.ts`:

```ts
import { flushPromises } from "@vue/test-utils";
vi.mock("@/lib/response-body", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/response-body")>()),
  saveResponse: vi.fn(),
}));
import { saveResponse } from "@/lib/response-body";
import type { ApiResponse } from "@/lib/request";
```

Append:

```ts
describe("stored bodies", () => {
  const response = (extra: Partial<ApiResponse> = {}): ApiResponse => ({
    status: 200,
    statusText: "OK",
    durationMs: 1,
    sizeBytes: 3,
    headers: [{ key: "Content-Type", value: "image/png" }],
    body: "abc",
    ...extra,
  });
  const render = (value: ApiResponse) =>
    mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: value,
        requestUrl: "https://x.test/logo.png",
      },
    });

  it("shows a binary notice in place of the editor", () => {
    const panel = render(
      response({ binary: true, body: "", sizeBytes: 2048, bodyId: "b" }),
    );
    expect(panel.get("[data-response-binary]").text()).toContain(
      "Binary response · 2.0 KiB · image/png",
    );
    expect(panel.find("[data-response-body]").exists()).toBe(false);
    expect(panel.find('[aria-label="Wrap lines"]').exists()).toBe(false);
    panel.unmount();
  });

  it("marks a truncated preview and disables formatting", () => {
    const panel = render(
      response({
        truncated: true,
        body: '{"a":1',
        sizeBytes: 5 * 1024 * 1024,
        bodyId: "b",
        headers: [{ key: "Content-Type", value: "application/json" }],
      }),
    );
    expect(panel.get("[data-response-truncated]").text()).toContain(
      "Preview shows the first 6 B of 5.00 MiB.",
    );
    const pretty = panel.findAll("button").find((b) => b.text() === "Raw")!;
    expect(pretty.attributes("disabled")).toBeDefined();
    expect(pretty.attributes("title")).toBe(
      "Unavailable for truncated responses",
    );
    panel.unmount();
  });

  it("disables Save when the stored body is gone", () => {
    const panel = render(response({ truncated: true }));
    const save = panel.get("[data-save-response]");
    expect(save.attributes("disabled")).toBeDefined();
    expect(save.attributes("title")).toBe(
      "Body is no longer available. Send the request again.",
    );
    panel.unmount();
  });

  it("saves and reports a failure inline", async () => {
    vi.mocked(saveResponse).mockRejectedValueOnce("disk full");
    const value = response({ bodyId: "b" });
    const panel = render(value);
    await panel.get("[data-save-response]").trigger("click");
    await flushPromises();
    expect(vi.mocked(saveResponse)).toHaveBeenCalledWith(
      value,
      "https://x.test/logo.png",
    );
    expect(panel.get("[data-save-error]").text()).toBe(
      "Cannot save the file: disk full.",
    );
    panel.unmount();
  });

  it("shows nothing when the user cancels", async () => {
    vi.mocked(saveResponse).mockResolvedValueOnce(false);
    const panel = render(response({ bodyId: "b" }));
    await panel.get("[data-save-response]").trigger("click");
    await flushPromises();
    expect(panel.find("[data-save-error]").exists()).toBe(false);
    panel.unmount();
  });

  it("shows the redirect target", () => {
    const desktop = render(
      response({ finalUrl: "https://final.test/", redirectCount: 2 }),
    );
    expect(desktop.get("[data-response-redirect]").text()).toBe(
      "→ https://final.test/ · 2 redirects",
    );
    desktop.unmount();
    const browser = render(response({ finalUrl: "https://final.test/" }));
    expect(browser.get("[data-response-redirect]").text()).toBe(
      "→ https://final.test/ · redirected",
    );
    browser.unmount();
  });

  it("uses the configured timeout while waiting", () => {
    const panel = mount(ResponsePanel, {
      props: {
        busy: true,
        error: "",
        elapsed: 1500,
        response: null,
        timeoutSeconds: 90,
      },
    });
    expect(panel.text()).toContain("1.5 s elapsed · 90 s timeout");
    panel.unmount();
  });
});
```

"first 6 B" is the UTF-8 size of the preview text `{"a":1`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun run test src/components/__test__/ResponsePanel.test.ts`
Expected: FAIL. `[data-response-binary]` is not found.

- [ ] **Step 3: Update the script**

In `ResponsePanel.vue`:

1. Add `Download` to the `lucide-vue-next` import. Add `import { canSaveResponse, saveResponse } from "@/lib/response-body";`.
2. Add to `defineProps`: `requestUrl?: string; timeoutSeconds?: number;`. Change `defineProps<...>()` to `withDefaults(defineProps<...>(), { timeoutSeconds: 30 })`.
3. Add `limited` and use it to block parsing, so formatting, the JSON tree, and jq are off for partial or binary bodies:

```ts
// A truncated or binary preview cannot be parsed as a whole document.
const limited = computed(() =>
  Boolean(props.response?.truncated || props.response?.binary),
);
const unavailable = "Unavailable for truncated responses";
```

In `sourceParsed` and `parsed`, add `if (limited.value) return null;` as the first line of each getter.

4. Add the save state:

```ts
const savable = computed(() =>
  props.response ? canSaveResponse(props.response) : false,
);
// UTF-8 size of the preview, shown in the truncated bar.
const previewSize = computed(() =>
  props.response?.truncated ? new Blob([props.response.body]).size : 0,
);
const saving = ref(false);
const saveError = ref("");
async function saveBody() {
  if (!props.response || !savable.value || saving.value) return;
  saving.value = true;
  saveError.value = "";
  try {
    await saveResponse(props.response, props.requestUrl ?? "");
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    saveError.value = `Cannot save the file: ${reason.replace(/\.$/, "")}.`;
  } finally {
    saving.value = false;
  }
}
const redirectLabel = computed(() => {
  const count = props.response?.redirectCount;
  return count === undefined
    ? "redirected"
    : `${count} ${count === 1 ? "redirect" : "redirects"}`;
});
```

5. In the `watch(() => props.response, ...)` callback, add `saveError.value = "";`.

- [ ] **Step 4: Update the template**

1. **Redirect line.** Add inside the status row `div`, after the size `span`:

```vue
        <span
          v-if="response.finalUrl"
          data-response-redirect
          class="basis-full min-w-0 truncate text-muted-foreground"
          :title="response.finalUrl"
          >→ {{ response.finalUrl }} · {{ redirectLabel }}</span
        >
```

2. **Toolbar.** Replace the Pretty button with:

```vue
                <Button
                  v-if="tab === 'body' && (parsed || response.truncated)"
                  variant="ghost"
                  :aria-pressed="pretty && !response.truncated"
                  :disabled="response.truncated"
                  :title="response.truncated ? unavailable : undefined"
                  @click="pretty = !pretty"
                >
                  {{ pretty && !response.truncated ? "Pretty" : "Raw" }}
                </Button>
```

Change `v-if="tab === 'body'"` on the Wrap and Find buttons to `v-if="tab === 'body' && !response.binary"`. Add this button before the Copy button:

```vue
                <Button
                  variant="ghost"
                  class="size-7 shrink-0 p-0"
                  data-save-response
                  aria-label="Save response body"
                  :title="
                    savable
                      ? 'Save response body…'
                      : 'Body is no longer available. Send the request again.'
                  "
                  :disabled="!savable || saving"
                  @click="saveBody"
                >
                  <Download :size="14" aria-hidden="true" />
                </Button>
```

3. **Context menu.** In the `<template v-if="tab === 'body'">` block of the toolbar context menu, change the condition to `tab === 'body' && !response.binary`. Add a separate item after "Copy response":

```vue
            <ContextMenuItem
              data-testid="ctx-save-response"
              :disabled="!savable"
              @select="saveBody"
            >
              Save response body…
            </ContextMenuItem>
```

4. **jq form.** Change `v-if="sourceParsed"` on the jq `<form>` to `v-if="sourceParsed || response.truncated"`. On its input add `:disabled="response.truncated"` and `:title="response.truncated ? unavailable : undefined"`. Change the Run jq button's `:disabled` to `!jqQuery.trim() || response.truncated`.

5. **Errors and truncated bar.** After the `jqError` paragraph, add:

```vue
        <p
          v-if="saveError"
          data-save-error
          role="alert"
          class="p-4 text-destructive text-xs"
        >
          {{ saveError }}
        </p>
        <p
          v-if="tab === 'body' && response.truncated && !response.binary"
          data-response-truncated
          role="status"
          class="flex flex-wrap items-center gap-1 py-1.5 px-3.5 border-b border-border text-muted-foreground font-mono text-[0.625rem]"
        >
          Preview shows the first {{ formatBytes(previewSize) }} of
          {{ formatBytes(response.sizeBytes) }}.
          <button
            type="button"
            class="underline decoration-dotted underline-offset-3 hover:text-foreground disabled:no-underline"
            :disabled="!savable || saving"
            @click="saveBody"
          >
            Save…
          </button>
          to get the full body.
        </p>
```

The test uses `toContain("Preview shows the first 6 B of 5.00 MiB.")`. Vue text interpolation across lines adds whitespace. If the test fails on spacing, put `Preview shows the first {{ formatBytes(previewSize) }} of {{ formatBytes(response.sizeBytes) }}.` on one line and add `<!-- prettier-ignore -->` above the `<p>`.

6. **Body tab.** Put the binary notice first in the body `TabsContent`, and change the `JsonTreeView` `v-if` to `v-else-if`:

```vue
          <div
            v-if="response.binary"
            data-response-binary
            class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-muted-foreground text-xs"
          >
            <p class="font-mono">
              Binary response · {{ formatBytes(response.sizeBytes) }} ·
              {{ contentType }}
            </p>
            <Button
              variant="secondary"
              :disabled="!savable || saving"
              @click="saveBody"
            >
              Save…
            </Button>
          </div>
          <JsonTreeView
            v-else-if="response.body && showJsonTree"
```

The same whitespace note applies to "Binary response · … · …".

7. **Waiting text.** Replace `· 30 s timeout` with `· {{ timeoutSeconds }} s timeout`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `bun run test src/components/__test__/`
Expected: PASS, including the existing `ResponsePanel` and `ResponsePanelContextMenu` tests.

- [ ] **Step 6: Lint, type-check, and commit**

Run: `bun run lint && bun run build`
Expected: no errors.

```bash
git add src/components/ResponsePanel.vue src/components/__test__/ResponsePanel.test.ts
git commit -m "feat: save, binary and truncated states in response panel"
```

---

### Task 9: E2E and README

**Files:**
- Create: `e2e/response-export.spec.ts`
- Modify: `README.md:31-33`, `README.md:160`

- [ ] **Step 1: Write the E2E test**

Create `e2e/response-export.spec.ts`:

```ts
import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";

test("a large response shows a preview and saves the full body", async ({
  page,
}) => {
  const body = "x".repeat(5 * 1024 * 1024);
  await page.route("https://example.test/**", (route) =>
    route.fulfill({ status: 200, contentType: "text/plain", body }),
  );
  await page.goto("/");
  await page
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/big.txt");
  await page.getByRole("button", { name: /Send/ }).click();
  const bar = page.locator("[data-response-truncated]");
  await expect(bar).toContainText("Preview shows the first 4.00 MiB of 5.00 MiB.");
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.locator("[data-save-response]").click(),
  ]);
  expect(download.suggestedFilename()).toBe("big.txt");
  const saved = await readFile((await download.path())!, "utf8");
  expect(saved.length).toBe(body.length);
});
```

Copy the Send step from `e2e/response-virtual.spec.ts` if the Send button needs the retry loop used there.

- [ ] **Step 2: Run the E2E suite**

Run: `bun run test:e2e`
Expected: PASS for the new test and all existing tests.

- [ ] **Step 3: Update the README**

Replace the paragraph that starts "Requests have a 30-second timeout and a 4 MiB response limit." with:

```markdown
Application Settings → Requests controls the timeout (default 30 s total,
10 s to connect), whether redirects are followed (default off; up to 20 hops),
and the inspection limit (default 4 MiB, up to 16 MiB). A body larger than the
inspection limit shows a truncated preview. Binary bodies show a summary in
place of the text. Use **Save response body…** to write the full body to a
file, up to 1 GiB. Stored bodies do not survive a restart. After a restart,
only complete text responses can be saved.
```

At `README.md:160`, the browser preview note says it "cannot expose manual redirect responses". Add after that sentence: "With Follow redirects on, the browser preview shows the final URL but not the hop count."

- [ ] **Step 4: Run the full verification**

Run: `bun run lint && bun run build && bun run test && (cd src-tauri && cargo test) && bun run test:e2e`
Expected: all pass.

- [ ] **Step 5: Manual desktop check**

Run: `bun run desktop`. Then:
1. Send a GET to `https://httpbin.org/image/png`. Expect the binary notice. Click **Save…**, and expect the native dialog with `png`. Save it and open the file.
2. Set Inspection limit to 1 MiB. Send `https://httpbin.org/bytes/2000000`. Expect the binary notice with size 1.91 MiB.
3. Turn on Follow redirects. Send `https://httpbin.org/redirect/3`. Expect status 200 and "→ https://httpbin.org/get · 3 redirects".
4. Set Max redirects to 2 and send again. Expect "Stopped after 2 redirects."
5. Restart the app. On the tab from step 1, expect Save to be disabled with the tooltip "Body is no longer available. Send the request again."

- [ ] **Step 6: Commit**

```bash
git add e2e/response-export.spec.ts README.md
git commit -m "test: e2e for response export; docs: request settings"
```
