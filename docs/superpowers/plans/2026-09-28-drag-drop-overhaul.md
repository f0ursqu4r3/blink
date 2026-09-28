# Drag and Drop Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the sidebar's native HTML5 drag and drop with a pointer-events engine, and add tab reorder plus drag in both directions between the sidebar and the tab bar.

**Architecture:** Pure functions in `src/lib/drag-drop.ts` and `src/lib/groups.ts` turn (payload, target row, zone) into move commands. A module-level singleton composable `useDragDrop` owns the pointer lifecycle, preview, hit-testing of registered surfaces, auto-scroll, hover-to-expand, and FLIP animation. `RequestBrowser.vue` and `RequestTabs.vue` register one surface each and emit the existing or new state actions.

**Tech Stack:** Vue 3.5 (`<script setup>`), TypeScript 6 (`lib: ES2020, DOM` — no `DOM.Iterable`, no `Array.prototype.at`/`findLastIndex`), Tailwind 4, Vitest 5 + jsdom 30, Playwright, Bun.

**Spec:** `docs/superpowers/specs/2026-09-28-drag-drop-overhaul-design.md`

## Global Constraints

- No new runtime dependency.
- Do not use the HTML5 drag API (`draggable`, `dragstart`, `dataTransfer`) anywhere in the sidebar or tabs.
- Colors come from existing theme tokens only: `primary`, `primary-foreground`, `accent`, `secondary`, `border`, `foreground`, `muted-foreground`. There is no `popover` token.
- Drag start threshold: 4px (mouse). Touch long-press: 250ms, 8px slop.
- Hover-to-expand: 600ms. Edge auto-scroll zone: 24px, max 12px per frame.
- FLIP transition: 150ms ease-out, off under `prefers-reduced-motion: reduce`.
- Workspace format does not change. No migration.
- Match file quote style: `RequestBrowser.vue`, `useWorkspaceState.ts`, `src/lib/*` use double quotes; `RequestTabs.vue` uses single quotes. Run `bun run lint` before each commit.
- jsdom has `PointerEvent` but no `document.elementFromPoint`, `setPointerCapture`, `Element.animate`, or `window.matchMedia`. The engine must hit-test by `getBoundingClientRect` and call `animate`/`matchMedia` through optional chaining.
- The working tree has unrelated uncommitted changes. Stage only the files each task names (`git add <paths>`), never `git add -A`.

## Spec deviations (approved in this plan)

- One state action `openRequests(ids, beforeId)` covers both tab reorder and sidebar → tab bar. The spec's separate `reorderTabs` is not built.
- Hit-testing uses registered surface rectangles, not `document.elementFromPoint`.
- Stale payload ids do not cancel the drag. The resolvers and state actions filter unknown ids, so a stale drop is a no-op.
- The multi-item preview shows the label "3 requests" instead of a separate count badge.

## Review Focus

1. **Click after drag.** Releasing a drag over a row must not also fire that row's click (select / toggle). Pinned in Task 3 (engine) and Task 4 (browser).
2. **Modifier clicks and right-click.** Shift-click, Cmd-click, and right-click on a row must behave exactly as before (no drag starts under 4px, button ≠ 0 is ignored). Pinned in Task 3 (`button: 2`) and Task 4 (shift-click range test kept).
3. **Drag from inside the rename input.** Pressing in the inline rename `<input>` and selecting text must not start a drag. Pinned in Task 3.
4. **Folder into its own descendant.** Must show no indicator and emit nothing. Pinned in Task 2 and Task 4.
5. **Drop that changes nothing** (request dropped just after itself). Must not emit, so no save churn and no animation. Pinned in Task 2.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/lib/groups.ts` (modify) | Add pure ordering helpers `sessionsAfterMove`, `groupsAfterMove`. |
| `src/lib/drag-drop.ts` (create) | Pure drop geometry and resolvers: `hitZone`, `resolveTreeDrop`, `resolveTabDrop`, `stepRequests`, `stepTab`. |
| `src/composables/useWorkspaceState.ts` (modify) | `moveRequests` uses `sessionsAfterMove`; new `moveGroup`, `openRequests`; `setGroupParent` delegates; remove `reorderGroup`. |
| `src/composables/useDragDrop.ts` (create) | Singleton drag session: press → drag → drop/cancel, surfaces, auto-scroll, hover-expand, FLIP, click suppression. |
| `src/components/DragPreview.vue` (create) | Teleported preview pill that follows the cursor. |
| `src/style.css` (modify) | Global cursor and `user-select` rules while dragging. |
| `src/components/RequestBrowser.vue` (modify) | Remove HTML5 drag; register the tree surface; indicators; `Alt+↑/↓`. |
| `src/components/RequestTabs.vue` (modify) | Register the tab surface; tab drag source; indicators; `Alt+←/→`. |
| `src/App.vue` (modify) | Wire `moveGroup`, `openRequests`; mount `<DragPreview />`. |
| Tests | `src/lib/__test__/groups-order.test.ts`, `src/lib/__test__/drag-drop.test.ts`, `src/composables/__test__/useWorkspaceStateMoves.test.ts`, `src/composables/__test__/useDragDrop.test.ts`, `src/components/__test__/RequestBrowserDrag.test.ts`, `src/components/__test__/RequestTabsDrag.test.ts`, `e2e/drag-drop.spec.ts`. |

---

### Task 1: Pure ordering helpers and state actions

**Files:**
- Modify: `src/lib/groups.ts` (append after `canNestGroup`)
- Modify: `src/composables/useWorkspaceState.ts:274-330` (`moveRequests`, `reorderGroup`), `:406-410` (`setGroupParent`), return object `:425-470`
- Test: `src/lib/__test__/groups-order.test.ts` (create)
- Test: `src/composables/__test__/useWorkspaceStateMoves.test.ts` (create)

**Interfaces:**
- Produces:
  - `sessionsAfterMove<T extends GroupedSession>(sessions: T[], ids: number[], groupId: number | null, beforeId: number | null): T[]` — same objects, new order; does not assign `groupId`.
  - `groupsAfterMove(groups: RequestGroup[], groupId: number, parentId: number | null, beforeGroupId: number | null): RequestGroup[] | null` — same objects, new order; does not assign `parentId`; `null` on cycle or unknown id.
  - State: `moveGroup(groupId: number, parentId: number | null, beforeGroupId: number | null): void`, `openRequests(ids: number[], beforeId: number | null): void`. `reorderGroup` stays in this task (removed in Task 4).

- [ ] **Step 1: Write failing lib tests**

Create `src/lib/__test__/groups-order.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  groupsAfterMove,
  sessionsAfterMove,
  type RequestGroup,
} from "@/lib/groups";

const group = (id: number, parentId: number | null): RequestGroup => ({
  id,
  name: `G${id}`,
  parentId,
  collapsed: false,
});
const ids = (items: { id: number }[]) => items.map((item) => item.id);

describe("sessionsAfterMove", () => {
  const sessions = [
    { id: 1, groupId: null },
    { id: 2, groupId: 10 },
    { id: 3, groupId: null },
    { id: 4, groupId: 10 },
  ];

  it("inserts before the named session", () => {
    expect(ids(sessionsAfterMove(sessions, [4], 10, 2))).toEqual([1, 4, 2, 3]);
  });

  it("appends after the last session of the group", () => {
    expect(ids(sessionsAfterMove(sessions, [1], 10, null))).toEqual([
      2, 3, 4, 1,
    ]);
  });

  it("appends to the end for an empty group", () => {
    expect(ids(sessionsAfterMove(sessions, [1], 20, null))).toEqual([
      2, 3, 4, 1,
    ]);
  });

  it("ignores a beforeId from another group and appends", () => {
    expect(ids(sessionsAfterMove(sessions, [3], 10, 1))).toEqual([1, 2, 4, 3]);
  });

  it("keeps the source order of several moved sessions", () => {
    expect(ids(sessionsAfterMove(sessions, [4, 1], null, 3))).toEqual([
      2, 1, 4, 3,
    ]);
  });

  it("returns the same objects", () => {
    expect(sessionsAfterMove(sessions, [1], null, null)[0]).toBe(sessions[1]);
  });
});

describe("groupsAfterMove", () => {
  const groups = [group(1, null), group(2, null), group(3, 1), group(4, 1)];

  it("reorders among siblings", () => {
    expect(ids(groupsAfterMove(groups, 2, null, 1) ?? [])).toEqual([
      2, 1, 3, 4,
    ]);
  });

  it("nests at the end of the new parent's children", () => {
    expect(ids(groupsAfterMove(groups, 2, 1, null) ?? [])).toEqual([
      1, 3, 4, 2,
    ]);
  });

  it("un-nests before a root group", () => {
    expect(ids(groupsAfterMove(groups, 4, null, 1) ?? [])).toEqual([
      4, 1, 2, 3,
    ]);
  });

  it("rejects a move into a descendant", () => {
    expect(groupsAfterMove(groups, 1, 3, null)).toBeNull();
  });

  it("rejects unknown ids", () => {
    expect(groupsAfterMove(groups, 99, null, null)).toBeNull();
    expect(groupsAfterMove(groups, 2, 99, null)).toBeNull();
  });

  it("does not assign parentId", () => {
    groupsAfterMove(groups, 2, 1, null);
    expect(groups[1].parentId).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `bunx vitest run src/lib/__test__/groups-order.test.ts`
Expected: FAIL — `sessionsAfterMove` / `groupsAfterMove` are not exported.

- [ ] **Step 3: Implement helpers**

Append to `src/lib/groups.ts` after `canNestGroup`:

```ts
/**
 * Order of `sessions` after moving `ids` into `groupId`, before `beforeId`
 * or after the last session of that group. Returns the same objects; the
 * caller assigns `groupId`.
 */
export function sessionsAfterMove<T extends GroupedSession>(
  sessions: T[],
  ids: number[],
  groupId: number | null,
  beforeId: number | null,
): T[] {
  const moving = new Set(ids);
  const moved = sessions.filter((session) => moving.has(session.id));
  const remaining = sessions.filter((session) => !moving.has(session.id));
  let insertAt =
    beforeId === null
      ? -1
      : remaining.findIndex(
          (session) => session.id === beforeId && session.groupId === groupId,
        );
  if (insertAt < 0) {
    const last = remaining.reduce(
      (index, session, current) =>
        session.groupId === groupId ? current : index,
      -1,
    );
    insertAt = last < 0 ? remaining.length : last + 1;
  }
  remaining.splice(insertAt, 0, ...moved);
  return remaining;
}

/**
 * Order of `groups` after moving `groupId` under `parentId`, before
 * `beforeGroupId` or after its last new sibling. Returns the same objects;
 * the caller assigns `parentId`. Null when the move names an unknown group
 * or would nest a group inside itself.
 */
export function groupsAfterMove(
  groups: RequestGroup[],
  groupId: number,
  parentId: number | null,
  beforeGroupId: number | null,
): RequestGroup[] | null {
  const source = groups.find((group) => group.id === groupId);
  if (!source || !canNestGroup(groups, groupId, parentId)) return null;
  const remaining = groups.filter((group) => group.id !== groupId);
  let insertAt =
    beforeGroupId === null
      ? -1
      : remaining.findIndex(
          (group) => group.id === beforeGroupId && group.parentId === parentId,
        );
  if (insertAt < 0) {
    const last = remaining.reduce(
      (index, group, current) => (group.parentId === parentId ? current : index),
      -1,
    );
    insertAt = last < 0 ? remaining.length : last + 1;
  }
  remaining.splice(insertAt, 0, source);
  return remaining;
}
```

- [ ] **Step 4: Run lib tests**

Run: `bunx vitest run src/lib/__test__/groups-order.test.ts`
Expected: PASS (11 tests).

- [ ] **Step 5: Write failing state tests**

Create `src/composables/__test__/useWorkspaceStateMoves.test.ts` (same mocks as `useWorkspaceStateSetters.test.ts`):

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope, type EffectScope } from "vue";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@/lib/transport", () => ({
  nativeTransport: false,
  sendRequest: vi.fn(),
}));
vi.mock("@/lib/workspace-storage", () => ({
  readBrowserWorkspace: vi.fn(() => null),
  readWorkspace: vi.fn(() => Promise.resolve(null)),
  writeWorkspace: vi.fn(() => Promise.resolve()),
  WorkspaceWriter: class {
    save = vi.fn(() => Promise.resolve());
  },
}));

import { useWorkspaceState } from "@/composables/useWorkspaceState";
import { createSession } from "@/lib/session";

let scope: EffectScope;
let state: ReturnType<typeof useWorkspaceState>;

beforeEach(() => {
  scope = effectScope();
  scope.run(() => {
    state = useWorkspaceState();
  });
});
afterEach(() => {
  scope.stop();
  vi.restoreAllMocks();
});

function addSessions(count: number) {
  for (let index = 0; index < count; index++)
    state.sessions.value.push(createSession());
  return state.sessions.value.map((session) => session.id);
}

describe("moveRequests", () => {
  it("moves into a group and assigns groupId", () => {
    const [first, second] = addSessions(1);
    const group = state.addGroup("A", null);
    state.moveRequests([first], group!.id, null);
    expect(state.sessions.value.map((session) => session.id)).toEqual([
      second,
      first,
    ]);
    expect(state.sessions.value[1].groupId).toBe(group!.id);
  });

  it("ignores an unknown group", () => {
    const [first] = addSessions(1);
    state.moveRequests([first], 999, null);
    expect(state.sessions.value[0].groupId).toBeNull();
  });
});

describe("moveGroup", () => {
  it("nests, un-nests, and reorders", () => {
    const a = state.addGroup("A", null)!;
    const b = state.addGroup("B", null)!;
    state.moveGroup(b.id, a.id, null);
    expect(state.groups.value.find((group) => group.id === b.id)?.parentId).toBe(
      a.id,
    );
    state.moveGroup(b.id, null, a.id);
    expect(state.groups.value.map((group) => group.id)).toEqual([b.id, a.id]);
    expect(state.groups.value[0].parentId).toBeNull();
  });

  it("rejects a cycle", () => {
    const a = state.addGroup("A", null)!;
    const b = state.addGroup("B", a.id)!;
    state.moveGroup(a.id, b.id, null);
    expect(state.groups.value.find((group) => group.id === a.id)?.parentId).toBeNull();
  });

  it("setGroupParent keeps order when the parent does not change", () => {
    const a = state.addGroup("A", null)!;
    const b = state.addGroup("B", null)!;
    state.setGroupParent(a.id, null);
    expect(state.groups.value.map((group) => group.id)).toEqual([a.id, b.id]);
  });
});

describe("openRequests", () => {
  it("opens closed requests before a tab and activates the first", () => {
    const [first, second, third] = addSessions(2);
    state.openRequests([second], null);
    state.openRequests([third], first);
    expect(state.openIds.value).toEqual([third, first, second]);
    expect(state.activeId.value).toBe(third);
  });

  it("moves an open tab without duplicating it", () => {
    const [first, second] = addSessions(1);
    state.openRequests([second], null);
    state.openRequests([second], first);
    expect(state.openIds.value).toEqual([second, first]);
  });

  it("ignores unknown ids", () => {
    const before = [...state.openIds.value];
    state.openRequests([999], null);
    expect(state.openIds.value).toEqual(before);
  });
});
```

Check `addGroup`'s return value before running: read `useWorkspaceState.ts` around `function addGroup` (line ~262). It returns the created group (`return group;` at line 265). If it returns nothing in your checkout, change the test to read the group from `state.groups.value.at(-1)` equivalent `state.groups.value[state.groups.value.length - 1]`.

- [ ] **Step 6: Run to verify failure**

Run: `bunx vitest run src/composables/__test__/useWorkspaceStateMoves.test.ts`
Expected: FAIL — `state.moveGroup is not a function`, `state.openRequests is not a function`.

- [ ] **Step 7: Implement state actions**

In `src/composables/useWorkspaceState.ts`, add `sessionsAfterMove` and `groupsAfterMove` to the existing `@/lib/groups` import. Replace the body of `moveRequests`:

```ts
  function moveRequests(
    sessionIds: number[],
    groupId: number | null,
    beforeSessionId: number | null,
  ) {
    if (groupId !== null && !groups.value.some((group) => group.id === groupId))
      return;
    const ids = new Set(sessionIds);
    const moving = sessions.value.filter((session) => ids.has(session.id));
    if (!moving.length) return;
    sessions.value = sessionsAfterMove(
      sessions.value,
      sessionIds,
      groupId,
      beforeSessionId,
    );
    moving.forEach((session) => {
      session.groupId = groupId;
    });
  }
```

Add after `reorderGroup`:

```ts
  /** Reorder, nest, or un-nest a group. Rejects cycles and unknown ids. */
  function moveGroup(
    groupId: number,
    parentId: number | null,
    beforeGroupId: number | null,
  ) {
    const group = groups.value.find((candidate) => candidate.id === groupId);
    const next = groupsAfterMove(groups.value, groupId, parentId, beforeGroupId);
    if (!group || !next) return;
    group.parentId = parentId;
    groups.value = next;
  }
```

Add after `closeTab`:

```ts
  /**
   * Open requests as tabs before `beforeId`, or at the end. Open requests
   * move there. The first request becomes active.
   */
  function openRequests(ids: number[], beforeId: number | null) {
    const known = [...new Set(ids)].filter((id) =>
      sessions.value.some((session) => session.id === id),
    );
    if (!known.length) return;
    const moving = new Set(known);
    const remaining = openIds.value.filter((id) => !moving.has(id));
    const index = beforeId === null ? -1 : remaining.indexOf(beforeId);
    remaining.splice(index < 0 ? remaining.length : index, 0, ...known);
    openIds.value = remaining;
    activeId.value = known[0];
  }
```

Replace `setGroupParent`:

```ts
  function setGroupParent(groupId: number, parentId: number | null) {
    const group = groups.value.find((g) => g.id === groupId);
    if (group && group.parentId !== parentId) moveGroup(groupId, parentId, null);
  }
```

Add `moveGroup,` after `reorderGroup,` and `openRequests,` after `closeTab,` in the returned object.

- [ ] **Step 8: Run all unit tests**

Run: `bunx vitest run`
Expected: PASS, including `WorkspacePreferences.test.ts` (cycle and unknown-parent rejection in `setGroupParent`).

- [ ] **Step 9: Commit**

```bash
bun run lint
git add src/lib/groups.ts src/lib/__test__/groups-order.test.ts src/composables/useWorkspaceState.ts src/composables/__test__/useWorkspaceStateMoves.test.ts
git commit -m "feat: add moveGroup and openRequests state actions"
```

Note: `useWorkspaceState.ts` has unrelated uncommitted edits in the working tree. If the user has not committed them, stage only your hunks with `git add -p src/composables/useWorkspaceState.ts`, or ask the user first.

---

### Task 2: Drop geometry and resolvers

**Files:**
- Create: `src/lib/drag-drop.ts`
- Test: `src/lib/__test__/drag-drop.test.ts`

**Interfaces:**
- Consumes: `sessionsAfterMove`, `groupsAfterMove`, `canNestGroup`, `GroupedSession`, `RequestGroup` from `@/lib/groups`.
- Produces (all exported from `@/lib/drag-drop`):

```ts
export type DropZone = "before" | "into" | "after";
export type Point = { x: number; y: number };
export type Box = { left: number; top: number; width: number; height: number };
export type DragPayload =
  | { kind: "requests"; ids: number[] }
  | { kind: "group"; id: number };
export type TreeTarget =
  | { type: "request"; id: number }
  | { type: "group"; id: number }
  | { type: "root"; position: "start" | "end" };
export type Tree = { sessions: GroupedSession[]; groups: RequestGroup[] };
export type TreeCommand =
  | { type: "moveRequests"; ids: number[]; groupId: number | null; beforeId: number | null }
  | { type: "moveGroup"; groupId: number; parentId: number | null; beforeGroupId: number | null };
/** `key` names the row that shows the indicator: "root", "group-<id>", or "request-<id>". */
export type TreeDrop = { key: string; zone: DropZone; command: TreeCommand };
/** `key` is "tab-<id>" or "strip" when no tabs are open. */
export type TabDrop = { key: string; zone: DropZone; ids: number[]; beforeId: number | null };
export function hitZone(box: Box, point: Point, kind: "request" | "group" | "tab"): DropZone;
export function resolveTreeDrop(payload: DragPayload, target: TreeTarget, zone: DropZone, tree: Tree): TreeDrop | null;
export function resolveTabDrop(payload: DragPayload, targetId: number | null, zone: DropZone, openIds: number[]): TabDrop | null;
export function stepRequests(sessions: GroupedSession[], ids: number[], direction: -1 | 1): { groupId: number | null; beforeId: number | null } | null;
export function stepTab(openIds: number[], id: number, direction: -1 | 1): { beforeId: number | null } | null;
```

Rules the resolvers implement:
- Requests on a request row: `before`/`after` in the anchor's group. "After X" becomes "before the next session of that group that is not moving", else `null`.
- Requests on a group row: always `into` that group (end).
- Requests on root: `into` root; `start` puts them before the first ungrouped session; `end` appends. Key `"root"`.
- Group on a group row: `before`/`into`/`after`. `after` on an expanded group that has children becomes `into`. Key `group-<id>`.
- Group on a request row: `into` the request's group (key of that group, or `"root"`).
- Group on root: `start` → before the first root group; `end` → append.
- Return `null` for: empty/unknown payload, unknown target, group onto itself, a cycle, and a move that leaves the visible order unchanged.

- [ ] **Step 1: Write failing tests**

Create `src/lib/__test__/drag-drop.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  hitZone,
  resolveTabDrop,
  resolveTreeDrop,
  stepRequests,
  stepTab,
  type Tree,
} from "@/lib/drag-drop";
import type { RequestGroup } from "@/lib/groups";

const box = { left: 0, top: 100, width: 200, height: 40 };
const group = (
  id: number,
  parentId: number | null,
  collapsed = false,
): RequestGroup => ({ id, name: `G${id}`, parentId, collapsed });

// Root: requests 1, 2. Group 10 (root) holds request 3 and group 11.
// Group 11 is empty. Group 12 (root) is collapsed and empty.
const tree: Tree = {
  sessions: [
    { id: 1, groupId: null },
    { id: 2, groupId: null },
    { id: 3, groupId: 10 },
  ],
  groups: [group(10, null), group(11, 10), group(12, null, true)],
};

describe("hitZone", () => {
  it("splits request rows in half", () => {
    expect(hitZone(box, { x: 5, y: 119 }, "request")).toBe("before");
    expect(hitZone(box, { x: 5, y: 120 }, "request")).toBe("after");
  });
  it("splits group rows 25/50/25", () => {
    expect(hitZone(box, { x: 5, y: 109 }, "group")).toBe("before");
    expect(hitZone(box, { x: 5, y: 110 }, "group")).toBe("into");
    expect(hitZone(box, { x: 5, y: 130 }, "group")).toBe("into");
    expect(hitZone(box, { x: 5, y: 131 }, "group")).toBe("after");
  });
  it("splits tabs left/right", () => {
    expect(hitZone(box, { x: 99, y: 0 }, "tab")).toBe("before");
    expect(hitZone(box, { x: 100, y: 0 }, "tab")).toBe("after");
  });
});

describe("resolveTreeDrop – requests", () => {
  const requests = (...ids: number[]) => ({ kind: "requests" as const, ids });

  it("drops before a request", () => {
    expect(
      resolveTreeDrop(requests(2), { type: "request", id: 1 }, "before", tree),
    ).toEqual({
      key: "request-1",
      zone: "before",
      command: { type: "moveRequests", ids: [2], groupId: null, beforeId: 1 },
    });
  });

  it("drops after the last request of a list", () => {
    expect(
      resolveTreeDrop(requests(1), { type: "request", id: 2 }, "after", tree)
        ?.command,
    ).toEqual({ type: "moveRequests", ids: [1], groupId: null, beforeId: null });
  });

  it("returns null when the order does not change", () => {
    expect(
      resolveTreeDrop(requests(1), { type: "request", id: 1 }, "after", tree),
    ).toBeNull();
    expect(
      resolveTreeDrop(requests(1), { type: "request", id: 2 }, "before", tree),
    ).toBeNull();
  });

  it("drops into a group on any zone of the group row", () => {
    for (const zone of ["before", "into", "after"] as const)
      expect(
        resolveTreeDrop(requests(1), { type: "group", id: 11 }, zone, tree),
      ).toEqual({
        key: "group-11",
        zone: "into",
        command: { type: "moveRequests", ids: [1], groupId: 11, beforeId: null },
      });
  });

  it("moves a multi-selection across folders", () => {
    expect(
      resolveTreeDrop(requests(1, 3), { type: "request", id: 2 }, "before", tree)
        ?.command,
    ).toEqual({ type: "moveRequests", ids: [1, 3], groupId: null, beforeId: 2 });
  });

  it("drops at the start of root", () => {
    expect(
      resolveTreeDrop(requests(3), { type: "root", position: "start" }, "into", tree),
    ).toEqual({
      key: "root",
      zone: "into",
      command: { type: "moveRequests", ids: [3], groupId: null, beforeId: 1 },
    });
  });

  it("drops unknown ids as null", () => {
    expect(
      resolveTreeDrop(requests(99), { type: "group", id: 10 }, "into", tree),
    ).toBeNull();
  });
});

describe("resolveTreeDrop – groups", () => {
  const folder = (id: number) => ({ kind: "group" as const, id });

  it("nests a group into another", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "group", id: 11 }, "into", tree),
    ).toEqual({
      key: "group-11",
      zone: "into",
      command: { type: "moveGroup", groupId: 12, parentId: 11, beforeGroupId: null },
    });
  });

  it("reorders before a sibling", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "group", id: 10 }, "before", tree)
        ?.command,
    ).toEqual({ type: "moveGroup", groupId: 12, parentId: null, beforeGroupId: 10 });
  });

  it("treats after on an expanded group with children as into", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "group", id: 10 }, "after", tree),
    ).toMatchObject({ zone: "into", command: { parentId: 10 } });
  });

  it("keeps after on a collapsed group", () => {
    expect(
      resolveTreeDrop(folder(10), { type: "group", id: 12 }, "after", tree),
    ).toMatchObject({ zone: "after", command: { parentId: null, beforeGroupId: null } });
  });

  it("rejects a group into itself or its descendant", () => {
    expect(resolveTreeDrop(folder(10), { type: "group", id: 10 }, "into", tree)).toBeNull();
    expect(resolveTreeDrop(folder(10), { type: "group", id: 11 }, "into", tree)).toBeNull();
  });

  it("un-nests to root end", () => {
    expect(
      resolveTreeDrop(folder(11), { type: "root", position: "end" }, "into", tree),
    ).toEqual({
      key: "root",
      zone: "into",
      command: { type: "moveGroup", groupId: 11, parentId: null, beforeGroupId: null },
    });
  });

  it("drops onto a request row into that request's group", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "request", id: 3 }, "before", tree),
    ).toMatchObject({ key: "group-10", zone: "into", command: { parentId: 10 } });
  });

  it("returns null for a no-op", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "root", position: "end" }, "into", tree),
    ).toBeNull();
  });
});

describe("resolveTabDrop", () => {
  const requests = (...ids: number[]) => ({ kind: "requests" as const, ids });
  const open = [1, 2, 3];

  it("reorders a tab after another", () => {
    expect(resolveTabDrop(requests(1), 2, "after", open)).toEqual({
      key: "tab-2",
      zone: "after",
      ids: [1],
      beforeId: 3,
    });
  });

  it("opens a closed request before a tab", () => {
    expect(resolveTabDrop(requests(9), 1, "before", open)).toMatchObject({
      ids: [9],
      beforeId: 1,
    });
  });

  it("appends after the last tab", () => {
    expect(resolveTabDrop(requests(1), 3, "after", open)?.beforeId).toBeNull();
  });

  it("uses the strip key with no tabs", () => {
    expect(resolveTabDrop(requests(1), null, "after", [])).toEqual({
      key: "strip",
      zone: "after",
      ids: [1],
      beforeId: null,
    });
  });

  it("returns null for no-ops and groups", () => {
    expect(resolveTabDrop(requests(2), 1, "after", open)).toBeNull();
    expect(resolveTabDrop({ kind: "group", id: 1 }, 1, "after", open)).toBeNull();
  });
});

describe("stepRequests", () => {
  const sessions = [
    { id: 1, groupId: null },
    { id: 2, groupId: null },
    { id: 3, groupId: null },
  ];
  it("moves up before the previous sibling", () => {
    expect(stepRequests(sessions, [2], -1)).toEqual({ groupId: null, beforeId: 1 });
  });
  it("moves down past the next sibling", () => {
    expect(stepRequests(sessions, [1], 1)).toEqual({ groupId: null, beforeId: 3 });
    expect(stepRequests(sessions, [2], 1)).toEqual({ groupId: null, beforeId: null });
  });
  it("stops at the edges and across groups", () => {
    expect(stepRequests(sessions, [1], -1)).toBeNull();
    expect(stepRequests(sessions, [3], 1)).toBeNull();
    expect(stepRequests([...sessions, { id: 4, groupId: 5 }], [3, 4], 1)).toBeNull();
  });
});

describe("stepTab", () => {
  it("moves left and right", () => {
    expect(stepTab([1, 2, 3], 2, -1)).toEqual({ beforeId: 1 });
    expect(stepTab([1, 2, 3], 1, 1)).toEqual({ beforeId: 3 });
    expect(stepTab([1, 2, 3], 2, 1)).toEqual({ beforeId: null });
  });
  it("stops at the edges", () => {
    expect(stepTab([1, 2], 1, -1)).toBeNull();
    expect(stepTab([1, 2], 2, 1)).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `bunx vitest run src/lib/__test__/drag-drop.test.ts`
Expected: FAIL — cannot resolve `@/lib/drag-drop`.

- [ ] **Step 3: Implement**

Create `src/lib/drag-drop.ts`:

```ts
import {
  canNestGroup,
  groupsAfterMove,
  sessionsAfterMove,
  type GroupedSession,
  type RequestGroup,
} from "./groups";

export type DropZone = "before" | "into" | "after";
export type Point = { x: number; y: number };
export type Box = { left: number; top: number; width: number; height: number };
export type DragPayload =
  | { kind: "requests"; ids: number[] }
  | { kind: "group"; id: number };
export type TreeTarget =
  | { type: "request"; id: number }
  | { type: "group"; id: number }
  | { type: "root"; position: "start" | "end" };
export type Tree = { sessions: GroupedSession[]; groups: RequestGroup[] };
export type TreeCommand =
  | {
      type: "moveRequests";
      ids: number[];
      groupId: number | null;
      beforeId: number | null;
    }
  | {
      type: "moveGroup";
      groupId: number;
      parentId: number | null;
      beforeGroupId: number | null;
    };
/** `key` names the row that shows the indicator: "root", "group-<id>", or "request-<id>". */
export type TreeDrop = { key: string; zone: DropZone; command: TreeCommand };
/** `key` is "tab-<id>", or "strip" when no tabs are open. */
export type TabDrop = {
  key: string;
  zone: DropZone;
  ids: number[];
  beforeId: number | null;
};

/** Which part of a row the pointer is over. */
export function hitZone(
  box: Box,
  point: Point,
  kind: "request" | "group" | "tab",
): DropZone {
  if (kind === "tab")
    return point.x < box.left + box.width / 2 ? "before" : "after";
  const offset = (point.y - box.top) / box.height;
  if (kind === "request") return offset < 0.5 ? "before" : "after";
  return offset < 0.25 ? "before" : offset > 0.75 ? "after" : "into";
}

/** First item after `index` that is not skipped and matches. */
function nextId<T extends { id: number }>(
  items: T[],
  index: number,
  skip: Set<number>,
  match: (item: T) => boolean,
) {
  for (let current = index + 1; current < items.length; current++) {
    const item = items[current];
    if (!skip.has(item.id) && match(item)) return item.id;
  }
  return null;
}

function sameOrder(left: { id: number }[], right: { id: number }[]) {
  return (
    left.length === right.length &&
    left.every((item, index) => item.id === right[index].id)
  );
}

function groupKey(groupId: number | null) {
  return groupId === null ? "root" : `group-${groupId}`;
}

export function resolveTreeDrop(
  payload: DragPayload,
  target: TreeTarget,
  zone: DropZone,
  tree: Tree,
): TreeDrop | null {
  return payload.kind === "requests"
    ? resolveRequestsDrop(payload.ids, target, zone, tree)
    : resolveGroupDrop(payload.id, target, zone, tree);
}

function resolveRequestsDrop(
  requestIds: number[],
  target: TreeTarget,
  zone: DropZone,
  tree: Tree,
): TreeDrop | null {
  const ids = requestIds.filter((id) =>
    tree.sessions.some((session) => session.id === id),
  );
  if (!ids.length) return null;
  const moving = new Set(ids);
  let groupId: number | null = null;
  let beforeId: number | null = null;
  let key = "root";
  let effective: DropZone = "into";
  if (target.type === "root") {
    if (target.position === "start")
      beforeId = nextId(
        tree.sessions,
        -1,
        moving,
        (session) => session.groupId === null,
      );
  } else if (target.type === "group") {
    if (!tree.groups.some((group) => group.id === target.id)) return null;
    groupId = target.id;
    key = groupKey(target.id);
  } else {
    const index = tree.sessions.findIndex(
      (session) => session.id === target.id,
    );
    const anchor = tree.sessions[index];
    if (!anchor || zone === "into") return null;
    groupId = anchor.groupId;
    key = `request-${anchor.id}`;
    effective = zone;
    beforeId = nextId(
      tree.sessions,
      zone === "before" ? index - 1 : index,
      moving,
      (session) => session.groupId === groupId,
    );
  }
  const inGroup = (session: GroupedSession) => session.groupId === groupId;
  const unchanged =
    tree.sessions.every(
      (session) => !moving.has(session.id) || session.groupId === groupId,
    ) &&
    sameOrder(
      sessionsAfterMove(tree.sessions, ids, groupId, beforeId).filter(inGroup),
      tree.sessions.filter(inGroup),
    );
  if (unchanged) return null;
  return {
    key,
    zone: effective,
    command: { type: "moveRequests", ids, groupId, beforeId },
  };
}

function resolveGroupDrop(
  groupId: number,
  target: TreeTarget,
  zone: DropZone,
  tree: Tree,
): TreeDrop | null {
  const source = tree.groups.find((group) => group.id === groupId);
  if (!source) return null;
  const skip = new Set([groupId]);
  const childOf = (parentId: number | null) => (group: RequestGroup) =>
    group.parentId === parentId;
  let parentId: number | null = null;
  let beforeGroupId: number | null = null;
  let key = "root";
  let effective: DropZone = "into";
  if (target.type === "root") {
    if (target.position === "start")
      beforeGroupId = nextId(tree.groups, -1, skip, childOf(null));
  } else if (target.type === "request") {
    const anchor = tree.sessions.find((session) => session.id === target.id);
    if (!anchor) return null;
    parentId = anchor.groupId;
    key = groupKey(parentId);
  } else {
    const index = tree.groups.findIndex((group) => group.id === target.id);
    const anchor = tree.groups[index];
    if (!anchor || anchor.id === groupId) return null;
    const hasChildren =
      tree.groups.some(childOf(anchor.id)) ||
      tree.sessions.some((session) => session.groupId === anchor.id);
    effective =
      zone === "after" && !anchor.collapsed && hasChildren ? "into" : zone;
    key = groupKey(anchor.id);
    if (effective === "into") parentId = anchor.id;
    else {
      parentId = anchor.parentId;
      beforeGroupId = nextId(
        tree.groups,
        effective === "before" ? index - 1 : index,
        skip,
        childOf(parentId),
      );
    }
  }
  if (!canNestGroup(tree.groups, groupId, parentId)) return null;
  const next = groupsAfterMove(tree.groups, groupId, parentId, beforeGroupId);
  if (!next) return null;
  if (
    source.parentId === parentId &&
    sameOrder(
      next.filter(childOf(parentId)),
      tree.groups.filter(childOf(parentId)),
    )
  )
    return null;
  return {
    key,
    zone: effective,
    command: { type: "moveGroup", groupId, parentId, beforeGroupId },
  };
}

/** Drop requests on the tab bar. `targetId` null means no tabs are open. */
export function resolveTabDrop(
  payload: DragPayload,
  targetId: number | null,
  zone: DropZone,
  openIds: number[],
): TabDrop | null {
  if (payload.kind !== "requests" || !payload.ids.length) return null;
  const ids = [...new Set(payload.ids)];
  const moving = new Set(ids);
  const index = targetId === null ? openIds.length : openIds.indexOf(targetId);
  if (index < 0) return null;
  const start = zone === "before" ? index : index + 1;
  const beforeId = openIds.slice(start).find((id) => !moving.has(id)) ?? null;
  const remaining = openIds.filter((id) => !moving.has(id));
  const at = beforeId === null ? remaining.length : remaining.indexOf(beforeId);
  const next = [...remaining.slice(0, at), ...ids, ...remaining.slice(at)];
  if (
    next.length === openIds.length &&
    next.every((id, position) => id === openIds[position])
  )
    return null;
  return {
    key: targetId === null ? "strip" : `tab-${targetId}`,
    zone,
    ids,
    beforeId,
  };
}

/** Keyboard move: new place for `ids` one row up or down inside their group. */
export function stepRequests(
  sessions: GroupedSession[],
  ids: number[],
  direction: -1 | 1,
): { groupId: number | null; beforeId: number | null } | null {
  const moving = new Set(ids);
  const first = sessions.find((session) => moving.has(session.id));
  if (
    !first ||
    sessions.some(
      (session) => moving.has(session.id) && session.groupId !== first.groupId,
    )
  )
    return null;
  const groupId = first.groupId;
  const siblings = sessions.filter((session) => session.groupId === groupId);
  const positions = siblings.flatMap((session, index) =>
    moving.has(session.id) ? [index] : [],
  );
  if (direction < 0) {
    const previous = siblings[positions[0] - 1];
    return previous ? { groupId, beforeId: previous.id } : null;
  }
  const last = positions[positions.length - 1];
  if (last >= siblings.length - 1) return null;
  return { groupId, beforeId: siblings[last + 2]?.id ?? null };
}

/** Keyboard move: new place for tab `id` one step left or right. */
export function stepTab(
  openIds: number[],
  id: number,
  direction: -1 | 1,
): { beforeId: number | null } | null {
  const index = openIds.indexOf(id);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= openIds.length) return null;
  return {
    beforeId: direction < 0 ? openIds[target] : (openIds[target + 1] ?? null),
  };
}
```

- [ ] **Step 4: Run tests**

Run: `bunx vitest run src/lib/__test__/drag-drop.test.ts`
Expected: PASS. If `stepRequests(sessions, [3,4], 1)` or any case fails, fix the implementation, not the test — the tests encode the spec rules listed above.

- [ ] **Step 5: Commit**

```bash
bun run lint
git add src/lib/drag-drop.ts src/lib/__test__/drag-drop.test.ts
git commit -m "feat: add drop geometry and tree/tab drop resolvers"
```

---

### Task 3: Pointer drag engine and preview

**Files:**
- Create: `src/composables/useDragDrop.ts`
- Create: `src/components/DragPreview.vue`
- Modify: `src/style.css` (inside `@layer base { ... }`, after the `html, body, #app` rule near line 82)
- Test: `src/composables/__test__/useDragDrop.test.ts`

**Interfaces:**
- Consumes: `DragPayload`, `DropZone`, `Point` from `@/lib/drag-drop`.
- Produces:

```ts
export type DropHit = { key: string; zone: DropZone; commit: () => void; expand?: () => void };
export type DropSurface = {
  el: () => HTMLElement | null | undefined;
  axis: "x" | "y";
  /** Element that scrolls at the edges. Defaults to `el`. */
  scroller?: () => HTMLElement | null | undefined;
  resolve: (payload: DragPayload, point: Point) => DropHit | null;
};
export type DragPreview = { label: string; method?: string; folder?: boolean };
export type DragSource = { payload: () => DragPayload; preview: () => DragPreview; onStart?: () => void };
export function useDragDrop(): {
  state: Readonly<{ payload: DragPayload | null; preview: DragPreview | null; point: Point; hit: { key: string; zone: DropZone } | null }>;
  startPress(event: PointerEvent, source: DragSource): void;
  registerSurface(surface: DropSurface): () => void; // auto-unregisters on unmount when called in setup
  cancel(): void;
};
```

- [ ] **Step 1: Write failing engine tests**

Create `src/composables/__test__/useDragDrop.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDragDrop, type DropHit } from "@/composables/useDragDrop";

const drag = useDragDrop();
let surfaceEl: HTMLElement;
let unregister: () => void;
let hit: DropHit | null;
const commit = vi.fn();
const expand = vi.fn();

function rect(left: number, top: number, width: number, height: number) {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON() {},
  } as DOMRect;
}
function pointer(type: string, x: number, y: number, init: PointerEventInit = {}) {
  return new PointerEvent(type, {
    bubbles: true,
    button: 0,
    clientX: x,
    clientY: y,
    pointerType: "mouse",
    ...init,
  });
}
const move = (x: number, y: number) => window.dispatchEvent(pointer("pointermove", x, y));
const up = (x: number, y: number) => window.dispatchEvent(pointer("pointerup", x, y));
const source = {
  payload: () => ({ kind: "requests" as const, ids: [1] }),
  preview: () => ({ label: "one" }),
};

beforeEach(() => {
  surfaceEl = document.createElement("div");
  surfaceEl.getBoundingClientRect = () => rect(0, 0, 200, 400);
  document.body.append(surfaceEl);
  hit = { key: "request-2", zone: "before", commit, expand };
  unregister = drag.registerSurface({
    el: () => surfaceEl,
    axis: "y",
    resolve: () => hit,
  });
});
afterEach(() => {
  drag.cancel();
  unregister();
  surfaceEl.remove();
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("useDragDrop", () => {
  it("starts only after 4px of movement", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(102, 101);
    expect(drag.state.payload).toBeNull();
    move(100, 105);
    expect(drag.state.payload).toEqual({ kind: "requests", ids: [1] });
    expect(drag.state.preview).toEqual({ label: "one" });
    expect(drag.state.hit).toEqual({ key: "request-2", zone: "before" });
    expect(document.documentElement.hasAttribute("data-dragging")).toBe(true);
  });

  it("commits on release and swallows the next click", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    up(100, 120);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(drag.state.payload).toBeNull();
    const click = vi.fn();
    surfaceEl.addEventListener("click", click);
    surfaceEl.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(click).not.toHaveBeenCalled();
  });

  it("does not swallow a click when no drag started", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    up(100, 100);
    const click = vi.fn();
    surfaceEl.addEventListener("click", click);
    surfaceEl.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(click).toHaveBeenCalledTimes(1);
  });

  it("cancels on Escape without committing", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    up(100, 120);
    expect(commit).not.toHaveBeenCalled();
    expect(drag.state.payload).toBeNull();
  });

  it("cancels on window blur", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    window.dispatchEvent(new Event("blur"));
    expect(drag.state.payload).toBeNull();
  });

  it("ignores non-primary buttons and presses inside inputs", () => {
    drag.startPress(pointer("pointerdown", 100, 100, { button: 2 }), source);
    move(100, 120);
    expect(drag.state.payload).toBeNull();
    const input = document.createElement("input");
    surfaceEl.append(input);
    const event = pointer("pointerdown", 100, 100);
    input.addEventListener("pointerdown", (pressed) => drag.startPress(pressed as PointerEvent, source));
    input.dispatchEvent(event);
    move(100, 120);
    expect(drag.state.payload).toBeNull();
  });

  it("marks the document when no target accepts", () => {
    hit = null;
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    expect(drag.state.hit).toBeNull();
    expect(document.documentElement.hasAttribute("data-drag-invalid")).toBe(true);
  });

  it("expands a folder after hovering the into zone for 600ms", () => {
    vi.useFakeTimers();
    hit = { key: "group-1", zone: "into", commit, expand };
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    vi.advanceTimersByTime(599);
    expect(expand).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(expand).toHaveBeenCalledTimes(1);
  });

  it("starts a touch drag after a 250ms hold, and not after a swipe", () => {
    vi.useFakeTimers();
    drag.startPress(pointer("pointerdown", 100, 100, { pointerType: "touch" }), source);
    vi.advanceTimersByTime(250);
    expect(drag.state.payload).not.toBeNull();
    drag.cancel();
    drag.startPress(pointer("pointerdown", 100, 100, { pointerType: "touch" }), source);
    move(100, 115);
    vi.advanceTimersByTime(250);
    expect(drag.state.payload).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `bunx vitest run src/composables/__test__/useDragDrop.test.ts`
Expected: FAIL — cannot resolve `@/composables/useDragDrop`.

- [ ] **Step 3: Implement the engine**

Create `src/composables/useDragDrop.ts`:

```ts
import { getCurrentInstance, nextTick, onBeforeUnmount, reactive, readonly } from "vue";
import type { DragPayload, DropZone, Point } from "@/lib/drag-drop";

export type DropHit = {
  key: string;
  zone: DropZone;
  commit: () => void;
  expand?: () => void;
};
export type DropSurface = {
  el: () => HTMLElement | null | undefined;
  axis: "x" | "y";
  /** Element that scrolls at the edges. Defaults to `el`. */
  scroller?: () => HTMLElement | null | undefined;
  resolve: (payload: DragPayload, point: Point) => DropHit | null;
};
export type DragPreview = { label: string; method?: string; folder?: boolean };
export type DragSource = {
  payload: () => DragPayload;
  preview: () => DragPreview;
  onStart?: () => void;
};

const START_DISTANCE = 4;
const TOUCH_DELAY = 250;
const TOUCH_SLOP = 8;
const EXPAND_DELAY = 600;
const EDGE = 24;
const MAX_SCROLL = 12;

// One drag session for the whole app, so the sidebar and the tab bar can
// drop onto each other.
const state = reactive({
  payload: null as DragPayload | null,
  preview: null as DragPreview | null,
  point: { x: 0, y: 0 } as Point,
  hit: null as { key: string; zone: DropZone } | null,
});
const surfaces = new Set<DropSurface>();
let current: DropHit | null = null;
let press: {
  source: DragSource;
  origin: Point;
  touch: boolean;
  timer?: ReturnType<typeof setTimeout>;
} | null = null;
let expandTimer: ReturnType<typeof setTimeout> | undefined;
let scrollFrame = 0;

function inside(box: DOMRect, point: Point) {
  return (
    point.x >= box.left &&
    point.x <= box.right &&
    point.y >= box.top &&
    point.y <= box.bottom
  );
}

function surfaceAt(point: Point) {
  for (const surface of surfaces) {
    const el = surface.el();
    if (el && inside(el.getBoundingClientRect(), point)) return surface;
  }
  return null;
}

function edgeSpeed(start: number, end: number, value: number) {
  if (value < start + EDGE)
    return -Math.ceil(((start + EDGE - value) / EDGE) * MAX_SCROLL);
  if (value > end - EDGE)
    return Math.ceil(((value - end + EDGE) / EDGE) * MAX_SCROLL);
  return 0;
}

function scrollSpeed(surface: DropSurface) {
  const scroller = surface.scroller?.() ?? surface.el();
  if (!scroller) return { scroller: null, speed: 0 };
  const box = scroller.getBoundingClientRect();
  const speed =
    surface.axis === "y"
      ? edgeSpeed(box.top, box.bottom, state.point.y)
      : edgeSpeed(box.left, box.right, state.point.x);
  return { scroller, speed };
}

function scrollStep() {
  scrollFrame = 0;
  const surface = state.payload ? surfaceAt(state.point) : null;
  if (!surface) return;
  const { scroller, speed } = scrollSpeed(surface);
  if (!scroller || !speed) return;
  const axis = surface.axis === "y" ? "scrollTop" : "scrollLeft";
  const before = scroller[axis];
  scroller[axis] += speed;
  if (scroller[axis] !== before) update(state.point);
}

function update(point: Point) {
  state.point = point;
  const surface = surfaceAt(point);
  const hit =
    surface && state.payload ? surface.resolve(state.payload, point) : null;
  if (hit?.key !== current?.key || hit?.zone !== current?.zone) {
    clearTimeout(expandTimer);
    const expand = hit?.zone === "into" ? hit.expand : undefined;
    if (expand) expandTimer = setTimeout(expand, EXPAND_DELAY);
  }
  current = hit;
  state.hit = hit ? { key: hit.key, zone: hit.zone } : null;
  document.documentElement.toggleAttribute("data-drag-invalid", !hit);
  if (surface && !scrollFrame && scrollSpeed(surface).speed)
    scrollFrame = requestAnimationFrame(scrollStep);
}

function activate(point: Point) {
  if (!press) return;
  clearTimeout(press.timer);
  press.source.onStart?.();
  state.payload = press.source.payload();
  state.preview = press.source.preview();
  document.documentElement.setAttribute("data-dragging", "");
  update(point);
}

function onMove(event: PointerEvent) {
  const point = { x: event.clientX, y: event.clientY };
  if (state.payload) return update(point);
  if (!press) return;
  const distance = Math.hypot(
    point.x - press.origin.x,
    point.y - press.origin.y,
  );
  if (press.touch) {
    if (distance > TOUCH_SLOP) end();
  } else if (distance >= START_DISTANCE) activate(point);
}

function onUp() {
  const dragged = state.payload !== null;
  const hit = dragged ? current : null;
  end();
  if (dragged) suppressClick();
  if (hit) commit(hit);
}

function onKey(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  event.stopPropagation();
  end();
}

function onTouchMove(event: TouchEvent) {
  // Keep the page from scrolling under an active touch drag.
  if (state.payload && event.cancelable) event.preventDefault();
}

function listen() {
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", end);
  window.addEventListener("keydown", onKey, { capture: true });
  window.addEventListener("blur", end);
  window.addEventListener("touchmove", onTouchMove, { passive: false });
}

function unlisten() {
  window.removeEventListener("pointermove", onMove);
  window.removeEventListener("pointerup", onUp);
  window.removeEventListener("pointercancel", end);
  window.removeEventListener("keydown", onKey, { capture: true });
  window.removeEventListener("blur", end);
  window.removeEventListener("touchmove", onTouchMove);
}

function end() {
  if (press) clearTimeout(press.timer);
  press = null;
  clearTimeout(expandTimer);
  if (scrollFrame) cancelAnimationFrame(scrollFrame);
  scrollFrame = 0;
  current = null;
  state.payload = null;
  state.preview = null;
  state.hit = null;
  document.documentElement.removeAttribute("data-dragging");
  document.documentElement.removeAttribute("data-drag-invalid");
  unlisten();
}

/** The release of a drag also fires `click` on the row under it. Drop it. */
function suppressClick() {
  const block = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
  };
  window.addEventListener("click", block, { capture: true, once: true });
  setTimeout(
    () => window.removeEventListener("click", block, { capture: true }),
    0,
  );
}

function rowRects() {
  const rects = new Map<string, DOMRect>();
  for (const surface of surfaces)
    surface
      .el()
      ?.querySelectorAll<HTMLElement>("[data-drop-key]")
      .forEach((el) =>
        rects.set(el.dataset.dropKey ?? "", el.getBoundingClientRect()),
      );
  return rects;
}

/** FLIP: slide rows from their old position to the new one. */
function animateFrom(before: Map<string, DOMRect>) {
  if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
  for (const surface of surfaces)
    surface
      .el()
      ?.querySelectorAll<HTMLElement>("[data-drop-key]")
      .forEach((el) => {
        const previous = before.get(el.dataset.dropKey ?? "");
        if (!previous) return;
        const next = el.getBoundingClientRect();
        const dx = previous.left - next.left;
        const dy = previous.top - next.top;
        if (dx || dy)
          el.animate?.(
            [{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }],
            { duration: 150, easing: "ease-out" },
          );
      });
}

function commit(hit: DropHit) {
  const before = rowRects();
  hit.commit();
  void nextTick(() => animateFrom(before));
}

function startPress(event: PointerEvent, source: DragSource) {
  if (event.button !== 0 || press || state.payload) return;
  const target = event.target as Element | null;
  if (target?.closest?.("input, textarea, select, [data-no-drag]")) return;
  const touch = event.pointerType === "touch";
  press = { source, origin: { x: event.clientX, y: event.clientY }, touch };
  if (touch)
    press.timer = setTimeout(() => {
      if (press) activate(press.origin);
    }, TOUCH_DELAY);
  listen();
}

function registerSurface(surface: DropSurface) {
  surfaces.add(surface);
  const unregister = () => {
    surfaces.delete(surface);
  };
  if (getCurrentInstance()) onBeforeUnmount(unregister);
  return unregister;
}

export function useDragDrop() {
  return {
    state: readonly(state),
    startPress,
    registerSurface,
    cancel: end,
  };
}
```

Note: `unlisten()` inside `end()` removes the `pointercancel`/`blur` listeners that call `end` — calling `end` twice is safe.

- [ ] **Step 4: Run engine tests**

Run: `bunx vitest run src/composables/__test__/useDragDrop.test.ts`
Expected: PASS (9 tests).

- [ ] **Step 5: Add the preview component and CSS**

Create `src/components/DragPreview.vue`:

```vue
<script setup lang="ts">
import { Folder } from "lucide-vue-next";
import { useDragDrop } from "@/composables/useDragDrop";

const { state } = useDragDrop();
</script>

<template>
  <Teleport to="body">
    <Transition
      leave-active-class="transition-opacity duration-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="state.preview"
        class="pointer-events-none fixed left-0 top-0 z-50 flex max-w-60 items-center gap-1.5 rounded-md border border-border bg-secondary px-2 py-1 font-mono text-[10px] text-foreground shadow-md"
        :style="{
          transform: `translate(${state.point.x + 8}px, ${state.point.y + 8}px)`,
        }"
        data-drag-preview
      >
        <Folder
          v-if="state.preview.folder"
          :size="12"
          class="shrink-0"
          aria-hidden="true"
        />
        <span
          v-else-if="state.preview.method"
          class="method shrink-0 text-[8px] font-bold"
          :data-method="state.preview.method"
          >{{ state.preview.method }}</span
        >
        <span class="min-w-0 truncate">{{ state.preview.label }}</span>
      </div>
    </Transition>
  </Teleport>
</template>
```

In `src/style.css`, inside `@layer base`, after the `html, body, #app { ... }` block, add:

```css
  /* Pointer drag in progress (useDragDrop). */
  html[data-dragging],
  html[data-dragging] * {
    cursor: grabbing !important;
    user-select: none;
  }
  html[data-dragging][data-drag-invalid],
  html[data-dragging][data-drag-invalid] * {
    cursor: not-allowed !important;
  }
```

- [ ] **Step 6: Run all unit tests and type check**

Run: `bunx vitest run && bunx vue-tsc --noEmit`
Expected: PASS, no type errors.

- [ ] **Step 7: Commit**

```bash
bun run lint
git add src/composables/useDragDrop.ts src/composables/__test__/useDragDrop.test.ts src/components/DragPreview.vue src/style.css
git commit -m "feat: add pointer-events drag engine and preview"
```

---

### Task 4: Sidebar integration

**Files:**
- Modify: `src/components/RequestBrowser.vue` (script `:1-300`, template rows `:396-790`)
- Modify: `src/App.vue` (destructure `:30-60`, `<RequestBrowser>` `:376-400`, root template)
- Modify: `src/composables/useWorkspaceState.ts` (remove `reorderGroup`)
- Modify: `src/components/__test__/RequestBrowser.test.ts:68-110`
- Test: `src/components/__test__/RequestBrowserDrag.test.ts` (create)

**Interfaces:**
- Consumes: `useDragDrop`, `DropHit` (Task 3); `hitZone`, `resolveTreeDrop`, `stepRequests`, `DragPayload`, `Point`, `TreeTarget`, `TreeCommand` (Task 2); state `moveGroup` (Task 1).
- Produces: `RequestBrowser` emit `moveGroup: [groupId: number, parentId: number | null, beforeGroupId: number | null]` replaces `reorderGroup`. DOM contract used by e2e: `[data-browser-list]` on the scroll list; `data-drop-key` on rows (`root-start`, `request-<id>`, `group-<id>`, `root-end`); `[data-drop-indicator="before|after|into"]` on the indicator row.

- [ ] **Step 1: Write failing component tests**

Create `src/components/__test__/RequestBrowserDrag.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
import RequestBrowser from "@/components/RequestBrowser.vue";
import { useDragDrop } from "@/composables/useDragDrop";
import { createSession, type RequestSession } from "@/lib/session";
import type { RequestGroup } from "@/lib/groups";

// Row layout: each key maps to [top, height]; rows are 240px wide.
let layout: Record<string, [number, number]> = {};
const mounted: VueWrapper[] = [];

function rect(left: number, top: number, width: number, height: number) {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON() {},
  } as DOMRect;
}
function pointer(type: string, x: number, y: number) {
  return new PointerEvent(type, {
    bubbles: true,
    button: 0,
    clientX: x,
    clientY: y,
    pointerType: "mouse",
  });
}

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(
    function (this: Element) {
      const el = this as HTMLElement;
      if (el.hasAttribute("data-browser-list")) return rect(0, 0, 240, 600);
      const row = layout[el.dataset?.dropKey ?? ""];
      return row ? rect(0, row[0], 240, row[1]) : rect(0, 0, 0, 0);
    },
  );
});
afterEach(() => {
  useDragDrop().cancel();
  mounted.splice(0).forEach((wrapper) => wrapper.unmount());
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function setup(
  sessions: RequestSession[],
  groups: RequestGroup[],
  extra: Record<string, unknown> = {},
) {
  const handlers = {
    onMoveRequests: vi.fn(),
    onMoveGroup: vi.fn(),
    onSelect: vi.fn(),
    onToggleGroup: vi.fn(),
    onUpdateSelection: vi.fn(),
  };
  // Attach so window-level click suppression sees row clicks.
  const wrapper = mount(RequestBrowser, {
    props: { sessions, groups, activeId: null, ...handlers, ...extra },
    attachTo: document.body,
  });
  mounted.push(wrapper);
  return { wrapper, ...handlers };
}

async function dragTo(wrapper: VueWrapper, fromKey: string, y: number) {
  const from = wrapper.get(`[data-drop-key="${fromKey}"]`).element;
  const [top, height] = layout[fromKey];
  from.dispatchEvent(pointer("pointerdown", 20, top + height / 2));
  window.dispatchEvent(pointer("pointermove", 20, top + height / 2 + 10));
  window.dispatchEvent(pointer("pointermove", 20, y));
  await nextTick();
}
const release = (y: number) =>
  window.dispatchEvent(pointer("pointerup", 20, y));

describe("RequestBrowser drag and drop", () => {
  const first = createSession();
  const second = createSession();
  const platform: RequestGroup = { id: 1, name: "Platform", parentId: null, collapsed: false };

  beforeEach(() => {
    layout = {
      "root-start": [0, 28],
      [`request-${first.id}`]: [28, 27],
      [`request-${second.id}`]: [55, 27],
      "group-1": [82, 28],
      "root-end": [110, 32],
    };
  });

  it("reorders a request after the last ungrouped request", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform]);
    await dragTo(wrapper, `request-${first.id}`, 78);
    expect(
      wrapper
        .get(`[data-drop-key="request-${second.id}"] [data-drop-indicator]`)
        .attributes("data-drop-indicator"),
    ).toBe("after");
    release(78);
    expect(onMoveRequests).toHaveBeenCalledWith([first.id], null, null);
  });

  it("moves a request into a folder", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform]);
    await dragTo(wrapper, `request-${first.id}`, 96);
    expect(
      wrapper.get('[data-drop-key="group-1"]').attributes("data-drop-target"),
    ).toBe("into");
    release(96);
    expect(onMoveRequests).toHaveBeenCalledWith([first.id], 1, null);
  });

  it("drags the whole selection", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform], {
      selectedIds: [first.id, second.id],
    });
    await dragTo(wrapper, `request-${second.id}`, 96);
    release(96);
    expect(onMoveRequests).toHaveBeenCalledWith([first.id, second.id], 1, null);
  });

  it("un-nests a folder to the root", async () => {
    const child: RequestGroup = { id: 2, name: "Child", parentId: 1, collapsed: false };
    layout["group-2"] = [110, 28];
    layout["root-end"] = [138, 32];
    const { wrapper, onMoveGroup } = setup([first, second], [platform, child]);
    await dragTo(wrapper, "group-2", 150);
    release(150);
    expect(onMoveGroup).toHaveBeenCalledWith(2, null, null);
  });

  it("does not nest a folder inside its own child", async () => {
    const child: RequestGroup = { id: 2, name: "Child", parentId: 1, collapsed: false };
    layout["group-2"] = [110, 28];
    layout["root-end"] = [138, 32];
    const { wrapper, onMoveGroup } = setup([first, second], [platform, child]);
    await dragTo(wrapper, "group-1", 124);
    expect(wrapper.find("[data-drop-target]").exists()).toBe(false);
    release(124);
    expect(onMoveGroup).not.toHaveBeenCalled();
  });

  it("does not select the row under the pointer after a drop", async () => {
    const { wrapper, onSelect } = setup([first, second], [platform]);
    await dragTo(wrapper, `request-${first.id}`, 78);
    release(78);
    await wrapper.get(`[data-drop-key="request-${second.id}"]`).trigger("click");
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("keeps shift-click range selection", async () => {
    const { wrapper, onUpdateSelection } = setup([first, second], [platform], {
      selectedIds: [first.id],
      selectionAnchorId: first.id,
    });
    await wrapper
      .get(`[data-drop-key="request-${second.id}"]`)
      .trigger("click", { shiftKey: true });
    expect(onUpdateSelection).toHaveBeenCalledWith([first.id, second.id], first.id);
  });

  it("expands a collapsed folder after a hover", async () => {
    vi.useFakeTimers();
    const { wrapper, onToggleGroup } = setup([first, second], [
      { ...platform, collapsed: true },
    ]);
    await dragTo(wrapper, `request-${first.id}`, 96);
    vi.advanceTimersByTime(600);
    expect(onToggleGroup).toHaveBeenCalledWith(1);
  });

  it("moves the selection down with Alt+ArrowDown", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform]);
    await wrapper
      .get(`[data-drop-key="request-${first.id}"]`)
      .trigger("keydown", { key: "ArrowDown", altKey: true });
    expect(onMoveRequests).toHaveBeenCalledWith([first.id], null, null);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `bunx vitest run src/components/__test__/RequestBrowserDrag.test.ts`
Expected: FAIL — no `[data-drop-key]` elements.

- [ ] **Step 3: Remove the HTML5 drag code**

In `src/components/RequestBrowser.vue` script:
1. Delete `const requestMime = ...` and `const groupMime = ...`.
2. Delete the functions `requestIdsFrom`, `hasType`, `startRequestDrag`, `allowDrop`, `dropOnRequest`, `dropOnGroup`, `dropOnUngrouped`, `startGroupDrag`.
3. In `defineEmits`, replace `reorderGroup: [groupId: number, beforeGroupId: number];` with:

```ts
  moveGroup: [
    groupId: number,
    parentId: number | null,
    beforeGroupId: number | null,
  ];
```

In the template: remove every `draggable="true"`, `@dragstart=...`, `@dragover="allowDrop"`, `@drop=...`, and the classes `cursor-grab` and `[-webkit-user-drag:element]`.

- [ ] **Step 4: Add the drag wiring to the script**

Change the Vue import to `import { computed, nextTick, ref } from "vue";` and add imports:

```ts
import { useDragDrop, type DropHit } from "@/composables/useDragDrop";
import {
  hitZone,
  resolveTreeDrop,
  stepRequests,
  type DragPayload,
  type Point,
  type TreeCommand,
  type TreeTarget,
} from "@/lib/drag-drop";
```

Replace `levelPadding` with:

```ts
function indent(level: number) {
  const pxMap = [12, 28, 44, 60, 76, 92, 108];
  return pxMap[Math.min(level, 6)];
}
function levelPadding(level: number): string {
  return `padding-left: ${indent(level)}px`;
}
function rowKey(row: BrowserRow) {
  return row.type === "group"
    ? `group-${row.group.id}`
    : `request-${row.session.id}`;
}
```

Change the `v-for` `:key` expression to `:key="rowKey(row)"`.

Add after `handleRequestContextMenu`:

```ts
const listEl = ref<HTMLElement>();
const drag = useDragDrop();
drag.registerSurface({
  el: () => listEl.value,
  axis: "y",
  resolve: resolveBrowserDrop,
});

function treeTarget(key: string): TreeTarget | null {
  const [type, value] = key.split("-");
  if (type === "root")
    return { type: "root", position: value === "start" ? "start" : "end" };
  const id = Number(value);
  if (!Number.isSafeInteger(id)) return null;
  if (type === "group") return { type: "group", id };
  return type === "request" ? { type: "request", id } : null;
}

/** The keyed row under the pointer. Below the last row counts as root-end. */
function rowAt(point: Point) {
  const rows = Array.from(
    listEl.value?.querySelectorAll<HTMLElement>("[data-drop-key]") ?? [],
  );
  const row = rows.find((candidate) => {
    const box = candidate.getBoundingClientRect();
    return point.y >= box.top && point.y < box.bottom;
  });
  if (row) return row;
  const last = rows[rows.length - 1];
  return last && point.y >= last.getBoundingClientRect().top ? last : null;
}

function resolveBrowserDrop(
  payload: DragPayload,
  point: Point,
): DropHit | null {
  const row = rowAt(point);
  const target = row ? treeTarget(row.dataset.dropKey ?? "") : null;
  if (!row || !target) return null;
  const zone =
    target.type === "root"
      ? "into"
      : hitZone(
          row.getBoundingClientRect(),
          point,
          target.type === "group" ? "group" : "request",
        );
  const drop = resolveTreeDrop(payload, target, zone, {
    sessions: props.sessions,
    groups: props.groups,
  });
  if (!drop) return null;
  const group =
    target.type === "group" ? groupById.value.get(target.id) : undefined;
  return {
    key: drop.key,
    zone: drop.zone,
    commit: () => commitTreeDrop(drop.command),
    ...(group?.collapsed
      ? { expand: () => emit("toggleGroup", group.id) }
      : {}),
  };
}

function commitTreeDrop(command: TreeCommand) {
  if (command.type === "moveRequests")
    emit("moveRequests", command.ids, command.groupId, command.beforeId);
  else
    emit("moveGroup", command.groupId, command.parentId, command.beforeGroupId);
}

function dragIds(id: number) {
  return selected.value.has(id) ? (props.selectedIds ?? []) : [id];
}

function pressRequest(session: RequestSession, event: PointerEvent) {
  if (deletingRequestId.value === session.id) return;
  drag.startPress(event, {
    payload: () => ({ kind: "requests", ids: dragIds(session.id) }),
    preview: () => {
      const count = dragIds(session.id).length;
      return count > 1
        ? { label: `${count} requests` }
        : { label: sessionLabel(session), method: session.draft.method };
    },
    onStart: () => {
      if (!selected.value.has(session.id))
        emit("updateSelection", [session.id], session.id);
    },
  });
}

function pressGroup(group: RequestGroup, event: PointerEvent) {
  if (editingId.value === group.id) return;
  drag.startPress(event, {
    payload: () => ({ kind: "group", id: group.id }),
    preview: () => ({ label: group.name, folder: true }),
  });
}

/** The indicator zone for a row key, when the drag targets it. */
function dropZoneFor(key: string) {
  const hit = drag.state.hit;
  return hit?.key === key ? hit.zone : null;
}
const draggedKeys = computed(() => {
  const payload = drag.state.payload;
  if (!payload) return new Set<string>();
  return new Set(
    payload.kind === "requests"
      ? payload.ids.map((id) => `request-${id}`)
      : [`group-${payload.id}`],
  );
});
/** Rows inside the folder that the drag targets with "into". */
const intoRows = computed(() => {
  const keys = new Set<string>();
  const hit = drag.state.hit;
  if (hit?.zone !== "into" || !hit.key.startsWith("group-")) return keys;
  const start = rows.value.findIndex((row) => rowKey(row) === hit.key);
  if (start < 0) return keys;
  const level = rows.value[start].level;
  for (const row of rows.value.slice(start + 1)) {
    if (row.level <= level) break;
    keys.add(rowKey(row));
  }
  return keys;
});

function stepSelection(session: RequestSession, event: KeyboardEvent) {
  if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown"))
    return;
  event.preventDefault();
  const ids = dragIds(session.id);
  const step = stepRequests(
    props.sessions,
    ids,
    event.key === "ArrowUp" ? -1 : 1,
  );
  if (!step) return;
  emit("moveRequests", ids, step.groupId, step.beforeId);
  void nextTick(() => {
    const row = listEl.value?.querySelector<HTMLElement>(
      `[data-request-id="${session.id}"]`,
    );
    row?.focus();
    row?.scrollIntoView?.({ block: "nearest" });
  });
}
```

- [ ] **Step 5: Update the template**

1. On the scroll list div (`class="min-h-0 flex-1 overflow-auto py-2"`), add `ref="listEl"` and `data-browser-list`.

2. On the UNGROUPED row div, add:

```vue
                data-drop-key="root-start"
                :data-drop-target="dropZoneFor('root') ?? undefined"
                :class="{
                  'bg-accent shadow-[inset_0_0_0_1px_var(--color-primary)]':
                    dropZoneFor('root') === 'into',
                }"
```

3. On the request row `<button>`, add `relative` to its class list and these attributes:

```vue
                    :data-drop-key="`request-${row.session.id}`"
                    @pointerdown="pressRequest(row.session, $event)"
                    @keydown="stepSelection(row.session, $event)"
```

Merge into its `:class` object:

```ts
                      'opacity-40': draggedKeys.has(`request-${row.session.id}`),
                      'bg-accent/40': intoRows.has(`request-${row.session.id}`),
```

Add as the first child of that button:

```vue
                    <span
                      v-if="
                        dropZoneFor(`request-${row.session.id}`) === 'before' ||
                        dropZoneFor(`request-${row.session.id}`) === 'after'
                      "
                      class="pointer-events-none absolute right-0 h-0.5 bg-primary"
                      :class="
                        dropZoneFor(`request-${row.session.id}`) === 'before'
                          ? 'top-0'
                          : 'bottom-0'
                      "
                      :style="{ left: `${indent(row.level)}px` }"
                      :data-drop-indicator="dropZoneFor(`request-${row.session.id}`)"
                    />
```

4. On the group row div (the one that had `:data-group-id`), add `relative` to its class list and:

```vue
                      :data-drop-key="`group-${row.group.id}`"
                      :data-drop-target="dropZoneFor(`group-${row.group.id}`) ?? undefined"
                      :class="{
                        'opacity-40': draggedKeys.has(`group-${row.group.id}`),
                        'bg-accent/40': intoRows.has(`group-${row.group.id}`),
                        'bg-accent shadow-[inset_0_0_0_1px_var(--color-primary)]':
                          dropZoneFor(`group-${row.group.id}`) === 'into',
                      }"
                      @pointerdown="pressGroup(row.group, $event)"
```

Add as its first child:

```vue
                      <span
                        v-if="
                          dropZoneFor(`group-${row.group.id}`) === 'before' ||
                          dropZoneFor(`group-${row.group.id}`) === 'after'
                        "
                        class="pointer-events-none absolute right-0 h-0.5 bg-primary"
                        :class="
                          dropZoneFor(`group-${row.group.id}`) === 'before'
                            ? 'top-0'
                            : 'bottom-0'
                        "
                        :style="{ left: `${indent(row.level)}px` }"
                        :data-drop-indicator="dropZoneFor(`group-${row.group.id}`)"
                      />
```

5. After the closing `</template>` of the `v-for`, still inside the list div, add the root drop area:

```vue
          <div data-drop-key="root-end" class="min-h-8" aria-hidden="true" />
```

- [ ] **Step 6: Wire App.vue and remove reorderGroup**

In `src/App.vue`:
- In the `useWorkspaceState()` destructure, replace `reorderGroup,` with `moveGroup,`.
- On `<RequestBrowser>`, replace `@reorder-group="reorderGroup"` with `@move-group="moveGroup"`.
- Add `import DragPreview from "./components/DragPreview.vue";` beside the other component imports, and add `<DragPreview />` as the last child of the root template element.

In `src/composables/useWorkspaceState.ts`: delete the `reorderGroup` function and remove `reorderGroup,` from the returned object.

- [ ] **Step 7: Replace the old HTML5 drop test**

In `src/components/__test__/RequestBrowser.test.ts`, rename the test at line 68 to `"selects a request range"`, delete the `moveRequests` spy and `onMoveRequests` prop, and delete the block from `await browser.get('[data-group-id="1"]').trigger("drop", {` through its `expect(moveRequests)...` assertion.

- [ ] **Step 8: Run tests and type check**

Run: `bunx vitest run && bunx vue-tsc --noEmit`
Expected: PASS. `grep -rn "dataTransfer\|draggable\|reorderGroup" src` returns nothing.

- [ ] **Step 9: Check in the running app**

Dev server runs on :1420 (see memory `work-in-place-live-reload`). In the app: drag a request between two others, into a folder, and a folder into another folder. Confirm the insert line, folder highlight, preview pill, and slide animation.

- [ ] **Step 10: Commit**

```bash
bun run lint
git add src/components/RequestBrowser.vue src/components/__test__/RequestBrowser.test.ts src/components/__test__/RequestBrowserDrag.test.ts src/App.vue src/composables/useWorkspaceState.ts
git commit -m "feat: pointer drag and drop in the request browser"
```

These files have unrelated uncommitted edits. Use `git add -p` for your hunks, or ask the user to commit their work first.

---

### Task 5: Tab reorder and cross-surface drag

**Files:**
- Modify: `src/components/RequestTabs.vue`
- Modify: `src/App.vue` (`<RequestTabs>` near line 403, destructure)
- Test: `src/components/__test__/RequestTabsDrag.test.ts` (create)

**Interfaces:**
- Consumes: `useDragDrop`, `DropHit` (Task 3); `hitZone`, `resolveTabDrop`, `stepTab`, `DragPayload`, `Point` (Task 2); state `openRequests` (Task 1).
- Produces: `RequestTabs` emit `openRequests: [ids: number[], beforeId: number | null]`. DOM contract: `[data-tab-bar]` on the outer bar, `data-drop-key="tab-<id>"` and `data-tab-id` on each `.tab-cell`, `[data-drop-indicator]` inside the target cell.

- [ ] **Step 1: Write failing tests**

Create `src/components/__test__/RequestTabsDrag.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
import RequestTabs from "@/components/RequestTabs.vue";
import { useDragDrop } from "@/composables/useDragDrop";
import { createSession } from "@/lib/session";

const first = createSession();
const second = createSession();
const third = createSession();
const tabLeft: Record<string, number> = {
  [`tab-${first.id}`]: 0,
  [`tab-${second.id}`]: 210,
  [`tab-${third.id}`]: 420,
};

function rect(left: number, top: number, width: number, height: number) {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON() {},
  } as DOMRect;
}
function pointer(type: string, x: number, y = 18) {
  return new PointerEvent(type, {
    bubbles: true,
    button: 0,
    clientX: x,
    clientY: y,
    pointerType: "mouse",
  });
}

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(
    function (this: Element) {
      const el = this as HTMLElement;
      if (el.hasAttribute("data-tab-bar") || el.getAttribute("role") === "tablist")
        return rect(0, 0, 1000, 36);
      const left = tabLeft[el.dataset?.dropKey ?? ""];
      return left === undefined ? rect(0, 0, 0, 0) : rect(left, 0, 210, 36);
    },
  );
});
const mounted: VueWrapper[] = [];
afterEach(() => {
  useDragDrop().cancel();
  mounted.splice(0).forEach((wrapper) => wrapper.unmount());
  vi.restoreAllMocks();
});

function setup() {
  const onOpenRequests = vi.fn();
  const onSelect = vi.fn();
  const wrapper = mount(RequestTabs, {
    props: {
      sessions: [first, second, third],
      activeId: first.id,
      onOpenRequests,
      onSelect,
    },
    attachTo: document.body,
  });
  mounted.push(wrapper);
  return { wrapper, onOpenRequests, onSelect };
}

describe("RequestTabs drag and drop", () => {
  it("reorders a tab after another", async () => {
    const { wrapper, onOpenRequests } = setup();
    wrapper
      .get(`[data-drop-key="tab-${first.id}"]`)
      .element.dispatchEvent(pointer("pointerdown", 100));
    window.dispatchEvent(pointer("pointermove", 110));
    window.dispatchEvent(pointer("pointermove", 400));
    await nextTick();
    expect(
      wrapper
        .get(`[data-drop-key="tab-${second.id}"] [data-drop-indicator]`)
        .attributes("data-drop-indicator"),
    ).toBe("after");
    window.dispatchEvent(pointer("pointerup", 400));
    expect(onOpenRequests).toHaveBeenCalledWith([first.id], third.id);
  });

  it("appends when dropped past the last tab", async () => {
    const { wrapper, onOpenRequests } = setup();
    wrapper
      .get(`[data-drop-key="tab-${first.id}"]`)
      .element.dispatchEvent(pointer("pointerdown", 100));
    window.dispatchEvent(pointer("pointermove", 110));
    window.dispatchEvent(pointer("pointermove", 900));
    window.dispatchEvent(pointer("pointerup", 900));
    expect(onOpenRequests).toHaveBeenCalledWith([first.id], null);
  });

  it("does not start a drag from the close button", async () => {
    const { wrapper } = setup();
    wrapper
      .get("[data-close-request]")
      .element.dispatchEvent(pointer("pointerdown", 190));
    window.dispatchEvent(pointer("pointermove", 400));
    expect(useDragDrop().state.payload).toBeNull();
  });

  it("does not select a tab after a drop", async () => {
    const { wrapper, onSelect } = setup();
    wrapper
      .get(`[data-drop-key="tab-${first.id}"]`)
      .element.dispatchEvent(pointer("pointerdown", 100));
    window.dispatchEvent(pointer("pointermove", 110));
    window.dispatchEvent(pointer("pointermove", 400));
    window.dispatchEvent(pointer("pointerup", 400));
    await wrapper.get(`#request-tab-${second.id}`).trigger("click");
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("moves the active tab with Alt+ArrowRight", async () => {
    const { wrapper, onOpenRequests } = setup();
    await wrapper
      .get(`#request-tab-${first.id}`)
      .trigger("keydown", { key: "ArrowRight", altKey: true });
    expect(onOpenRequests).toHaveBeenCalledWith([first.id], third.id);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `bunx vitest run src/components/__test__/RequestTabsDrag.test.ts`
Expected: FAIL — no `[data-drop-key]` elements.

- [ ] **Step 3: Implement the script changes**

In `src/components/RequestTabs.vue` (single quotes), add imports:

```ts
import { useDragDrop, type DropHit } from '@/composables/useDragDrop';
import {
  hitZone,
  resolveTabDrop,
  stepTab,
  type DragPayload,
  type Point,
} from '@/lib/drag-drop';
```

Add to `defineEmits`:

```ts
  openRequests: [ids: number[], beforeId: number | null];
```

Add after `const strip = ref<HTMLElement>();`:

```ts
const bar = ref<HTMLElement>();
const drag = useDragDrop();
drag.registerSurface({
  el: () => bar.value,
  scroller: () => strip.value,
  axis: 'x',
  resolve: resolveTabsDrop,
});
function resolveTabsDrop(payload: DragPayload, point: Point): DropHit | null {
  const openIds = props.sessions.map((session) => session.id);
  const cell = Array.from(
    strip.value?.querySelectorAll<HTMLElement>('[data-tab-id]') ?? []
  ).find((candidate) => {
    const box = candidate.getBoundingClientRect();
    return point.x >= box.left && point.x < box.right;
  });
  // Past the last tab (over the buttons or empty bar) appends.
  const targetId = cell
    ? Number(cell.dataset.tabId)
    : (openIds[openIds.length - 1] ?? null);
  const zone = cell
    ? hitZone(cell.getBoundingClientRect(), point, 'tab')
    : 'after';
  const drop = resolveTabDrop(payload, targetId, zone, openIds);
  if (!drop) return null;
  return {
    key: drop.key,
    zone: drop.zone,
    commit: () => emit('openRequests', drop.ids, drop.beforeId),
  };
}
function pressTab(session: RequestSession, event: PointerEvent) {
  drag.startPress(event, {
    payload: () => ({ kind: 'requests', ids: [session.id] }),
    preview: () => ({
      label: sessionLabel(session),
      method: session.draft.method,
    }),
  });
}
function dropZoneFor(id: number) {
  const hit = drag.state.hit;
  return hit?.key === `tab-${id}` ? hit.zone : null;
}
function isDragged(id: number) {
  const payload = drag.state.payload;
  return payload?.kind === 'requests' && payload.ids.includes(id);
}
```

At the top of `navigate`, before `if (event.altKey || event.ctrlKey || event.metaKey) return;`, add:

```ts
  if (
    event.altKey &&
    (event.key === 'ArrowLeft' || event.key === 'ArrowRight')
  ) {
    event.preventDefault();
    const id = props.sessions[index].id;
    const step = stepTab(
      props.sessions.map((session) => session.id),
      id,
      event.key === 'ArrowLeft' ? -1 : 1
    );
    if (step) emit('openRequests', [id], step.beforeId);
    void nextTick(() => document.getElementById(`request-tab-${id}`)?.focus());
    return;
  }
```

- [ ] **Step 4: Update the template**

1. On the outer bar div (`class="flex min-w-0 shrink-0 h-9 bg-muted border-b ..."`), add `ref="bar"` and `data-tab-bar`.
2. On each `.tab-cell` div, add:

```vue
                :data-drop-key="`tab-${session.id}`"
                :data-tab-id="session.id"
                @pointerdown="pressTab(session, $event)"
```

and change `:class="{ selected: activeId === session.id }"` to:

```vue
                :class="{
                  selected: activeId === session.id,
                  'opacity-40': isDragged(session.id),
                }"
```

3. Add as the first child of each `.tab-cell` div:

```vue
                <span
                  v-if="dropZoneFor(session.id)"
                  class="pointer-events-none absolute inset-y-0 z-10 w-0.5 bg-primary"
                  :class="dropZoneFor(session.id) === 'before' ? 'left-0' : '-right-px'"
                  :data-drop-indicator="dropZoneFor(session.id)"
                />
```

4. On the close button (`data-close-request`), add `data-no-drag`.

- [ ] **Step 5: Wire App.vue**

In `src/App.vue`: add `openRequests,` to the `useWorkspaceState()` destructure. Add beside `select`:

```ts
/** Place requests in the tab bar (drag or Alt+Arrow) and select them. */
function placeTabs(ids: number[], beforeId: number | null) {
  openRequests(ids, beforeId);
  updateSelection(ids, ids[0] ?? null);
}
```

On `<RequestTabs>`, add `@open-requests="placeTabs"`.

- [ ] **Step 6: Run tests and type check**

Run: `bunx vitest run && bunx vue-tsc --noEmit`
Expected: PASS, including existing `src/__test__/RequestTabs.test.ts` and `RequestTabsContextMenu.test.ts`.

- [ ] **Step 7: Check in the running app**

On :1420: reorder tabs by drag, drag a closed sidebar request onto the tab bar, drag a tab onto a sidebar folder, and try `Alt+←/→` on a focused tab.

- [ ] **Step 8: Commit**

```bash
bun run lint
git add src/components/RequestTabs.vue src/components/__test__/RequestTabsDrag.test.ts src/App.vue
git commit -m "feat: drag to reorder tabs and drag between tabs and browser"
```

Use `git add -p` for `RequestTabs.vue` and `App.vue` if they still hold unrelated uncommitted edits.

---

### Task 6: End-to-end drag flows

**Files:**
- Create: `e2e/drag-drop.spec.ts`

**Interfaces:**
- Consumes: DOM contract from Tasks 4 and 5 (`[data-browser-list]`, `data-drop-key`, `[data-tab-bar]`), existing labels: "New request" button, "Request URL" field, "Add top-level group" button, "Top-level group name" field, "Create top-level group" button, "SAVED LOCALLY" status.

- [ ] **Step 1: Write the spec**

Create `e2e/drag-drop.spec.ts`:

```ts
import { test, expect, type Locator, type Page } from "@playwright/test";

type Spot = "top" | "middle" | "bottom" | "left" | "right";

async function drag(page: Page, from: Locator, to: Locator, spot: Spot = "middle") {
  const a = await from.boundingBox();
  const b = await to.boundingBox();
  if (!a || !b) throw new Error("drag target not visible");
  const x = spot === "left" ? b.x + 6 : spot === "right" ? b.x + b.width - 6 : b.x + b.width / 2;
  const y = spot === "top" ? b.y + 3 : spot === "bottom" ? b.y + b.height - 3 : b.y + b.height / 2;
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2 + 8, { steps: 2 });
  await page.mouse.move(x, y, { steps: 10 });
  await page.mouse.up();
}

const list = (page: Page) => page.locator("[data-browser-list]");
const row = (page: Page, text: string) =>
  list(page).locator("[data-drop-key^='request-']", { hasText: text });
const folder = (page: Page, name: string) =>
  list(page).locator("[data-drop-key^='group-']", { hasText: name });
const tabs = (page: Page) =>
  page.getByRole("tablist", { name: "Requests", exact: true }).getByRole("tab");
async function rowOrder(page: Page) {
  return list(page)
    .locator("[data-drop-key]")
    .evaluateAll((els) => els.map((el) => el.textContent?.trim().replace(/\s+/g, " ") ?? ""));
}

async function addRequest(page: Page, path: string, first = false) {
  if (!first)
    await page.getByRole("button", { name: "New request", exact: true }).click();
  await page
    .locator('[data-request-pane][data-active="true"]')
    .getByLabel("Request URL", { exact: true })
    .fill(`https://example.test/${path}`);
}

async function addGroup(page: Page, name: string) {
  await page.getByRole("button", { name: "Add top-level group" }).click();
  await page.getByLabel("Top-level group name").fill(name);
  await page.getByRole("button", { name: "Create top-level group" }).click();
}

test("drag and drop in the browser and the tab bar", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await addRequest(page, "alpha", true);
  await addRequest(page, "beta");
  await addRequest(page, "gamma");

  // Reorder: gamma before alpha.
  await drag(page, row(page, "/gamma"), row(page, "/alpha"), "top");
  const order = await rowOrder(page);
  expect(order.findIndex((text) => text.includes("/gamma"))).toBeLessThan(
    order.findIndex((text) => text.includes("/alpha")),
  );

  // Into a folder.
  await addGroup(page, "Platform");
  await drag(page, row(page, "/alpha"), folder(page, "Platform"));
  await page.getByRole("button", { name: "Collapse Platform" }).click();
  await expect(row(page, "/alpha")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand Platform" }).click();

  // Nest a folder.
  await addGroup(page, "Nested");
  await drag(page, folder(page, "Nested"), folder(page, "Platform"));
  await page.getByRole("button", { name: "Collapse Platform" }).click();
  await expect(folder(page, "Nested")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand Platform" }).click();

  // Reorder tabs: alpha to the end.
  await drag(page, tabs(page).filter({ hasText: "/alpha" }), tabs(page).last(), "right");
  await expect(tabs(page).last()).toContainText("/alpha");

  // Sidebar to tab bar: close beta, then drag its row before the first tab.
  await page.getByRole("button", { name: "Close /beta", exact: true }).click();
  await expect(tabs(page).filter({ hasText: "/beta" })).toHaveCount(0);
  await drag(page, row(page, "/beta"), tabs(page).first(), "left");
  await expect(tabs(page).first()).toContainText("/beta");

  // Tab to folder: gamma's tab into Platform.
  await drag(page, tabs(page).filter({ hasText: "/gamma" }), folder(page, "Platform"));
  await page.getByRole("button", { name: "Collapse Platform" }).click();
  await expect(row(page, "/gamma")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand Platform" }).click();

  // Persistence.
  const before = await rowOrder(page);
  const tabOrder = await tabs(page).allTextContents();
  await expect(page.getByText("SAVED LOCALLY", { exact: true })).toBeVisible();
  await page.reload();
  await expect(list(page).locator("[data-drop-key^='request-']").first()).toBeVisible();
  expect(await rowOrder(page)).toEqual(before);
  expect(await tabs(page).allTextContents()).toEqual(tabOrder);
  expect(errors).toEqual([]);
});

test("Escape cancels a drag", async ({ page }) => {
  await page.goto("/");
  await addRequest(page, "alpha", true);
  await addRequest(page, "beta");
  const before = await rowOrder(page);
  const from = await row(page, "/alpha").boundingBox();
  const to = await row(page, "/beta").boundingBox();
  if (!from || !to) throw new Error("rows not visible");
  await page.mouse.move(from.x + 20, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + 20, to.y + to.height - 3, { steps: 10 });
  await expect(page.locator("[data-drag-preview]")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(page.locator("[data-drag-preview]")).toHaveCount(0);
  expect(await rowOrder(page)).toEqual(before);
});
```

- [ ] **Step 2: Run the spec**

Run: `bunx playwright test e2e/drag-drop.spec.ts`
Expected: PASS (2 tests). The dev server on :1420 is reused if running.

If a step fails, open the trace (`bunx playwright show-trace test-results/**/trace.zip`) and fix the product code when the behavior differs from the spec. Adjust only selectors in the test.

- [ ] **Step 3: Run the full suites**

Run: `bunx vitest run && bunx playwright test && bun run lint && bun run build`
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
git add e2e/drag-drop.spec.ts
git commit -m "test: end-to-end drag and drop flows"
```
