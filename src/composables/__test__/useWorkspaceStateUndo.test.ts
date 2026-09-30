import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope, type EffectScope } from "vue";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: () => false,
}));
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
  for (let index = 0; index < count; index++) {
    const session = createSession();
    session.draft.url = `https://example.test/${index}`;
    state.sessions.value.push(session);
    state.openRequest(session.id);
  }
  return state.sessions.value.map((session) => session.id);
}

describe("reopenClosedTab", () => {
  it("reopens tabs in reverse close order", () => {
    const [a, b, c] = addSessions(2);
    state.closeTab(b);
    state.closeTabs([a, c]);
    expect(state.openIds.value).toEqual([]);
    expect(state.reopenClosedTab()).toBe(c);
    expect(state.reopenClosedTab()).toBe(a);
    expect(state.reopenClosedTab()).toBe(b);
    expect(state.reopenClosedTab()).toBeNull();
    expect(state.activeId.value).toBe(b);
  });
  it("skips deleted and already open requests", () => {
    const [a, b] = addSessions(1);
    state.closeTab(a);
    state.closeTab(b);
    state.deleteRequest(b);
    state.openRequest(a);
    expect(state.reopenClosedTab()).toBeNull();
  });
});

describe("undoDelete", () => {
  it("restores a deleted request at its place and tab", () => {
    const [a, b, c] = addSessions(2);
    state.deleteRequest(b);
    expect(state.lastDeletion.value?.kind).toBe("requests");
    expect(state.undoDelete()).toBe(true);
    expect(state.sessions.value.map((s) => s.id)).toEqual([a, b, c]);
    expect(state.openIds.value).toEqual([a, b, c]);
    expect(state.activeId.value).toBe(b);
    expect(state.lastDeletion.value).toBeNull();
    expect(state.undoDelete()).toBe(false);
  });
  it("undoes requests deleted in one task together", async () => {
    const [a, b, c] = addSessions(2);
    state.deleteRequest(a);
    state.deleteRequest(c);
    expect(state.undoDelete()).toBe(true);
    expect(state.sessions.value.map((s) => s.id)).toEqual([a, b, c]);
    state.deleteRequest(a);
    await new Promise((resolve) => setTimeout(resolve));
    state.deleteRequest(b);
    state.undoDelete();
    expect(state.sessions.value.map((s) => s.id)).toEqual([b, c]);
  });
  it("drops the placeholder made when the last request was deleted", () => {
    const [only] = state.sessions.value.map((s) => s.id);
    state.sessions.value[0].draft.url = "https://example.test";
    state.deleteRequest(only);
    expect(state.sessions.value[0].id).not.toBe(only);
    state.undoDelete();
    expect(state.sessions.value.map((s) => s.id)).toEqual([only]);
    expect(state.openIds.value).toEqual([only]);
  });
  it("restores a deleted group and its contents", () => {
    const [a] = addSessions(0);
    const parent = state.addGroup("Parent", null);
    const group = state.addGroup("Child", parent.id);
    const nested = state.addGroup("Nested", group.id);
    state.moveRequest(a, group.id);
    state.deleteGroup(group.id);
    expect(state.sessions.value[0].groupId).toBe(parent.id);
    state.undoDelete();
    expect(state.groups.value.map((g) => g.name)).toEqual([
      "Parent",
      "Child",
      "Nested",
    ]);
    expect(state.groups.value[2].parentId).toBe(group.id);
    expect(state.sessions.value[0].groupId).toBe(group.id);
    expect(nested.id).toBe(state.groups.value[2].id);
  });
  it("expires after the undo window", () => {
    vi.useFakeTimers();
    try {
      const [, b] = addSessions(1);
      state.deleteRequest(b);
      vi.advanceTimersByTime(10_001);
      expect(state.lastDeletion.value).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
