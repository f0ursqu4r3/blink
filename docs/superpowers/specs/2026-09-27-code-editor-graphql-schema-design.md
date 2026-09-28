# Code editor and GraphQL schema completion

Date: 2026-09-27
Status: Approved design, awaiting spec review

## Goal

1. Syntax highlighting for request bodies in JSON and GraphQL modes.
2. In GraphQL mode, a **Fetch schema** button that introspects the endpoint so the
   query editor offers schema-aware autocomplete.

## Decisions

| Topic | Decision |
|---|---|
| Schema lifetime | In memory only, keyed by resolved URL. Lost on app restart. |
| Editor scope | JSON body, GraphQL query, GraphQL variables (JSON). Text mode keeps `<textarea>`. |
| Introspection request | POST to the request URL with the request's headers, auth (including inherited group auth), and environment interpolation. Body is the standard introspection query. |
| Editor library | CodeMirror 6. Monaco rejected: ~10x bundle size and harder to theme with Tailwind. |

## Dependencies

- `codemirror`, `@codemirror/state`, `@codemirror/view`, `@codemirror/lang-json`
- `cm6-graphql`, `graphql`

Editor modules load lazily when an editor field first renders, matching the
lazy Prettier import in `src/lib/graphql.ts`.

## Units

### `src/components/CodeEditor.vue`

CodeMirror wrapper. One purpose: edit a string with a language mode.

- Props: `modelValue: string` (via `v-model`), `language: 'json' | 'graphql'`,
  `schema?: GraphQLSchema`, `disabled?: boolean`, `placeholder?: string`,
  `id?: string`, `ariaLabelledby?: string`.
- Emits `update:modelValue` on document change.
- External `modelValue` changes replace the document only when the value differs,
  so the cursor is kept on no-op updates.
- `schema` changes reconfigure the GraphQL extension through a `Compartment`.
- `disabled` sets `EditorState.readOnly` and `EditorView.editable` to false and
  applies the same dimmed style as disabled textareas.
- Theme uses existing CSS variables (`--background`, `--foreground`,
  `--muted-foreground`, etc.) and the same font size / line height as the
  current body textarea (`text-[0.8125rem] leading-[1.75]`, `tab-2`).
- Tab inserts 2-space indent. Right-click keeps `@contextmenu.stop` behavior.
- Content element carries `data-testid` for E2E selection.

### `src/lib/graphql-schema.ts`

Fetch and cache. No UI code.

- `fetchSchema(draft: Draft, ctx?: ResolvedRequestContext): Promise<GraphQLSchema>`
  1. Copy draft with `method: 'POST'`, `bodyMode: 'graphql'`,
     `body: getIntrospectionQuery()`, `variables: ''`.
  2. `buildRequest(copy, ctx)` — applies URL, query params, headers, auth, env.
  3. `sendRequest(request)` from `src/lib/transport.ts`.
  4. Validate and `buildClientSchema(data)`.
  5. Store in cache under the built request URL.
- `getCachedSchema(url: string): CachedSchema | undefined` where
  `CachedSchema = { schema: GraphQLSchema; fetchedAt: number }`.
- Cache: module-level `Map<string, CachedSchema>`.

### `src/components/RequestEditor.vue`

- JSON body, GraphQL query, and GraphQL variables render `CodeEditor`.
  Text mode keeps the existing `<textarea>`.
- GraphQL mode shows a **Fetch schema** button in the body action row, next to
  the Body mode select, with a `HelpTooltip`: the request uses this request's
  URL, headers, and auth.
- Status line: "Schema loaded · <relative time>" or an error.
- New prop: resolved request context `ctx`, passed from `RequestWorkspace`
  (same source `useRequestRunner` uses).
- Query editor receives `schema` from the cache lookup for the current resolved URL.

## Data flow

Click Fetch schema → `fetchSchema(draft, ctx)` → cache set → `RequestEditor`
cache lookup returns schema → `CodeEditor` `schema` prop → `cm6-graphql`
reconfigures autocomplete and lint.

## Error handling

Errors show inline below the action row with `role="alert"`, like `formatError`.
The editor stays usable.

| Case | Message |
|---|---|
| `buildRequest` throws | Its existing message |
| Network error or timeout | Transport message |
| Non-2xx status | `Schema request failed: <status> <statusText>` |
| Body not JSON, or no `data.__schema` | `Endpoint did not return an introspection result. Introspection may be disabled.` |
| Response has `errors` and no `data.__schema` | First `errors[].message` |
| Response over 4 MiB | Existing transport limit message. Limit unchanged in this scope. |

## Edge cases

- Button disabled while `busy`, while a fetch runs, or when URL is empty.
- Clicks during a running fetch have no effect; button shows a spinner.
- URL change: cache lookup uses the new URL. Miss → no schema; completion stops,
  status clears. Old entries stay in memory.
- No schema: GraphQL syntax highlight and parse errors only. Schema lint
  (unknown fields) runs only when a schema is loaded.
- Fetch never changes the draft, response panel, or history.
- Body mode switch keeps current body text behavior; only the editor language changes.
- Format JSON / Format GraphQL keep writing `draft.body`; editor syncs.
- Labels: existing `sr-only` labels link to the editor via `aria-labelledby`.

## Testing

### Existing tests to update

`RequestEditorGraphql.test.ts`, `RequestEditorInheritedAuth.test.ts`,
`e2e/console.spec.ts`, `e2e/persistence.spec.ts`, `e2e/tabs.spec.ts` select body
fields as `<textarea>`.

- Unit (jsdom): stub `CodeEditor` with a textarea test double exposing the same
  props and `v-model`. Tests assert `RequestEditor` wiring, not CodeMirror.
- E2E: select editor by `data-testid` on `.cm-content`; type with
  `locator.fill()` / `keyboard.type()`.

### New tests (written first)

1. `src/lib/__test__/graphql-schema.test.ts` (mock `sendRequest`):
   - Request is POST with introspection body and the draft's headers and auth;
     draft body, method, and variables are ignored.
   - Success returns `GraphQLSchema` and fills cache for the URL.
   - One test per error table row.
2. `src/components/__test__/CodeEditor.test.ts` (minimal jsdom):
   mount creates `.cm-editor`; external `modelValue` change updates document;
   typing emits `update:modelValue`; `disabled` makes it read-only.
3. `RequestEditorGraphql.test.ts` additions: button visible only in GraphQL mode;
   disabled while busy; shows status and errors; passes `schema` to query editor.
4. `e2e/graphql-schema.spec.ts`: route endpoint to a fixture introspection result,
   click Fetch schema, type `{ vi`, assert `viewer` completion appears. Assert
   JSON body has highlight token spans.

### Verification

`bun run test`, `bun run lint`, `bun run build`, `bun run test:e2e`.

## Out of scope

- Persisting schemas across restarts.
- Separate schema URL field.
- Raising the 4 MiB response limit.
- Highlighting for text mode.
