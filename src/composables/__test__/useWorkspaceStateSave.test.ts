import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, type EffectScope } from "vue";

const saves = vi.hoisted(() => [] as string[]);
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
    save = vi.fn((content: string) => {
      saves.push(content);
      return Promise.resolve();
    });
  },
}));

import { useWorkspaceState } from "@/composables/useWorkspaceState";

let scope: EffectScope;
let state: ReturnType<typeof useWorkspaceState>;

beforeEach(async () => {
  scope = effectScope();
  scope.run(() => {
    state = useWorkspaceState();
  });
  state.ready.value = true;
  await nextTick();
  saves.length = 0;
});
afterEach(() => {
  scope.stop();
  vi.restoreAllMocks();
});

const savedScroll = (content: string) =>
  JSON.parse(content).tabs[0].view.responseScroll;

describe("workspace autosave", () => {
  it("does not save when only the response scroll changes", async () => {
    state.sessions.value[0].view.responseScroll = 480;
    await nextTick();
    expect(saves).toHaveLength(0);
  });

  it("saves the latest scroll with the next real change", async () => {
    state.sessions.value[0].view.responseScroll = 480;
    await nextTick();
    state.sessions.value[0].draft.url = "https://example.test/next";
    await nextTick();
    expect(saves).toHaveLength(1);
    expect(savedScroll(saves[0])).toBe(480);
  });

  it("saves the latest scroll on flush (quit)", async () => {
    state.sessions.value[0].view.responseScroll = 480;
    await state.flush();
    expect(savedScroll(saves[saves.length - 1])).toBe(480);
  });
});
