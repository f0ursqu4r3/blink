# Drag and drop overhaul: sidebar and tabs

Date: 2026-09-28
Status: Approved design, pending implementation plan

## Goal

Make drag and drop in the request browser (sidebar) reliable, and add tab
reorder and cross-surface drag between the sidebar and the tab bar. The feel
and visuals match VS Code. All colors come from the theme tokens.

## Problems with the current implementation

`src/components/RequestBrowser.vue` uses native HTML5 drag and drop with
custom MIME types.

1. No drop feedback: no insert line, no folder highlight, no custom preview.
2. Requests can only drop *before* a request. You cannot drop after the last
   item in a list.
3. Folders can only reorder among siblings (`reorderGroup` rejects different
   parents). You cannot nest or un-nest by drag.
4. A folder dropped on "UNGROUPED" does nothing.
5. No hover-to-expand, no edge auto-scroll, no Esc to cancel.
6. In Tauri, `dragDropEnabled` defaults to on. The native file-drop handler
   can intercept HTML5 drag events, and WKWebView can strip custom MIME
   types. Drag can fail in the app but work in the browser.
7. Tabs (`src/components/RequestTabs.vue`) cannot reorder.

## Scope

- Sidebar: reorder requests, move requests into or out of folders, reorder
  folders, nest and un-nest folders, multi-select drag.
- Tabs: reorder tabs.
- Cross-surface, both directions:
  - Drag sidebar requests onto the tab bar to open them at that position.
    Requests that are already open move to that position.
  - Drag a tab onto a sidebar folder or row to move that request in the tree.

Out of scope: dragging folders onto the tab bar, file drops from the OS,
drag between windows.

## Approach

A custom pointer-events engine. It does not use the HTML5 drag API. This
avoids the Tauri and WKWebView problems and gives full control over the
preview, indicators, auto-scroll, and touch. No new dependency.

Rejected:

- Atlassian Pragmatic Drag and Drop: still uses native HTML5 drag, so the
  Tauri risk remains; the preview is less flexible.
- SortableJS / vue-draggable-plus: poor fit for nested trees and cross-list
  drops; it moves DOM nodes that Vue owns.

## Architecture

### `src/lib/drag-drop.ts` (pure, no Vue, no DOM)

- `hitZone(rect, pointer, kind)` returns `"before" | "into" | "after"`.
  - `kind: "request"`: top half is `before`, bottom half is `after`.
  - `kind: "group"`: top 25% is `before`, middle 50% is `into`, bottom 25% is
    `after`.
  - `kind: "tab"`: horizontal; left half is `before`, right half is `after`.
- `resolveTreeDrop(payload, target, zone, tree)` returns a move command or
  `null`. It returns `null` for invalid drops (a folder into itself or its
  descendant) and for no-op moves. Commands:
  - `{ type: "moveRequests", ids, groupId, beforeId }`
  - `{ type: "moveGroup", groupId, parentId, beforeGroupId }`
  - "after X" converts to "before the next sibling of the same kind", or
    `null` (end of list).
- `resolveTabDrop(payload, targetTabId, zone, openIds)` returns
  `{ type: "reorderTabs" | "openRequests", ids, beforeId }` or `null`.

### `src/composables/useDragDrop.ts` (shared drag session)

A module-level singleton, so the sidebar and the tab bar share one session.

- Sources call `startPress(event, payload, preview)` on `pointerdown`.
- Pointer lifecycle: the drag starts after 4px of movement (mouse), or after
  a 250ms long-press (`pointer: coarse`). The engine uses pointer capture.
- Payload: `{ kind: "requests", ids }` or `{ kind: "group", id }`.
- Targets register `{ el, accepts(payload), resolve(pointer) => indicator |
  null, drop(indicator) }`. On each move, the engine finds the target under
  the pointer (`document.elementFromPoint` plus the registered elements) and
  exposes a reactive `indicator`.
- Preview: an element teleported to `body` that follows the cursor with an
  8px offset.
- Edge auto-scroll: 24px zone at the edges of the scroll container of the
  current target. The speed increases near the edge.
- Hover-to-expand: 600ms on the `into` zone of a collapsed folder calls the
  target's `expand()`.
- Cancel on Esc, `pointercancel`, window `blur`, lost pointer capture, or
  payload ids that no longer exist.
- After a completed drag, suppress the next `click` on the source.

### Components

- `RequestBrowser.vue`: remove `draggable`, `@dragstart`, `@dragover`,
  `@drop`, the MIME constants, and the helper functions. Register request
  rows, folder rows, the "UNGROUPED" header, and the empty area below the
  list as targets. Rows with an inline rename form or a delete confirmation
  do not start a drag.
- `RequestTabs.vue`: register tabs as sources and targets, and register the
  strip's empty area (after the last tab) as an "append" target. New emits:
  `reorder: [ids, beforeId]`, `openRequests: [ids, beforeId]`.
- `App.vue`: wire the new emits to the state actions.

## State changes (`src/composables/useWorkspaceState.ts`)

- `moveRequests(ids, groupId, beforeId)`: no signature change.
- New `moveGroup(groupId, parentId, beforeGroupId | null)`: replaces
  `reorderGroup`. It handles reorder, nest, and un-nest. It checks
  `canNestGroup` and that the ids exist. `setGroupParent` delegates to it.
- New `reorderTabs(ids, beforeId | null)`: reorders `openIds`.
- New `openRequests(ids, beforeId | null)`: inserts ids into `openIds` at the
  position. Already-open ids move there. The first id becomes active.

Requests always list before subfolders inside a folder. A request drop in the
subfolder area goes to the end of that folder's requests.

The workspace format does not change (`sessions`, `groups`, and `openIds`
already save their order). No migration.

## Interaction and visuals

- Source rows dim to 40% opacity during the drag.
- Preview: pill with `bg-popover`, `border-border`, `shadow-md`. It shows the
  method badge and label (request), or the folder icon and name (folder).
  With more than one item, it shows a count badge ("3 requests").
- Before/after indicator: 2px `--color-primary` line, indented to the target
  level. Tabs use a vertical 2px line between tabs.
- Into indicator: the folder row gets `bg-accent` plus a 1px `primary` inset
  outline. Its visible child rows get a faint tint.
- Invalid target: no indicator, `cursor: not-allowed`.
- Folders stay open after hover-to-expand.
- Drop: FLIP transition, 150ms ease-out. Disabled by
  `prefers-reduced-motion`.
- Moved items stay selected and scroll into view.
- Cancel: the preview fades out; no state change.

### Keyboard

- Sidebar: `Alt+ArrowUp` / `Alt+ArrowDown` move the selected requests one row
  up or down within their folder.
- Tabs: `Alt+ArrowLeft` / `Alt+ArrowRight` move the active tab.
- The context menu "Move to group" stays.

## Error handling

- Every drop goes through the pure resolvers. `null` means no change.
- State actions check ids and nesting again, so a stale drop cannot corrupt
  the workspace.
- A `busy` request can move; a move does not affect the request in flight.

## Testing

1. Unit, `src/lib/__test__/drag-drop.test.ts`: `hitZone` boundaries;
   `resolveTreeDrop` for before, after, into, after the last item, a folder
   into its own descendant (reject), un-nest to root, multi-select across
   folders, no-op moves; `resolveTabDrop` for reorder and open.
2. State: `moveGroup`, `reorderTabs`, `openRequests`, and the existing
   `moveRequests` cases.
3. Component (vitest + jsdom): pointer-event sequences with stubbed
   `getBoundingClientRect`. Check emitted events, click suppression after a
   drag, Esc cancel, and keyboard shortcuts. Replace the HTML5 drop test in
   `src/components/__test__/RequestBrowser.test.ts`.
4. E2E, `e2e/drag-drop.spec.ts` (Playwright, real mouse): reorder requests,
   move into a folder, nest a folder, reorder tabs, sidebar to tab bar, tab to
   folder, persistence after reload.
