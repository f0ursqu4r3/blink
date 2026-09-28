# Code Editor and GraphQL Schema Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Syntax-highlighted CodeMirror editors for JSON and GraphQL bodies, plus a Fetch schema button that introspects the endpoint for schema-aware GraphQL autocomplete.

**Architecture:** `src/lib/graphql-schema.ts` builds an introspection request from the draft through the existing `buildRequest` + `sendRequest` path and caches the parsed schema in memory by URL. `src/lib/code-editor.ts` owns all CodeMirror setup; `src/components/CodeEditor.vue` is a thin Vue wrapper that lazy-imports it. `RequestEditor.vue` swaps its JSON/GraphQL textareas for `CodeEditor` and adds the Fetch schema control; `RequestWorkspace.vue` passes the resolved request context down.

**Tech Stack:** Vue 3.5, TypeScript, CodeMirror 6 (`@codemirror/*`), `cm6-graphql`, `graphql`, Vitest + jsdom, Playwright, Bun.

**Spec:** `docs/superpowers/specs/2026-09-27-code-editor-graphql-schema-design.md`

## Global Constraints

- Schema lifetime: in memory only, keyed by resolved URL. Lost on app restart.
- Editor scope: JSON body, GraphQL query, GraphQL variables (JSON). Text mode keeps `<textarea>`.
- Introspection request: POST to the request URL with the request's headers, auth (including inherited group auth), and environment interpolation. Body is the standard introspection query.
- Editor modules load lazily when an editor field first renders.
- Response limit stays 4 MiB.
- Fetch never changes the draft, response panel, or history.
- Error strings (verbatim):
  - `Schema request failed: <status> <statusText>`
  - `Endpoint did not return an introspection result. Introspection may be disabled.`
- Status line format: `Schema loaded · <relative time>`.
- Package manager: `bun`. Test commands: `bun run test`, `bun run lint`, `bun run build`, `bun run test:e2e`.
- Code style: double quotes, Prettier defaults, Tailwind utility classes in templates, sparse comments.
- Deviation from spec, intentional: install explicit `@codemirror/*` packages instead of the `codemirror` meta package (smaller, only what is used).

## Review Focus

1. **Cmd/Ctrl+Enter inside the editor** — must send the request (window handler in `RequestWorkspace.vue:122`) and must not insert a blank line. CodeMirror's `defaultKeymap` binds `Mod-Enter`. Test in Task 2 and Task 5.
2. **External value change while focused** (Format, Clear body, switching request tabs) — editor must show the new text; an unchanged value must not reset the cursor. Test in Task 2.
3. **Server returns 200 with `{"errors":[…]}` and no data** (introspection disabled) — show the server message, not a crash. Test in Task 1.
4. **URL contains `{{token}}` or is invalid** — `schemaKey` must return `null` without throwing so the editor renders; Fetch shows the `buildRequest` error. Test in Task 1 and Task 4.
5. **Busy state (request in flight)** — editors read-only, Fetch disabled. Test in Task 2 and Task 4.

## File Map

| File | Action | Responsibility |
|---|---|---|
| `package.json`, `bun.lock` | Modify | New dependencies |
| `src/lib/graphql-schema.ts` | Create | Introspection request, parse, in-memory cache, age format |
| `src/lib/__test__/graphql-schema.test.ts` | Create | Unit tests for the above |
| `src/lib/code-editor.ts` | Create | CodeMirror extensions, theme, highlight, handle API |
| `src/components/CodeEditor.vue` | Create | Vue wrapper; lazy-loads `code-editor.ts` |
| `src/components/__test__/CodeEditor.test.ts` | Create | jsdom tests for the wrapper |
| `src/components/__test__/code-editor-stub.ts` | Create | Textarea test double for `RequestEditor` tests |
| `src/components/RequestEditor.vue` | Modify | Use `CodeEditor`; Fetch schema button and status |
| `src/components/RequestWorkspace.vue` | Modify | Pass `resolvedCtx` to `RequestEditor` |
| `src/components/__test__/RequestEditorGraphql.test.ts` | Modify | Mock `CodeEditor`; new Fetch schema tests |
| `src/components/__test__/RequestEditorInheritedAuth.test.ts` | Modify | Mock `CodeEditor` |
| `e2e/console.spec.ts`, `e2e/persistence.spec.ts`, `e2e/tabs.spec.ts` | Modify | `toHaveValue` → `toHaveText` for body |
| `e2e/graphql-schema.spec.ts` | Create | Fetch schema + completion + highlight E2E |

---

### Task 1: Schema fetch and cache library

**Files:**
- Modify: `package.json`, `bun.lock`
- Create: `src/lib/graphql-schema.ts`
- Test: `src/lib/__test__/graphql-schema.test.ts`

**Interfaces:**
- Consumes: `buildRequest(draft: Draft, ctx?: ResolvedRequestContext | InterpolationContext): RequestInput` (`src/lib/request.ts`), `sendRequest(request: RequestInput): Promise<ApiResponse>` (`src/lib/transport.ts`).
- Produces:
  - `type CachedSchema = { schema: GraphQLSchema; fetchedAt: number }`
  - `fetchSchema(draft: Draft, ctx?: ResolvedRequestContext): Promise<GraphQLSchema>`
  - `schemaKey(draft: Draft, ctx?: ResolvedRequestContext): string | null`
  - `getCachedSchema(key: string): CachedSchema | undefined` (reactive read)
  - `clearSchemaCache(): void`
  - `formatSchemaAge(fetchedAt: number, now: number): string`

- [ ] **Step 1: Install dependencies**

Run:
```bash
bun add graphql cm6-graphql @codemirror/state @codemirror/view @codemirror/commands @codemirror/language @codemirror/autocomplete @codemirror/lint @codemirror/lang-json @lezer/highlight
```
Expected: `package.json` lists all ten; `bun.lock` updated.

- [ ] **Step 2: Write the failing tests**

Create `src/lib/__test__/graphql-schema.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import { buildSchema, introspectionFromSchema } from "graphql";
import { createDraft, type Draft } from "../request";
import type { ResolvedRequestContext } from "../authorization";

vi.mock("../transport", () => ({ sendRequest: vi.fn() }));
import { sendRequest } from "../transport";
import {
  clearSchemaCache,
  fetchSchema,
  formatSchemaAge,
  getCachedSchema,
  schemaKey,
} from "../graphql-schema";

const send = vi.mocked(sendRequest);
const introspection = introspectionFromSchema(
  buildSchema("type Query { viewer: User } type User { id: ID! }"),
);
const url = "https://api.test/graphql";

function respond(body: string, status = 200, statusText = "OK") {
  send.mockResolvedValueOnce({
    status,
    statusText,
    durationMs: 1,
    sizeBytes: body.length,
    headers: [],
    body,
  });
}

function draft(overrides: Partial<Draft> = {}): Draft {
  return {
    ...createDraft(),
    url,
    method: "GET",
    bodyMode: "json",
    body: '{"ignored":true}',
    variables: '{"x":1}',
    headers: [{ id: 901, key: "X-Api-Key", value: "{{key}}", enabled: true }],
    ...overrides,
  };
}

const ctx: ResolvedRequestContext = {
  definitions: { key: "secret" },
  auth: { type: "bearer", token: "tok" },
};

afterEach(() => {
  send.mockReset();
  clearSchemaCache();
});

describe("fetchSchema", () => {
  it("sends a POST introspection with the draft's headers and auth", async () => {
    respond(JSON.stringify({ data: introspection }));
    await fetchSchema(draft(), ctx);
    const request = send.mock.calls[0][0];
    expect(request.method).toBe("POST");
    expect(request.url).toBe(url);
    const body = JSON.parse(request.body ?? "");
    expect(body.query).toContain("__schema");
    expect(body).not.toHaveProperty("variables");
    expect(request.headers).toContainEqual({ key: "X-Api-Key", value: "secret" });
    expect(request.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer tok",
    });
  });

  it("returns the schema and caches it by URL", async () => {
    respond(JSON.stringify({ data: introspection }));
    const schema = await fetchSchema(draft(), ctx);
    expect(schema.getQueryType()?.getFields()).toHaveProperty("viewer");
    const cached = getCachedSchema(url);
    expect(cached?.schema).toBe(schema);
    expect(typeof cached?.fetchedAt).toBe("number");
  });

  it("does not modify the draft", async () => {
    respond(JSON.stringify({ data: introspection }));
    const input = draft();
    const before = JSON.stringify(input);
    await fetchSchema(input, ctx);
    expect(JSON.stringify(input)).toBe(before);
  });

  it("rethrows buildRequest errors", async () => {
    await expect(fetchSchema(draft({ url: "not a url" }), ctx)).rejects.toThrow(
      "Enter an absolute",
    );
    expect(send).not.toHaveBeenCalled();
  });

  it("rethrows transport errors", async () => {
    send.mockRejectedValueOnce(new Error("Request timed out after 30 seconds."));
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Request timed out after 30 seconds.",
    );
  });

  it("reports non-2xx status", async () => {
    respond("nope", 401, "Unauthorized");
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Schema request failed: 401 Unauthorized",
    );
  });

  it("reports non-2xx status without status text", async () => {
    respond("nope", 500, "");
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      /^Schema request failed: 500$/,
    );
  });

  it.each([
    ["non-JSON body", "<html>"],
    ["JSON null", "null"],
    ["no __schema", JSON.stringify({ data: {} })],
    ["malformed __schema", JSON.stringify({ data: { __schema: { types: 1 } } })],
  ])("reports %s as not an introspection result", async (_, body) => {
    respond(body);
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Endpoint did not return an introspection result. Introspection may be disabled.",
    );
  });

  it("reports the first GraphQL error when there is no schema", async () => {
    respond(
      JSON.stringify({
        errors: [{ message: "Introspection is disabled" }, { message: "x" }],
      }),
    );
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Introspection is disabled",
    );
  });

  it("loads the schema when errors accompany a usable schema", async () => {
    respond(
      JSON.stringify({ data: introspection, errors: [{ message: "partial" }] }),
    );
    const schema = await fetchSchema(draft(), ctx);
    expect(schema.getQueryType()).toBeTruthy();
  });

  it("does not cache on failure", async () => {
    respond("nope", 401, "Unauthorized");
    await fetchSchema(draft(), ctx).catch(() => {});
    expect(getCachedSchema(url)).toBeUndefined();
  });
});

describe("schemaKey", () => {
  it("returns the resolved URL including query params", () => {
    const d = draft({
      url: "https://{{host}}/graphql",
      query: [{ id: 902, key: "v", value: "2", enabled: true }],
    });
    expect(
      schemaKey(d, { definitions: { host: "api.test" }, auth: { type: "none" } }),
    ).toBe("https://api.test/graphql?v=2");
  });

  it("returns null instead of throwing for an invalid URL", () => {
    expect(schemaKey(draft({ url: "" }), ctx)).toBeNull();
  });
});

describe("formatSchemaAge", () => {
  const t = 1_000_000_000;
  it.each([
    [0, "just now"],
    [59_000, "just now"],
    [60_000, "1m ago"],
    [59 * 60_000, "59m ago"],
    [60 * 60_000, "1h ago"],
    [5 * 60 * 60_000, "5h ago"],
  ])("formats %i ms as %s", (elapsed, expected) => {
    expect(formatSchemaAge(t, t + elapsed)).toBe(expected);
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `bunx vitest run src/lib/__test__/graphql-schema.test.ts`
Expected: FAIL — cannot resolve `../graphql-schema`.

- [ ] **Step 4: Write the implementation**

Create `src/lib/graphql-schema.ts`:

```ts
import { markRaw, shallowReactive } from "vue";
import {
  buildClientSchema,
  getIntrospectionQuery,
  type GraphQLSchema,
  type IntrospectionQuery,
} from "graphql";
import { buildRequest, type Draft } from "./request";
import type { ResolvedRequestContext } from "./authorization";
import { sendRequest } from "./transport";

export type CachedSchema = { schema: GraphQLSchema; fetchedAt: number };

const NOT_INTROSPECTION =
  "Endpoint did not return an introspection result. Introspection may be disabled.";

/** In memory only, keyed by resolved URL. Schemas are refetched after restart. */
const cache = shallowReactive(new Map<string, CachedSchema>());

const introspectionDraft = (draft: Draft): Draft => ({
  ...draft,
  method: "POST",
  bodyMode: "graphql",
  body: getIntrospectionQuery(),
  variables: "",
});

export function schemaKey(
  draft: Draft,
  ctx?: ResolvedRequestContext,
): string | null {
  try {
    return buildRequest(introspectionDraft(draft), ctx).url;
  } catch {
    return null;
  }
}

export const getCachedSchema = (key: string) => cache.get(key);
export const clearSchemaCache = () => cache.clear();

export async function fetchSchema(
  draft: Draft,
  ctx?: ResolvedRequestContext,
): Promise<GraphQLSchema> {
  const request = buildRequest(introspectionDraft(draft), ctx);
  const response = await sendRequest(request);
  if (response.status < 200 || response.status >= 300)
    throw new Error(
      `Schema request failed: ${response.status} ${response.statusText}`.trim(),
    );
  const schema = markRaw(parseIntrospection(response.body));
  cache.set(request.url, { schema, fetchedAt: Date.now() });
  return schema;
}

type IntrospectionPayload = {
  data?: { __schema?: unknown } | null;
  errors?: { message?: unknown }[];
} | null;

function parseIntrospection(body: string): GraphQLSchema {
  let payload: IntrospectionPayload;
  try {
    payload = JSON.parse(body);
  } catch {
    throw new Error(NOT_INTROSPECTION);
  }
  if (!payload?.data?.__schema) {
    const message = payload?.errors?.[0]?.message;
    throw new Error(typeof message === "string" ? message : NOT_INTROSPECTION);
  }
  try {
    return buildClientSchema(payload.data as unknown as IntrospectionQuery);
  } catch {
    throw new Error(NOT_INTROSPECTION);
  }
}

export function formatSchemaAge(fetchedAt: number, now: number): string {
  const minutes = Math.floor((now - fetchedAt) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  return `${Math.floor(minutes / 60)}h ago`;
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `bunx vitest run src/lib/__test__/graphql-schema.test.ts`
Expected: PASS. If the `schemaKey` query-param test fails because of URL encoding, assert with the exact output of `buildRequest` for the same draft instead — the key must equal what `fetchSchema` stores.

- [ ] **Step 6: Commit**

```bash
git add package.json bun.lock src/lib/graphql-schema.ts src/lib/__test__/graphql-schema.test.ts
git commit -m "feat: add GraphQL schema introspection and cache"
```

---

### Task 2: CodeMirror editor component

**Files:**
- Create: `src/lib/code-editor.ts`
- Create: `src/components/CodeEditor.vue`
- Test: `src/components/__test__/CodeEditor.test.ts`

**Interfaces:**
- Consumes: packages from Task 1 Step 1.
- Produces:
  - `type CodeLanguage = "json" | "graphql"` (from `@/lib/code-editor`)
  - `createCodeEditor(options: CodeEditorOptions): CodeEditorHandle`
  - `CodeEditorHandle = { view: EditorView; setDoc(value: string): void; setLanguage(language: CodeLanguage, schema?: GraphQLSchema): void; setDisabled(disabled: boolean): void; destroy(): void }`
  - `<CodeEditor v-model="…" :language :schema? :disabled? :placeholder? :id? :aria-labelledby? :test-id? />` — root is a `div`; `class` falls through to it.

- [ ] **Step 1: Write the failing tests**

Create `src/components/__test__/CodeEditor.test.ts`:

```ts
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { EditorView } from "@codemirror/view";
import CodeEditor from "../CodeEditor.vue";

beforeAll(() => {
  // jsdom lacks layout APIs CodeMirror calls during measure.
  const rect = () => ({
    x: 0, y: 0, top: 0, left: 0, bottom: 0, right: 0, width: 0, height: 0,
    toJSON: () => ({}),
  });
  Range.prototype.getBoundingClientRect = rect as never;
  Range.prototype.getClientRects = (() => ({
    length: 0, item: () => null, [Symbol.iterator]: [][Symbol.iterator],
  })) as never;
});

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => wrappers.splice(0).forEach((w) => w.unmount()));

async function mountEditor(props: Record<string, unknown> = {}) {
  const wrapper = mount(CodeEditor, {
    attachTo: document.body,
    props: { modelValue: '{"a":1}', language: "json", testId: "ed", ...props },
  });
  wrappers.push(wrapper);
  await vi.waitFor(() => expect(wrapper.find(".cm-editor").exists()).toBe(true));
  const content = wrapper.find(".cm-content").element as HTMLElement;
  return { wrapper, view: EditorView.findFromDOM(content)! };
}

describe("CodeEditor", () => {
  it("renders the model value", async () => {
    const { view } = await mountEditor();
    expect(view.state.doc.toString()).toBe('{"a":1}');
  });

  it("applies content attributes", async () => {
    const { wrapper } = await mountEditor({ id: "body", ariaLabelledby: "lbl" });
    const content = wrapper.find(".cm-content");
    expect(content.attributes("id")).toBe("body");
    expect(content.attributes("aria-labelledby")).toBe("lbl");
    expect(content.attributes("data-testid")).toBe("ed");
  });

  it("emits update:modelValue on edit", async () => {
    const { wrapper, view } = await mountEditor();
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: "[]" } });
    expect(wrapper.emitted("update:modelValue")?.at(-1)).toEqual(["[]"]);
  });

  it("replaces the document on external change without re-emitting", async () => {
    const { wrapper, view } = await mountEditor();
    await wrapper.setProps({ modelValue: '{\n  "a": 1\n}' });
    expect(view.state.doc.toString()).toBe('{\n  "a": 1\n}');
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
  });

  it("keeps the selection when the external value is unchanged", async () => {
    const { wrapper, view } = await mountEditor();
    view.dispatch({ selection: { anchor: 3 } });
    await wrapper.setProps({ modelValue: '{"a":1}' });
    expect(view.state.selection.main.head).toBe(3);
  });

  it("is read-only while disabled", async () => {
    const { wrapper, view } = await mountEditor({ disabled: true });
    expect(view.state.readOnly).toBe(true);
    expect(wrapper.find(".cm-content").attributes("contenteditable")).toBe("false");
    await wrapper.setProps({ disabled: false });
    await flushPromises();
    expect(view.state.readOnly).toBe(false);
  });

  it("does not bind Mod-Enter or Mod-l", async () => {
    const { view } = await mountEditor();
    view.dispatch({ selection: { anchor: view.state.doc.length } });
    for (const key of ["Enter", "l"]) {
      view.contentDOM.dispatchEvent(
        new KeyboardEvent("keydown", { key, metaKey: true, ctrlKey: true, bubbles: true }),
      );
    }
    expect(view.state.doc.toString()).toBe('{"a":1}');
    expect(view.state.selection.main.empty).toBe(true);
  });

  it("switches language without losing text", async () => {
    const { wrapper, view } = await mountEditor();
    await wrapper.setProps({ language: "graphql" });
    expect(view.state.doc.toString()).toBe('{"a":1}');
  });

  it("stops context menu propagation", async () => {
    const { wrapper } = await mountEditor();
    let reached = false;
    document.body.addEventListener("contextmenu", () => (reached = true), { once: true });
    wrapper.find(".cm-content").element.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true }),
    );
    expect(reached).toBe(false);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `bunx vitest run src/components/__test__/CodeEditor.test.ts`
Expected: FAIL — cannot resolve `../CodeEditor.vue`.

- [ ] **Step 3: Write `src/lib/code-editor.ts`**

```ts
import { Annotation, Compartment, EditorState } from "@codemirror/state";
import { EditorView, drawSelection, keymap, placeholder } from "@codemirror/view";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  HighlightStyle,
  bracketMatching,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
} from "@codemirror/language";
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import { json } from "@codemirror/lang-json";
import { graphql } from "cm6-graphql";
import { tags } from "@lezer/highlight";
import type { GraphQLSchema } from "graphql";

export type CodeLanguage = "json" | "graphql";

export type CodeEditorOptions = {
  parent: HTMLElement;
  doc: string;
  language: CodeLanguage;
  schema?: GraphQLSchema;
  disabled: boolean;
  placeholder?: string;
  attributes: Record<string, string>;
  onChange: (value: string) => void;
};

export type CodeEditorHandle = {
  view: EditorView;
  setDoc(value: string): void;
  setLanguage(language: CodeLanguage, schema?: GraphQLSchema): void;
  setDisabled(disabled: boolean): void;
  destroy(): void;
};

/** App shortcuts (send, focus URL) that CodeMirror must not consume. */
const reservedKeys = new Set(["Mod-Enter", "Mod-l"]);
const external = Annotation.define<boolean>();

// Mirrors the response CodeView highlight colors.
const highlight = HighlightStyle.define([
  { tag: [tags.propertyName, tags.attributeName], color: "var(--foreground)" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--success)" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--primary)" },
  {
    tag: [tags.keyword, tags.definitionKeyword, tags.operatorKeyword, tags.typeName],
    color: "oklch(0.8 0.07 240)",
  },
  { tag: [tags.comment, tags.meta], color: "var(--muted-foreground)" },
  { tag: tags.invalid, color: "var(--destructive)" },
]);

const theme = EditorView.theme(
  {
    "&": {
      flex: "1",
      minHeight: "0",
      fontSize: "0.8125rem",
      color: "var(--foreground)",
      backgroundColor: "transparent",
    },
    "&.cm-focused": { outline: "2px solid var(--ring)", outlineOffset: "-2px" },
    ".cm-scroller": {
      fontFamily: "var(--font-mono)",
      lineHeight: "1.75",
      overflow: "auto",
    },
    ".cm-content": { padding: "1rem 0", caretColor: "var(--foreground)" },
    ".cm-line": { padding: "0 1rem" },
    ".cm-placeholder": { color: "var(--muted-foreground)", opacity: "0.85" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "oklch(0.82 0.145 85 / 25%)",
    },
    ".cm-matchingBracket": { outline: "1px solid var(--border)" },
    ".cm-tooltip": {
      backgroundColor: "var(--muted)",
      border: "1px solid var(--border)",
      color: "var(--foreground)",
    },
    ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
      backgroundColor: "var(--accent)",
      color: "var(--foreground)",
    },
  },
  { dark: true },
);

const languageExtension = (language: CodeLanguage, schema?: GraphQLSchema) =>
  language === "graphql" ? graphql(schema) : json();

const editable = (enabled: boolean) => [
  EditorState.readOnly.of(!enabled),
  EditorView.editable.of(enabled),
];

export function createCodeEditor(options: CodeEditorOptions): CodeEditorHandle {
  const languageSlot = new Compartment();
  const editableSlot = new Compartment();
  const view = new EditorView({
    parent: options.parent,
    state: EditorState.create({
      doc: options.doc,
      extensions: [
        history(),
        drawSelection(),
        indentOnInput(),
        bracketMatching(),
        closeBrackets(),
        autocompletion(),
        indentUnit.of("  "),
        EditorState.tabSize.of(2),
        syntaxHighlighting(highlight),
        theme,
        options.placeholder ? placeholder(options.placeholder) : [],
        keymap.of([
          ...closeBracketsKeymap,
          ...completionKeymap,
          ...historyKeymap,
          ...defaultKeymap.filter((b) => !reservedKeys.has(b.key ?? "")),
          indentWithTab,
        ]),
        EditorView.contentAttributes.of(options.attributes),
        languageSlot.of(languageExtension(options.language, options.schema)),
        editableSlot.of(editable(!options.disabled)),
        EditorView.updateListener.of((update) => {
          if (
            update.docChanged &&
            !update.transactions.some((t) => t.annotation(external))
          )
            options.onChange(update.state.doc.toString());
        }),
      ],
    }),
  });
  return {
    view,
    setDoc(value) {
      if (view.state.doc.toString() === value) return;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
        annotations: external.of(true),
      });
    },
    setLanguage(language, schema) {
      view.dispatch({
        effects: languageSlot.reconfigure(languageExtension(language, schema)),
      });
    },
    setDisabled(disabled) {
      view.dispatch({ effects: editableSlot.reconfigure(editable(!disabled)) });
    },
    destroy: () => view.destroy(),
  };
}
```

If `cm6-graphql`'s `graphql` export has a different signature, check `node_modules/cm6-graphql/dist/index.d.ts` and use its documented entry point that takes an optional schema; keep `languageExtension` as the only call site.

- [ ] **Step 4: Write `src/components/CodeEditor.vue`**

```vue
<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { GraphQLSchema } from "graphql";
import type { CodeEditorHandle, CodeLanguage } from "@/lib/code-editor";
const model = defineModel<string>({ default: "" });
const props = defineProps<{
  language: CodeLanguage;
  schema?: GraphQLSchema;
  disabled?: boolean;
  placeholder?: string;
  id?: string;
  ariaLabelledby?: string;
  testId?: string;
}>();
const host = ref<HTMLElement>();
let handle: CodeEditorHandle | undefined;
let disposed = false;

onMounted(async () => {
  // CodeMirror loads on first use to keep startup lean.
  const { createCodeEditor } = await import("@/lib/code-editor");
  if (disposed || !host.value) return;
  const attributes: Record<string, string> = {};
  if (props.id) attributes.id = props.id;
  if (props.ariaLabelledby) attributes["aria-labelledby"] = props.ariaLabelledby;
  if (props.testId) attributes["data-testid"] = props.testId;
  handle = createCodeEditor({
    parent: host.value,
    doc: model.value,
    language: props.language,
    schema: props.schema,
    disabled: !!props.disabled,
    placeholder: props.placeholder,
    attributes,
    onChange: (value) => (model.value = value),
  });
});
onBeforeUnmount(() => {
  disposed = true;
  handle?.destroy();
});
watch(model, (value) => handle?.setDoc(value));
watch(
  () => [props.language, props.schema] as const,
  ([language, schema]) => handle?.setLanguage(language, schema),
);
watch(
  () => !!props.disabled,
  (disabled) => handle?.setDisabled(disabled),
);
</script>

<template>
  <div
    ref="host"
    class="flex min-h-0 flex-col overflow-hidden"
    :class="{ 'opacity-60': disabled }"
    @contextmenu.stop
  />
</template>
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `bunx vitest run src/components/__test__/CodeEditor.test.ts`
Expected: PASS. If a jsdom layout error appears (e.g. `getClientRects` on an element), extend the `beforeAll` stubs for that API only; do not change production code for jsdom.

- [ ] **Step 6: Commit**

```bash
git add src/lib/code-editor.ts src/components/CodeEditor.vue src/components/__test__/CodeEditor.test.ts
git commit -m "feat: add lazy CodeMirror editor component"
```

---

### Task 3: Use CodeEditor for JSON and GraphQL bodies

**Files:**
- Create: `src/components/__test__/code-editor-stub.ts`
- Modify: `src/components/RequestEditor.vue` (script imports; template lines ~296-329)
- Modify: `src/components/__test__/RequestEditorGraphql.test.ts`
- Modify: `src/components/__test__/RequestEditorInheritedAuth.test.ts`

**Interfaces:**
- Consumes: `CodeEditor` props from Task 2.
- Produces: body editor content has `data-testid="body-editor"`; variables editor content has `data-testid="variables-editor"`. Labels have ids `${id}-body-label` and `${id}-variables-label`.

- [ ] **Step 1: Create the test double**

Create `src/components/__test__/code-editor-stub.ts`:

```ts
import { defineComponent, h } from "vue";

/** Textarea stand-in for CodeEditor so RequestEditor tests avoid CodeMirror. */
export default defineComponent({
  name: "CodeEditor",
  props: {
    modelValue: { type: String, default: "" },
    language: { type: String, required: true },
    schema: { type: Object, default: undefined },
    disabled: Boolean,
    placeholder: { type: String, default: undefined },
    id: { type: String, default: undefined },
    ariaLabelledby: { type: String, default: undefined },
    testId: { type: String, default: undefined },
  },
  emits: ["update:modelValue"],
  setup(props, { emit }) {
    return () =>
      h("textarea", {
        id: props.id,
        value: props.modelValue,
        disabled: props.disabled,
        placeholder: props.placeholder,
        "aria-labelledby": props.ariaLabelledby,
        "data-testid": props.testId,
        "data-language": props.language,
        "data-schema": props.schema ? "loaded" : "none",
        onInput: (event: Event) =>
          emit("update:modelValue", (event.target as HTMLTextAreaElement).value),
      });
  },
});
```

- [ ] **Step 2: Write failing tests**

In `src/components/__test__/RequestEditorGraphql.test.ts`, add after the imports:

```ts
vi.mock("../CodeEditor.vue", () => import("./code-editor-stub"));
```

Add to the `describe` block:

```ts
  it("uses a JSON code editor for JSON bodies", () => {
    const { wrapper } = mountEditor("json");
    const body = wrapper.find("[data-testid='body-editor']");
    expect(body.attributes("data-language")).toBe("json");
    expect(body.attributes("aria-labelledby")).toMatch(/-body-label$/);
  });
  it("uses GraphQL and JSON editors in GraphQL mode", () => {
    const { wrapper } = mountEditor("graphql");
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("data-language"),
    ).toBe("graphql");
    expect(
      wrapper.find("[data-testid='variables-editor']").attributes("data-language"),
    ).toBe("json");
  });
  it("keeps a plain textarea for text bodies", async () => {
    const { wrapper, draft } = mountEditor("json");
    await wrapper.setProps({ modelValue: { ...draft, bodyMode: "text" } });
    expect(wrapper.find("[data-testid='body-editor']").exists()).toBe(false);
    expect(wrapper.find("textarea[id$='-body']").exists()).toBe(true);
  });
  it("disables the code editors while busy", async () => {
    const { wrapper } = mountEditor("graphql");
    await wrapper.setProps({ busy: true });
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("disabled"),
    ).toBeDefined();
    expect(
      wrapper.find("[data-testid='variables-editor']").attributes("disabled"),
    ).toBeDefined();
  });
  it("clears the format error when the body is edited", async () => {
    const { wrapper } = mountEditor("json");
    await wrapper.setProps({
      modelValue: { ...wrapper.props("modelValue"), body: "{" },
    });
    await wrapper.find("[data-body-actions] button").trigger("click");
    await vi.waitFor(() =>
      expect(wrapper.find("[role='alert']").exists()).toBe(true),
    );
    await wrapper.find("[data-testid='body-editor']").setValue("{}");
    expect(wrapper.find("[role='alert']").exists()).toBe(false);
  });
```

In `src/components/__test__/RequestEditorInheritedAuth.test.ts`, add after the imports (add `vi` to its vitest import if absent):

```ts
vi.mock("../CodeEditor.vue", () => import("./code-editor-stub"));
```

- [ ] **Step 3: Run tests to verify the new ones fail**

Run: `bunx vitest run src/components/__test__/RequestEditorGraphql.test.ts`
Expected: FAIL — `[data-testid='body-editor']` not found.

- [ ] **Step 4: Update `RequestEditor.vue`**

Script: add `import CodeEditor from "./CodeEditor.vue";` next to the `KeyValueEditor` import.

Template: replace the block from `<template v-if="draft.bodyMode !== 'none'">` through its closing `</template>` (the body label, body textarea, and GraphQL variables block) with:

```vue
        <template v-if="draft.bodyMode !== 'none'">
          <label
            :id="`${id}-body-label`"
            class="sr-only"
            :for="`${id}-body`"
            >{{
              draft.bodyMode === "graphql" ? "GraphQL query" : "Request body"
            }}</label
          >
          <textarea
            v-if="draft.bodyMode === 'text'"
            :id="`${id}-body`"
            v-model="draft.body"
            :disabled="busy"
            class="flex-1 min-h-45 w-full resize-none border-0 rounded-none p-4 font-mono text-[0.8125rem] leading-[1.75] bg-transparent tab-2 pointer-coarse:text-base"
            spellcheck="false"
            autocomplete="off"
            :placeholder="bodyPlaceholder"
            @input="formatError = ''"
            @contextmenu.stop
          />
          <CodeEditor
            v-else
            :id="`${id}-body`"
            :model-value="draft.body"
            :language="draft.bodyMode === 'graphql' ? 'graphql' : 'json'"
            :disabled="busy"
            :placeholder="bodyPlaceholder"
            :aria-labelledby="`${id}-body-label`"
            test-id="body-editor"
            class="flex-1 min-h-45 pointer-coarse:text-base"
            @update:model-value="setBody"
          />
          <template v-if="draft.bodyMode === 'graphql'">
            <label
              :id="`${id}-variables-label`"
              :for="`${id}-variables`"
              class="shrink-0 px-3 py-2 border-y border-border text-muted-foreground text-xs"
            >
              Variables
            </label>
            <CodeEditor
              :id="`${id}-variables`"
              :model-value="draft.variables ?? ''"
              language="json"
              :disabled="busy"
              :placeholder="variablesPlaceholder"
              :aria-labelledby="`${id}-variables-label`"
              test-id="variables-editor"
              class="h-32 shrink-0 pointer-coarse:text-base"
              @update:model-value="setVariables"
            />
          </template>
        </template>
```

Script: add next to `clearBody`:

```ts
function setBody(value: string) {
  draft.value.body = value;
  formatError.value = "";
}
function setVariables(value: string) {
  draft.value.variables = value;
  formatError.value = "";
}
```

Note: the JSON body and GraphQL query share one `CodeEditor` instance, so switching JSON ↔ GraphQL reconfigures the language and keeps the text (spec: body text behavior unchanged).

- [ ] **Step 5: Run the component tests**

Run: `bunx vitest run src/components/__test__/RequestEditorGraphql.test.ts src/components/__test__/RequestEditorInheritedAuth.test.ts`
Expected: PASS (existing `textarea[id$='-variables']` selectors still match the stub).

- [ ] **Step 6: Run the full unit suite**

Run: `bun run test`
Expected: PASS. If another suite that mounts `RequestEditor` (for example `RequestWorkspace*.test.ts`, `App.test.ts`, `InteractionFocus.test.ts`) fails with a CodeMirror/jsdom error or a missing `textarea`, add the same `vi.mock("../CodeEditor.vue", () => import("./code-editor-stub"));` line to that suite (adjust the relative path for `src/__test__/App.test.ts`: `vi.mock("@/components/CodeEditor.vue", () => import("@/components/__test__/code-editor-stub"))`).

- [ ] **Step 7: Commit**

```bash
git add src/components/RequestEditor.vue src/components/__test__/code-editor-stub.ts src/components/__test__/RequestEditorGraphql.test.ts src/components/__test__/RequestEditorInheritedAuth.test.ts
# plus any suite changed in Step 6
git commit -m "feat: highlight JSON and GraphQL request bodies"
```

---

### Task 4: Fetch schema button and status

**Files:**
- Modify: `src/components/RequestEditor.vue`
- Modify: `src/components/RequestWorkspace.vue:359-365`
- Test: `src/components/__test__/RequestEditorGraphql.test.ts`

**Interfaces:**
- Consumes: `fetchSchema`, `schemaKey`, `getCachedSchema`, `formatSchemaAge`, `clearSchemaCache` (Task 1); `CodeEditor` `schema` prop (Task 2).
- Produces: `RequestEditor` prop `ctx?: ResolvedRequestContext`. Button accessible name `Fetch schema`; status `data-testid="schema-status"`; error `data-testid="schema-error"` with `role="alert"`.

- [ ] **Step 1: Write the failing tests**

In `src/components/__test__/RequestEditorGraphql.test.ts`:

Add imports:
```ts
import { flushPromises } from "@vue/test-utils";
import { buildSchema, introspectionFromSchema } from "graphql";
import { clearSchemaCache } from "@/lib/graphql-schema";
```

Add a transport mock after the `CodeEditor` mock:
```ts
vi.mock("@/lib/transport", () => ({ sendRequest: vi.fn(), nativeTransport: false }));
import { sendRequest } from "@/lib/transport";
const send = vi.mocked(sendRequest);
const introspection = JSON.stringify({
  data: introspectionFromSchema(buildSchema("type Query { viewer: ID }")),
});
function respond(body: string, status = 200, statusText = "OK") {
  send.mockResolvedValueOnce({
    status, statusText, durationMs: 1, sizeBytes: body.length, headers: [], body,
  });
}
```

Change the `afterEach` to also reset:
```ts
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  send.mockReset();
  clearSchemaCache();
});
```

Change `mountEditor` to set a URL: `const draft = { ...createDraft(), method: "POST" as const, bodyMode, url: "https://api.test/graphql" };`

Add tests:
```ts
  const fetchButton = (w: ReturnType<typeof mount>) =>
    w.findAll("button").find((b) => b.text().includes("Fetch schema"));

  it("shows Fetch schema only in GraphQL mode", async () => {
    const { wrapper, draft } = mountEditor("json");
    expect(fetchButton(wrapper)).toBeUndefined();
    await wrapper.setProps({ modelValue: { ...draft, bodyMode: "graphql" } });
    expect(fetchButton(wrapper)).toBeDefined();
  });
  it("disables Fetch schema while busy or without a URL", async () => {
    const { wrapper, draft } = mountEditor("graphql");
    await wrapper.setProps({ busy: true });
    expect(fetchButton(wrapper)!.attributes("disabled")).toBeDefined();
    await wrapper.setProps({ busy: false, modelValue: { ...draft, url: " " } });
    expect(fetchButton(wrapper)!.attributes("disabled")).toBeDefined();
  });
  it("loads a schema and passes it to the query editor", async () => {
    const { wrapper } = mountEditor("graphql");
    const before = JSON.stringify(wrapper.props("modelValue"));
    respond(introspection);
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-testid='schema-status']").text()).toBe(
      "Schema loaded · just now",
    );
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("data-schema"),
    ).toBe("loaded");
    expect(
      wrapper.find("[data-testid='variables-editor']").attributes("data-schema"),
    ).toBe("none");
    expect(JSON.stringify(wrapper.props("modelValue"))).toBe(before);
  });
  it("ignores clicks while a fetch is running", async () => {
    const { wrapper } = mountEditor("graphql");
    let finish!: () => void;
    send.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = () =>
          resolve({
            status: 200, statusText: "OK", durationMs: 1,
            sizeBytes: introspection.length, headers: [], body: introspection,
          });
      }),
    );
    await fetchButton(wrapper)!.trigger("click");
    expect(fetchButton(wrapper)!.attributes("disabled")).toBeDefined();
    await fetchButton(wrapper)!.trigger("click");
    finish();
    await flushPromises();
    expect(send).toHaveBeenCalledTimes(1);
  });
  it("shows fetch errors and keeps the editor usable", async () => {
    const { wrapper } = mountEditor("graphql");
    respond("nope", 401, "Unauthorized");
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    const error = wrapper.find("[data-testid='schema-error']");
    expect(error.attributes("role")).toBe("alert");
    expect(error.text()).toBe("Schema request failed: 401 Unauthorized");
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("disabled"),
    ).toBeUndefined();
  });
  it("shows buildRequest errors from Fetch schema", async () => {
    const { wrapper, draft } = mountEditor("graphql");
    await wrapper.setProps({ modelValue: { ...draft, url: "not a url" } });
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-testid='schema-error']").text()).toContain(
      "Enter an absolute",
    );
    expect(send).not.toHaveBeenCalled();
  });
  it("drops the schema and error when the URL changes", async () => {
    const { wrapper } = mountEditor("graphql");
    respond(introspection);
    await fetchButton(wrapper)!.trigger("click");
    await flushPromises();
    await wrapper.setProps({
      modelValue: { ...wrapper.props("modelValue"), url: "https://other.test/graphql" },
    });
    expect(wrapper.find("[data-testid='schema-status']").exists()).toBe(false);
    expect(
      wrapper.find("[data-testid='body-editor']").attributes("data-schema"),
    ).toBe("none");
  });
  it("renders with an unresolved URL token", () => {
    const draft = {
      ...createDraft(), method: "POST" as const, bodyMode: "graphql" as const,
      url: "https://{{missing}}/graphql",
    };
    const wrapper = mount(RequestEditor, {
      attachTo: document.body,
      props: { modelValue: draft, tab: "body", busy: false },
    });
    wrappers.push(wrapper);
    expect(wrapper.find("[data-testid='body-editor']").exists()).toBe(true);
  });
```

Note: `RequestEditor` imports `formatGraphql` etc. and `RequestWorkspace` imports `nativeTransport`; the transport mock exports both names so other imports keep working.

- [ ] **Step 2: Run tests to verify they fail**

Run: `bunx vitest run src/components/__test__/RequestEditorGraphql.test.ts`
Expected: FAIL — no `Fetch schema` button.

- [ ] **Step 3: Implement in `RequestEditor.vue`**

Script changes:

```ts
import { computed, ref, useId, watch } from "vue";
import { Braces, KeyRound, LoaderCircle, Network } from "lucide-vue-next";
import type { AuthorizationConfig, ResolvedRequestContext } from "@/lib/authorization";
import {
  fetchSchema,
  formatSchemaAge,
  getCachedSchema,
  schemaKey,
} from "@/lib/graphql-schema";
```

Replace the `import type { AuthorizationConfig } …` line with the combined one above. Extend props:

```ts
const props = defineProps<{
  busy: boolean;
  effectiveAuth?: AuthorizationConfig;
  inheritedSource?: string;
  ctx?: ResolvedRequestContext;
}>();
```

Add after `bodyAllowed`:

```ts
const schemaLoading = ref(false);
const schemaError = ref("");
const currentSchemaKey = computed(() =>
  draft.value.bodyMode === "graphql" ? schemaKey(draft.value, props.ctx) : null,
);
const cachedSchema = computed(() =>
  currentSchemaKey.value ? getCachedSchema(currentSchemaKey.value) : undefined,
);
watch(currentSchemaKey, () => (schemaError.value = ""));

async function loadSchema() {
  if (props.busy || schemaLoading.value) return;
  schemaLoading.value = true;
  schemaError.value = "";
  try {
    await fetchSchema(draft.value, props.ctx);
  } catch (cause) {
    schemaError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    schemaLoading.value = false;
  }
}
```

Template, in the `[data-body-actions]` row, after the `<select>` and before the Format `<Button>`:

```vue
              <template v-if="draft.bodyMode === 'graphql'">
                <Button
                  variant="ghost"
                  :disabled="busy || schemaLoading || !draft.url.trim()"
                  @click="loadSchema"
                >
                  <LoaderCircle
                    v-if="schemaLoading"
                    :size="13"
                    class="animate-spin"
                    aria-hidden="true"
                  />
                  <Network v-else :size="13" aria-hidden="true" />Fetch schema
                </Button>
                <HelpTooltip
                  text="Fetch schema sends an introspection query with this request's URL, headers, and auth."
                >
                  <button
                    type="button"
                    class="text-[0.6875rem] text-muted-foreground underline decoration-dotted underline-offset-3"
                  >
                    Schema help
                  </button>
                </HelpTooltip>
                <span
                  v-if="cachedSchema"
                  data-testid="schema-status"
                  class="text-[0.6875rem]"
                  >Schema loaded ·
                  {{ formatSchemaAge(cachedSchema.fetchedAt, Date.now()) }}</span
                >
              </template>
```

Make sure the status text renders exactly `Schema loaded · just now` (Prettier may wrap it; the test uses `.text()`, which trims outer whitespace but keeps inner — if the test fails on whitespace, put the whole string in one mustache: `{{ `Schema loaded · ${formatSchemaAge(cachedSchema.fetchedAt, Date.now())}` }}`).

Pass the schema to the body editor — add to the body `<CodeEditor v-else …>`:

```vue
            :schema="draft.bodyMode === 'graphql' ? cachedSchema?.schema : undefined"
```

Add the error paragraph directly after the existing `formatError` paragraph:

```vue
        <p
          v-if="schemaError && draft.bodyMode === 'graphql'"
          role="alert"
          data-testid="schema-error"
          class="px-4 py-3 text-[0.6875rem] leading-[1.7] text-destructive"
        >
          {{ schemaError }}
        </p>
```

If `Network` or `LoaderCircle` does not exist in the installed `lucide-vue-next`, pick the closest existing icon (`Share2`, `Loader2`) — check with `grep -o "LoaderCircle\|Network\b" node_modules/lucide-vue-next/dist/lucide-vue-next.d.ts | sort -u`.

- [ ] **Step 4: Pass the context from `RequestWorkspace.vue`**

In the `<RequestEditor …>` element (~line 359), add:

```vue
        :ctx="resolvedCtx"
```

- [ ] **Step 5: Run tests**

Run: `bunx vitest run src/components/__test__/RequestEditorGraphql.test.ts && bun run test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/RequestEditor.vue src/components/RequestWorkspace.vue src/components/__test__/RequestEditorGraphql.test.ts
git commit -m "feat: fetch GraphQL schema for query autocomplete"
```

---

### Task 5: End-to-end tests

**Files:**
- Modify: `e2e/console.spec.ts:57`, `e2e/persistence.spec.ts:77`, `e2e/tabs.spec.ts:78`
- Create: `e2e/graphql-schema.spec.ts`

**Interfaces:**
- Consumes: labels `Request body` / `GraphQL query` (via `aria-labelledby` on `.cm-content`), `data-testid="body-editor"`, button `Fetch schema`, `data-testid="schema-status"`.

- [ ] **Step 1: Update existing E2E assertions**

CodeMirror content is a `contenteditable` div, so `toHaveValue` no longer applies. `getByLabel("Request body")` still resolves through `aria-labelledby`, and `.fill()` works on contenteditable.

`e2e/console.spec.ts` (~line 57):
```ts
  await expect(page.getByLabel("Request body", { exact: true })).toHaveText(
    /9223372036854775807/,
  );
```

`e2e/persistence.spec.ts` (~line 77):
```ts
    await expect(pane.getByLabel("Request body", { exact: true })).toHaveText(
      '{"id":9223372036854775807}',
    );
```

`e2e/tabs.spec.ts` (~line 78):
```ts
  await expect(pane.getByLabel("Request body", { exact: true })).toHaveText(
    /"depth": 2/,
  );
```

- [ ] **Step 2: Run the existing E2E suite**

Run: `bun run test:e2e`
Expected: PASS. `console.spec.ts:66` presses `Meta+Enter` inside the body editor; it must still send (Review Focus 1). If a multi-line `.fill()` in `tabs.spec.ts` produces different text because of auto-indent, replace that `.fill()` with a focus + `page.keyboard.insertText(...)` call, which bypasses keymaps.

- [ ] **Step 3: Write the new E2E test**

Create `e2e/graphql-schema.spec.ts`:

```ts
import { test, expect } from "@playwright/test";
import { buildSchema, introspectionFromSchema } from "graphql";

const introspection = JSON.stringify({
  data: introspectionFromSchema(
    buildSchema("type Query { viewer: User } type User { id: ID! }"),
  ),
});

test("fetch schema enables GraphQL completion; JSON is highlighted", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("https://graph.test/**", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: introspection,
    });
  });
  await page.goto("/");
  await page
    .getByLabel("Request URL", { exact: true })
    .fill("https://graph.test/graphql");
  await page.getByLabel("HTTP method").selectOption("POST");
  const requestTabs = page.getByRole("tablist", { name: "Request options" });
  await requestTabs.getByRole("tab", { name: "Body" }).click();
  const bodyMode = page.getByRole("combobox", { name: "Body", exact: true });

  await bodyMode.selectOption("json");
  await page.getByLabel("Request body", { exact: true }).fill('{"a": "b"}');
  await expect(
    page.locator("[data-testid='body-editor'] span[class]").first(),
  ).toBeVisible();

  await bodyMode.selectOption("graphql");
  await page.getByRole("button", { name: "Fetch schema" }).click();
  await expect(page.getByTestId("schema-status")).toHaveText(
    "Schema loaded · just now",
  );
  const query = page.getByLabel("GraphQL query", { exact: true });
  await query.fill("");
  await query.click();
  await page.keyboard.type("{ vi");
  await expect(page.locator(".cm-tooltip-autocomplete")).toContainText(
    "viewer",
  );
  expect(errors).toEqual([]);
});
```

- [ ] **Step 4: Run it**

Run: `bunx playwright test e2e/graphql-schema.spec.ts`
Expected: PASS. If `fill('{"a": "b"}')` fails because the editor switched from JSON (the `{`/`"` close-bracket handling only applies to typed keys, not fill), check the stored text with `toHaveText('{"a": "b"}')` to confirm.

- [ ] **Step 5: Commit**

```bash
git add e2e/console.spec.ts e2e/persistence.spec.ts e2e/tabs.spec.ts e2e/graphql-schema.spec.ts
git commit -m "test: cover code editor and schema completion end to end"
```

---

### Task 6: Full verification

- [ ] **Step 1: Run all checks**

```bash
bun run lint && bun run test && bun run build && bun run test:e2e
```
Expected: all PASS. If `prettier --check` fails, run `bunx prettier --write` on the changed files only and re-run.

- [ ] **Step 2: Confirm lazy loading**

Run: `bun run build` and inspect `dist/assets/`: CodeMirror code must be in a separate chunk from the main `index-*.js` (search: `grep -l "cm-editor" dist/assets/*.js` returns a file that is not the entry chunk).

- [ ] **Step 3: Commit any fix-ups**

```bash
git add <each file changed in Step 1, by explicit path>
git commit -m "chore: lint and format fix-ups"
```
Only if Step 1 required changes.
