# Context Menus and Inherited Authorization Implementation Specification

<!-- markdownlint-disable MD013 -->

> **For Hermes:** Use `subagent-driven-development` to implement this plan task-by-task.

**Goal:** Add custom context menus, group settings, and inherited
authorization to Blink.

**Architecture:** Add one styled Reka UI context-menu wrapper and one
group-settings modal. Compose context-specific menu contents at each surface,
but route state changes through the current `App.vue` and
`useWorkspaceState.ts` ownership boundaries. Replace the flat authorization
fields in `Draft` with one explicit configuration that can be inherited.
Resolve authorization and token references only when Blink builds a request or
cURL command. Keep the draft text unexpanded in the editor and workspace.

**Tech stack:** Vue 3, TypeScript, Reka UI `ContextMenu*`, Vitest, Vue Test Utils, Bun.

---

## Scope and decisions

### Context menus

- Use Reka UI's installed `ContextMenuRoot`, `ContextMenuTrigger`, `ContextMenuContent`, `ContextMenuItem`, `ContextMenuSeparator`, and `ContextMenuSub` components. Do not add another menu dependency.
- Create one Blink-owned wrapper for shared menu styling, portal behavior, focus behavior, and item layout. The wrapper must not contain domain actions.
- Add a menu to each Blink application surface that has an application action. This includes the request tab strip, a request tab, Browser blank space, Ungrouped, a Browser request, a Browser group, the request bar, query/header tables and rows, the body editor, authorization editor, response body, JSON tree rows, response-header rows, cURL preview, and the response toolbar.
- Keep native browser menus for editable text controls only when Blink cannot provide the standard operation without unsafe or unsupported browser behavior. This exception is limited to text editing operations such as spelling, input methods, and browser-managed password fields. Every Blink-specific action still appears in the custom menu attached to the containing surface.
- Do not duplicate every visible button in every menu. A menu lists actions that are valid for its target. A disabled action is shown only when it explains a current restriction, such as a running request that cannot close.
- Open menus with right-click, the keyboard Context Menu key, and `Shift+F10`. Support touch/pen long press through Reka UI.
- A context menu must use the same mutation path as the existing button or keyboard command. It must not change request state directly inside a leaf component when `App.vue` or `useWorkspaceState.ts` currently owns that transition.
- Context actions must keep the target selected before they act. Right-clicking a Browser request selects that request. Right-clicking a tab selects that tab. A multi-selected Browser request retains the multi-selection when it is the target.
- Use nested menus for group destinations. Do not flatten an unbounded group tree into one menu level.

### Inherited authorization

- An authorization configuration is either `none`, `bearer`, or `basic`. The bearer configuration has `token`. The basic configuration has `username` and `password`.
- A missing authorization configuration means inherit. It is distinct from explicit `none`.
- Each `RequestGroup` has an optional local authorization configuration. A group with no local configuration inherits from its parent. A top-level group with no local configuration resolves to explicit `none`.
- Each request draft has an optional local authorization configuration. A request with no local configuration inherits from its assigned group. An ungrouped request with no local configuration resolves to explicit `none`.
- A request-local configuration overrides every group configuration. A group-local configuration overrides its parent configuration. A group configuration therefore propagates to every descendant group and request that still inherits.
- Moving a request or group changes only its location. It does not copy an inherited value into that request or group. Its effective authorization updates from the new ancestor chain.
- Creating a request or duplicating a request retains the draft's authorization mode. An inheriting draft remains inheriting. A local override remains a local override.
- Deleting a group preserves direct request and child-group authorization modes while the current promotion behavior moves them to the deleted group's parent. Effective authorization then resolves from their new ancestor chain.
- A manual enabled `Authorization` header conflicts with any resolved bearer or basic configuration, exactly as it does with the current request-local authentication. It remains valid only when the effective configuration is explicit or resolved `none`.
- Validate the effective configuration only when Blink builds a request. Do not erase incomplete draft credentials while the user edits them.
- Render the authorization editor with an `Inherit` option. It must show the effective source and type, for example `Inherited from Platform · Bearer token`, without exposing credential values.
- Render a compact inherited-authorization indicator in Browser groups and requests only when their effective configuration is not `none`. The indicator must not reveal token text, usernames, passwords, or basic-auth values.

### Group settings and token definitions

- Opening **Group settings** uses a modal. It is available from the group row's
  visible actions and its context menu.
- The modal has three sections: **General**, **Authorization**, and **Tokens**.
- **General** edits the group name and shows a read-only parent breadcrumb,
  effective authorization source, descendant request count, and descendant group
  count. Moving, nesting, collapsing, and deleting remain Browser actions, not
  modal settings.
- **Authorization** configures the group's local `Authorization` header source:
  `Inherit`, `No auth`, `Bearer`, or `Basic`. It shows the inherited effective
  type and source before a local override is saved.
- Token references interpolate in authorization credentials, the request URL,
  enabled query row names and values, arbitrary request-header values, and the
  request body. Header names remain literal and cannot contain references.
- A token reference has the form `{{name}}`. It resolves the closest definition
  in the selected group and its ancestors. A global reference uses `_.`, such
  as `{{_.accessToken}}`, and resolves only from the workspace-global token
  definitions.
- Global definitions are stored once at workspace scope. The **Tokens** section
  exposes a local group-definition list and the workspace-global list. Any group
  settings modal can edit the global list. Global names are displayed and edited
  without the `_.` reference prefix.
- Local definition names cannot begin with `_`. A local name shadows only the
  same non-global reference from an ancestor group. It never shadows
  `{{_.name}}`.
- A token definition value is plaintext local workspace data. Do not show it in
  summaries, Browser labels, menus, tooltips, or logs. It may appear only in its
  password input and in the explicitly requested cURL preview.
- A reference can appear with literal text and other references in one value.
  Resolve references once, left to right. Do not resolve references created by a
  token value. This prevents recursive expansion and makes resolution bounded.
- Resolve references after authorization inheritance and before URL, header, and
  body validation. Reject missing references, empty resolved authorization
  values, resolved bearer tokens that contain whitespace, basic usernames that
  contain `:`, and resolved header values that contain line breaks.
- Validate URLs after interpolation. Use the resolved URL and query rows for the
  request and cURL command. The visible draft URL remains unexpanded, so tab and
  Browser labels cannot expose token values.
- Validate arbitrary request-header values after interpolation. Never interpolate
  header names. This preserves header-name validation and prevents a token from
  creating a different header field.
- Preserve the resolved request body exactly. Do not parse, reformat, or log it
  while expanding references. JSON validation therefore runs on the resolved
  body, not on its template text.
- Token definitions do not apply to jq, response content, group names, labels,
  tooltips, menu items, or error output. Missing-reference errors identify only
  the reference name, never its value.
- A sent-response fingerprint includes the resolved request input and the
  selected authorization source. Changing a used token definition therefore
  marks the prior response stale without rewriting the request draft.
- The modal does not add OAuth, refresh behavior, environment variables, secret
  vaults, or encrypted storage.

### Persistence and migration

- Introduce snapshot version 3. Version 3 stores group settings, group-local
  token definitions, workspace-global token definitions, and request-level
  authorization modes.
- Decode version 1 as no groups and explicit request-local authorization, because version 1 requests already store the current flat draft credentials.
- Decode version 2 groups with no local authorization and decode each request's current flat authentication fields as a local configuration. This keeps existing requests behaviorally identical after upgrade.
- Decode version 1 and version 2 with an empty global token-definition list and
  empty group-local token-definition lists.
- Encode only version 3 after the first changed save. Keep validation strict for
  discriminated authorization objects, nullable inheritance fields, bounded
  token names and values, valid group references, and existing snapshot limits.
- Keep the existing warning accurate: credentials are locally persisted as plaintext. Do not retain the false statement in `RequestEditor.vue` that credentials only stay in memory.

## Source boundaries

| Area | Current owner | Required change |
| --- | --- | --- |
| Authorization and token resolution | `src/lib/request.ts:13-141` | Add pure resolution of inheritance, token references, header construction, and existing conflict validation. |
| Request session cloning and fingerprints | `src/lib/session.ts:3-67` | Retain explicit inheritance or local override when cloning. Compare sent and current resolved request inputs without showing secret values. |
| Request preparation and stale state | `src/composables/useRequestRunner.ts:6-54` | Build one resolved request for send, cURL, validation, and stale-response comparison. |
| Group data and deletion behavior | `src/lib/groups.ts:1-71` | Add local authorization and token-definition settings. Keep group promotion structural only. |
| Snapshot validation and migration | `src/lib/workspace.ts:5-221` | Add version 3 encoding, strict decoding, token-definition storage, and v1/v2 migration. |
| Workspace mutations | `src/composables/useWorkspaceState.ts:24-285` | Expose narrow setters for request authorization, group settings, and global definitions. Keep moves from materializing inherited values. |
| Shell actions | `src/App.vue:12-246` | Own tab, Browser, group, and global context actions. |
| Browser menus | `src/components/RequestBrowser.vue:18-515` | Add request, group, Ungrouped, and blank-space menus. |
| Group settings modal | New `src/components/GroupSettingsDialog.vue` | Edit one group's general, authorization, and token settings. |
| Request editor menu and status | `src/components/RequestEditor.vue:7-177` | Add inherited-auth UI and editor surface menus. |
| Tab menus | `src/components/RequestTabs.vue:10-124` | Add request-tab and strip menus. |
| Request workspace menus | `src/components/RequestWorkspace.vue:11-236` | Add request-bar, cURL, and body-editor menus. |
| Resolved request context | `src/App.vue:12-246` and `src/components/RequestWorkspace.vue:11-236` | Pass each request's ancestry and token scopes to preparation without copying secrets into the draft. |
| Response menus | `src/components/ResponsePanel.vue:29-361` | Add response body, header, and toolbar menus. |
| JSON tree menus | `src/components/JsonTreeView.vue:6-196` | Add a row menu without disrupting virtualization. |

## Context-menu action matrix

| Target | Actions |
| --- | --- |
| Tab strip blank space | New request. Duplicate active request. |
| Request tab | Select. Duplicate. Close. Move to group submenu. Set request authorization. |
| Browser blank space | New group. New request. |
| Ungrouped | Move selected requests here. New request here. |
| Browser request | Select. Duplicate. Close. Move to group submenu. Set request authorization. |
| Browser group | Open settings. New child group. Rename. Collapse or expand. Move selected requests here. Delete. |
| Request bar | Send. Focus URL. Copy cURL. Set request authorization. |
| Query/header blank table | Add row. Disable or enable all rows. |
| Query/header row | Enable or disable row. Duplicate row. Remove row. |
| Body editor | Format JSON when applicable. Clear body. Set body mode. |
| Authorization editor | Inherit. No auth. Bearer token. Basic auth. Clear local override. |
| cURL preview | Copy cURL. Close preview. |
| Response body | Copy displayed response. Toggle Pretty/Raw when JSON. Toggle wrapping. Open or close inspector. Clear jq result when present. |
| JSON tree row | Copy path. Copy displayed value. Collapse or expand containers. |
| Response-header row | Copy header name. Copy header value. Copy full header. |
| Response toolbar | Copy current response. Toggle view controls valid for the current tab. |

## Test-first delivery order

### Task 1: Specify authorization and token resolution

**Objective:** Add pure types and resolvers without changing persistence or UI
behavior.

**Files:**

- Modify: `src/lib/request.ts:13-141`
- Create: `src/lib/__test__/authorization.test.ts`
- Create: `src/lib/__test__/token-definitions.test.ts`

1. Write a failing test for an unconfigured request resolving to `none`.
2. Run `bun run test -- src/lib/__test__/authorization.test.ts` and verify the
   failure is caused by the missing resolver.
3. Add an `AuthorizationConfig` discriminated union and a pure resolver that
   accepts a request configuration plus ordered ancestor configurations.
4. Add failing tests for a request override, a child group override, and
   inheritance through multiple group levels.
5. Run the focused test file and verify it passes.
6. Write failing token tests for a nearest local `{{name}}` reference, an
   ancestor fallback, an exact `{{_.name}}` global reference, missing or
   malformed references, repeated references, and literal text around a
   reference.
7. Add a pure one-pass resolver for local and global definitions. It must allow
   interpolation but must not resolve a reference created by a token value.
8. Add failing request tests for references in the URL, enabled query-row names
   and values, arbitrary header values, JSON bodies, and text bodies. Add a
   negative test that header names remain literal.
9. Resolve values before URL, header-value, and body validation. Preserve the
   resolved body exactly. Validate JSON after resolution. Do not include token
   values in missing-reference errors.
10. Add failing tests that a resolved bearer or basic configuration conflicts
    with a manual `Authorization` header and that a resolved `none` does not.
11. Update `buildRequest` to receive the effective authorization configuration
    and token scopes. It must return the same resolved `RequestInput` for send
    and cURL generation.
12. Run `bun run test -- src/lib/__test__/authorization.test.ts src/lib/__test__/token-definitions.test.ts src/lib/__test__/request.test.ts` and verify it passes.

### Task 2: Add group and request authorization state

**Objective:** Make inheritance and token-definition scope explicit in live
workspace state.

**Files:**

- Modify: `src/lib/groups.ts:1-71`
- Modify: `src/lib/session.ts:3-67`
- Modify: `src/composables/useWorkspaceState.ts:24-285`
- Test: `src/lib/__test__/groups.test.ts`
- Test: `src/lib/__test__/session.test.ts`

1. Write a failing group test that a group can store no local configuration,
   explicit `none`, bearer, basic authentication, and local token definitions.
2. Run `bun run test -- src/lib/__test__/groups.test.ts` and verify the test
   fails because `RequestGroup` has no settings fields.
3. Add the nullable group authorization field and local definition list. Keep
   `createGroup` defaulting to inheritance and no definitions.
4. Write a failing session test that a duplicated draft retains its
   authorization mode without sharing references.
5. Run the two focused test files and verify the session test fails for the
   missing representation.
6. Replace the current flat draft authentication fields with one nullable local
   configuration and update `createDraft` and cloning. Keep the raw template
   draft unchanged by request preparation.
7. Add workspace-state setters for request configuration, group settings,
   group-local definitions, and workspace-global definitions. Do not write
   effective values into descendants.
8. Add a failing session-status test for a response becoming stale after a used
   token definition changes. Compare fingerprints of resolved request inputs,
   not raw template drafts.
9. Run the focused test files and verify they pass.

### Task 3: Persist and migrate inherited authorization

**Objective:** Safely round-trip version 3 workspaces and preserve existing
user credentials during migration.

**Files:**

- Modify: `src/lib/workspace.ts:5-221`
- Modify: `src/lib/__test__/workspace.test.ts`
- Modify: `README.md:68-101`

1. Write a failing snapshot test for version 3 group inheritance, a request
   override, group-local definitions, and global definitions.
2. Run `bun run test -- src/lib/__test__/workspace.test.ts` and verify the
   failure is caused by unsupported version 3 data.
3. Add strict validators for authorization union objects, nullable inheritance
   fields, local token names, global token names, and token values.
4. Encode version 3 snapshots.
5. Add failing migration tests for version 1 and version 2 snapshots that use
   bearer and basic authentication.
6. Implement migrations that preserve existing request-local authentication and
   assign no group-local or global token definitions to old workspaces.
7. Add malformed-config tests for unknown type, missing credential fields,
   wrong credential types, invalid token scope/name/value, and an invalid group
   reference.
8. Run `bun run test -- src/lib/__test__/workspace.test.ts` and verify it
   passes.
9. Update the local-state section in `README.md` with version 3 migration,
   token-definition storage, and accurate plaintext credential storage.

### Task 4: Build the shared custom context-menu primitive

**Objective:** Create one accessible and consistently styled menu wrapper from the installed Reka UI primitives.

**Files:**

- Create: `src/components/ui/context-menu/ContextMenu.vue`
- Create: `src/components/ui/context-menu/index.ts`
- Create: `src/components/ui/context-menu/__test__/ContextMenu.test.ts`

1. Write a failing component test that opens a wrapped action with right-click and invokes that action.
2. Run `bun run test -- src/components/ui/context-menu/__test__/ContextMenu.test.ts` and verify it fails because the wrapper does not exist.
3. Implement the wrapper with `ContextMenuRoot`, `ContextMenuTrigger` using `as-child`, `ContextMenuPortal`, and `ContextMenuContent`.
4. Add Blink menu styles in the wrapper. Use existing CSS variables and reduced-motion behavior. Do not add global styles.
5. Add a test that a disabled item remains visible but cannot invoke its handler.
6. Run the focused test file and verify it passes.

### Task 5: Add context menus to request and Browser entities

**Objective:** Give requests and groups the same action set through buttons, keyboard commands, and menus.

**Files:**

- Modify: `src/App.vue:46-140`
- Modify: `src/components/RequestTabs.vue:10-124`
- Modify: `src/components/RequestBrowser.vue:18-515`
- Create: `src/components/GroupSettingsDialog.vue`
- Modify: `src/components/__test__/RequestBrowser.test.ts`
- Create: `src/components/__test__/GroupSettingsDialog.test.ts`
- Modify: `src/__test__/App.test.ts`

1. Write a failing tab test for right-click selection followed by duplicate and close actions.
2. Run the focused test and verify the custom menu is absent.
3. Add tab-strip and tab menus. Emit actions through the existing `select`, `create`, and `close` event paths.
4. Write a failing Browser test for right-clicking a request, retaining multi-selection, and moving it through a nested group submenu.
5. Run the focused Browser test and verify it fails for the new menu action.
6. Add Browser menus for blank space, Ungrouped, requests, and groups. Reuse
   existing emit events for every structural operation.
7. Write a failing test that the group menu opens the modal for that exact group
   and that opening settings retains the current Browser selection.
8. Implement `GroupSettingsDialog.vue` as a controlled modal. It receives the
   selected group, its ancestor context, relevant counts, and explicit submit
   events. It does not own workspace mutation.
9. Thread the group-settings open, close, and save events through `App.vue` and
   `useWorkspaceState.ts`.
10. Run `bun run test -- src/components/__test__/RequestBrowser.test.ts src/components/__test__/GroupSettingsDialog.test.ts src/__test__/App.test.ts` and verify it passes.

### Task 6: Configure group settings and inherited authorization

**Objective:** Let users configure group authorization and tokens, then inspect
and set request inheritance without exposing secret values.

**Files:**

- Modify: `src/components/RequestEditor.vue:7-177`
- Modify: `src/components/RequestBrowser.vue:54-83`
- Modify: `src/components/RequestWorkspace.vue:11-236`
- Modify: `src/components/GroupSettingsDialog.vue`
- Modify: `src/components/__test__/GroupSettingsDialog.test.ts`
- Create: `src/components/__test__/RequestEditor.test.ts`
- Modify: `src/__test__/App.test.ts`

1. Write a failing modal test for a local token definition, an inherited token
   reference, and a global `{{_.name}}` reference. Assert that no token value is
   present in the modal summary text after the field loses focus.
2. Run `bun run test -- src/components/__test__/GroupSettingsDialog.test.ts` and
   verify it fails because the modal has no authorization or token controls.
3. Add the **General**, **Authorization**, and **Tokens** sections. Require an
   explicit save or cancel. Cancel must not mutate the group or global list.
4. Add a failing test that the modal rejects a local `_name`, a missing token
   reference, and a duplicate definition in one scope.
5. Add client-side validation for definition names and reference syntax. Keep
   final interpolation, URL, header, and body validation in the request builder.
6. Write a failing editor test for an inheriting request displaying the effective
   source and type without its token or basic-auth values.
7. Run `bun run test -- src/components/__test__/RequestEditor.test.ts` and
   verify it fails because inheritance is not rendered.
8. Add the `Inherit`, `No auth`, `Bearer token`, and `Basic auth` selector
   states. Preserve incomplete local values while editing.
9. Add a failing test that choosing `Inherit` clears only the request-local
   configuration and does not change the group configuration.
10. Add secret-safe group and request indicators to the Browser tree.
11. Update request execution so the resolver receives the current request,
    ancestor group configurations, local definitions, and workspace-global
    definitions before `buildRequest` creates HTTP headers and cURL output.
    Use that same resolved input to compute stale-response state.
12. Run `bun run test -- src/components/__test__/GroupSettingsDialog.test.ts src/components/__test__/RequestEditor.test.ts src/__test__/App.test.ts src/lib/__test__/request.test.ts` and verify it passes.

### Task 7: Add context menus to editors and response inspection

**Objective:** Finish the application-surface menu coverage without breaking virtualized rendering, editing, or response inspection.

**Files:**

- Modify: `src/components/KeyValueEditor.vue:1-89`
- Modify: `src/components/RequestEditor.vue:42-177`
- Modify: `src/components/RequestWorkspace.vue:111-236`
- Modify: `src/components/ResponsePanel.vue:170-361`
- Modify: `src/components/JsonTreeView.vue:144-196`
- Modify: `src/components/__test__/ResponsePanel.test.ts`
- Create: `src/components/__test__/KeyValueEditor.test.ts`
- Create: `src/components/__test__/JsonTreeView.test.ts`

1. Write a failing key/value editor test for duplicate, enable/disable, and remove row actions from a row context menu.
2. Run the focused test and verify the menu is absent.
3. Add table and row menus. Keep component mutations through its existing `v-model` update path.
4. Write a failing response test for copying the displayed body from its context menu and for toggling line wrap.
5. Run the focused response test and verify it fails for the new actions.
6. Add response body, response-header, cURL, and response-toolbar menus. Use the current clipboard and state functions.
7. Write a failing JSON-tree test for copying a path and toggling a container from a row menu.
8. Run the focused JSON-tree test and verify it fails for the new menu.
9. Add JSON-tree row menus without replacing virtual-row keys, row measurement, or scroll persistence.
10. Run all focused component tests and verify they pass.

### Task 8: Verify the complete feature and documentation

**Objective:** Prove no existing request, persistence, accessibility, or UI behavior regressed.

**Files:**

- Modify: `README.md:7-101`
- Modify: `DESIGN.md:28-56`
- Modify: `e2e/tabs.spec.ts`
- Create or modify: `e2e/context-menus.spec.ts`

1. Add end-to-end coverage for a request-tab context action, a nested Browser
   group action, a group-settings save and cancel path, and a response context
   action.
2. Add end-to-end coverage for a request inheriting group authorization,
   resolving local and `{{_.globalToken}}` definitions in the URL, query,
   header values, body, and authorization. Verify that changing a used
   definition marks the response stale without changing the visible draft.
3. Add end-to-end negative coverage for a missing token reference, a resolved
   header line break, and a resolved invalid JSON body. Verify no request sends.
4. Document context menus, group settings, token-reference syntax, plaintext
   credential storage, and snapshot migration in `README.md`.
5. Add the interaction rules for custom menus, group settings, token privacy,
   and inheritance to `DESIGN.md`.
6. Run `bun run lint`.
7. Run `bun run test`.
8. Run `bun run build`.
9. Run `bunx playwright install chromium` if Chromium is not already available.
10. Run `bun run test:e2e`.
11. Run `cargo test --manifest-path src-tauri/Cargo.toml`.
12. Run `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.
13. Run `git diff --check`.

## Acceptance criteria

- Right-click, Context Menu key, and `Shift+F10` open a Blink-styled menu for every scoped application surface.
- Every menu action calls the same state transition as its existing UI or keyboard equivalent.
- Every Browser group opens a controlled settings modal that provides General,
  Authorization, and Tokens sections.
- A request and every descendant group can inherit authorization from the nearest configured ancestor.
- A request or group local configuration overrides inherited authorization until the user changes it back to `Inherit`.
- `{{name}}` resolves the nearest local definition in the group ancestry.
  `{{_.name}}` resolves only from the workspace-global definitions.
- Group settings can save and delete local and global token definitions without
  exposing a definition value outside its masked input or requested cURL output.
- Token references interpolate in URLs, enabled query rows, request-header
  values, request bodies, and authorization credentials. They do not
  interpolate in header names, labels, menus, jq, or response content.
- Request validation runs after interpolation. Missing references and unsafe
  resolved values block the request without sending it or exposing a value.
- Moving, duplicating, deleting, restoring, and migrating requests and groups preserves the defined inheritance semantics.
- Tokens, passwords, usernames, and encoded basic-auth values never appear in tab labels, Browser indicators, context-menu labels, tooltips, or status summaries.
- Request send and cURL output use the same resolved effective authorization.
- The sent-response state compares resolved request inputs. Changing a used
  token definition marks the response stale while the editor retains templates.
- Existing version 1 and version 2 workspaces migrate without dropping existing
  request authentication. They start without token definitions.
- All required lint, unit, build, end-to-end, Rust test, and Rust clippy commands pass.

## Non-goals

- Add OAuth, API-key schemes, environment variables, shared credential vaults, cloud sync, or encrypted workspace storage.
- Add native application menus or a separate cross-platform menu package.
- Persist a new global request history.
