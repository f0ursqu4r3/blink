import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { useWorkspaceState } from "../useWorkspaceState";
import { decodeWorkspace, WORKSPACE_KEY } from "@/lib/workspace";
import { defaultPreferences } from "@/lib/preferences";
import type { Method } from "@/lib/request";

vi.mock("@/lib/transport", () => ({ nativeTransport: false }));

let wrapper: VueWrapper;
let state: ReturnType<typeof useWorkspaceState>;
beforeEach(() => {
  localStorage.clear();
  wrapper = mount(
    defineComponent({
      setup() {
        state = useWorkspaceState();
        return () => null;
      },
    }),
  );
});
afterEach(async () => {
  wrapper.unmount();
  await flushPromises();
  localStorage.clear();
});

const stored = () => decodeWorkspace(localStorage.getItem(WORKSPACE_KEY)!);

describe("workspace preference lifecycle", () => {
  it("preserves global tokens and all preferences on browser unload", async () => {
    const preferences = {
      ...defaultPreferences(),
      defaultMethod: "PATCH" as const,
      defaultBodyMode: "json" as const,
      pretty: false,
      wrap: true,
      confirmCloseDrafts: false,
    };
    state.setGlobalDefinitions({ host: "https://example.test" });
    state.setPreferences(preferences);
    window.dispatchEvent(new Event("beforeunload"));
    expect(stored().globalDefinitions).toEqual({
      host: "https://example.test",
    });
    expect(stored().preferences).toEqual(preferences);
    await flushPromises();
    expect(stored().preferences).toEqual(preferences);
  });

  it("resets tokens and preferences both in memory and in the saved workspace", async () => {
    state.setGlobalDefinitions({ host: "https://example.test" });
    state.setPreferences({ ...defaultPreferences(), wrap: true });
    state.addGroup("API", null);
    await state.flush();
    await state.reset();
    await flushPromises();
    expect(state.globalDefinitions.value).toEqual({});
    expect(state.preferences.value).toEqual(defaultPreferences());
    expect(stored()).toMatchObject({
      globalDefinitions: {},
      groups: [],
      preferences: defaultPreferences(),
    });
  });

  it("does not reparent into a missing group or descendant", () => {
    const parent = state.addGroup("Parent", null);
    const child = state.addGroup("Child", parent.id);
    state.setGroupParent(parent.id, child.id);
    state.setGroupParent(parent.id, 999999);
    expect(
      state.groups.value.find((g) => g.id === parent.id)?.parentId,
    ).toBeNull();
  });

  it("rejects invalid preferences without poisoning the saved workspace", async () => {
    state.setPreferences({
      ...defaultPreferences(),
      defaultMethod: "BAD METHOD" as Method,
    });
    expect(state.preferences.value).toEqual(defaultPreferences());
    expect(await state.flush()).toBe(true);
  });

  it("rejects invalid group defaults without replacing existing defaults", async () => {
    const group = state.addGroup("API", null);
    state.setGroupNewRequestDefaults(group.id, "POST", "https://example.test");
    state.setGroupNewRequestDefaults(
      group.id,
      "BAD METHOD" as Method,
      "x".repeat(65537),
    );
    expect(state.groups.value[0]).toMatchObject({
      defaultMethod: "POST",
      defaultUrl: "https://example.test",
    });
    expect(await state.flush()).toBe(true);
  });

  it("rejects oversized group names in both rename paths", () => {
    const group = state.addGroup("API", null);
    state.setGroupName(group.id, "x".repeat(81));
    expect(state.groups.value[0].name).toBe("API");
    state.renameGroup(group.id, "x".repeat(81));
    expect(state.groups.value[0].name).toBe("API");
  });

  it("clears overrides and resolves group location independently", async () => {
    const parent = state.addGroup("Parent", null);
    const child = state.addGroup("Child", parent.id);
    state.setGroupNewRequestDefaults(child.id, "PUT", "https://example.test");
    state.setGroupParent(child.id, null);
    state.setGroupNewRequestDefaults(child.id, undefined, undefined);
    await state.flush();
    expect(stored().groups.find((g) => g.id === child.id)).toMatchObject({
      parentId: null,
    });
    expect(
      stored().groups.find((g) => g.id === child.id)?.defaultMethod,
    ).toBeUndefined();
  });
});
