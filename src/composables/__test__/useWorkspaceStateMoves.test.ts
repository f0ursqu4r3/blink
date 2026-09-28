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
    expect(
      state.groups.value.find((group) => group.id === b.id)?.parentId,
    ).toBe(a.id);
    state.moveGroup(b.id, null, a.id);
    expect(state.groups.value.map((group) => group.id)).toEqual([b.id, a.id]);
    expect(state.groups.value[0].parentId).toBeNull();
  });

  it("rejects a cycle", () => {
    const a = state.addGroup("A", null)!;
    const b = state.addGroup("B", a.id)!;
    state.moveGroup(a.id, b.id, null);
    expect(
      state.groups.value.find((group) => group.id === a.id)?.parentId,
    ).toBeNull();
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
