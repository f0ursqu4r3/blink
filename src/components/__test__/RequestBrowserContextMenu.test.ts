import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import RequestBrowser from "../RequestBrowser.vue";
import { createSession } from "@/lib/session";
import type { RequestGroup } from "@/lib/groups";

// Stubs for context menu primitives – always render content visible so items
// can be found and clicked in jsdom without needing real pointer events.
const contextMenuStubs = {
  ContextMenu: { template: "<div><slot /></div>" },
  ContextMenuTrigger: {
    template: "<div><slot /></div>",
    props: ["asChild"],
  },
  ContextMenuContent: { template: "<div><slot /></div>" },
  ContextMenuItem: {
    template: "<button @click=\"$emit('select', $event)\"><slot /></button>",
    emits: ["select"],
  },
  ContextMenuSeparator: { template: "<hr />" },
  ContextMenuSub: { template: "<div><slot /></div>" },
  ContextMenuSubTrigger: {
    template: "<div><slot /></div>",
  },
  ContextMenuSubContent: { template: "<div><slot /></div>" },
};

function makeGroup(overrides: Partial<RequestGroup> = {}): RequestGroup {
  return {
    id: 1,
    name: "Alpha",
    parentId: null,
    collapsed: false,
    ...overrides,
  };
}

function mountBrowser(propsOverride: Record<string, unknown> = {}) {
  const session = createSession();
  session.groupId = 1;
  const group = makeGroup({ id: 1, name: "Alpha" });
  return mount(RequestBrowser, {
    props: {
      sessions: [session],
      activeId: session.id,
      groups: [group],
      ...propsOverride,
    },
    global: { stubs: contextMenuStubs },
  });
}

describe("RequestBrowser – context menus and new features", () => {
  // ── Group settings button ────────────────────────────────────────────────

  it("group settings button emits openGroupSettings with group id", async () => {
    const openGroupSettings = vi.fn();
    const browser = mountBrowser({ onOpenGroupSettings: openGroupSettings });

    const btn = browser.get('[aria-label="Group settings for Alpha"]');
    await btn.trigger("click");

    expect(openGroupSettings).toHaveBeenCalledWith(1);
  });

  // ── Group row context menu ───────────────────────────────────────────────

  it("context menu Settings item on group row emits openGroupSettings", async () => {
    const openGroupSettings = vi.fn();
    const browser = mountBrowser({ onOpenGroupSettings: openGroupSettings });

    // The Settings item is in the context menu content rendered next to the group row.
    // Since stubs flatten the tree, search within the whole browser for a Settings button
    // near the group.
    const groupCtx = browser.get('[data-group-context="1"]');
    const items = groupCtx
      .findAll("button")
      .filter((b) => b.text() === "Settings");
    expect(items.length).toBeGreaterThan(0);
    await items[0].trigger("click");

    expect(openGroupSettings).toHaveBeenCalledWith(1);
  });

  // ── Request row context menu ─────────────────────────────────────────────

  it("context menu Duplicate item emits duplicateRequest", async () => {
    const session = createSession();
    session.groupId = 1;
    const duplicateRequest = vi.fn();
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [session],
        activeId: session.id,
        groups: [makeGroup()],
        onDuplicateRequest: duplicateRequest,
      },
      global: { stubs: contextMenuStubs },
    });

    const requestCtx = browser.get(`[data-request-context="${session.id}"]`);
    const items = requestCtx
      .findAll("button")
      .filter((b) => b.text() === "Duplicate");
    expect(items.length).toBeGreaterThan(0);
    await items[0].trigger("click");

    expect(duplicateRequest).toHaveBeenCalledWith(session.id);
  });

  it("context menu Close item emits closeRequest", async () => {
    const session = createSession();
    session.groupId = 1;
    const closeRequest = vi.fn();
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [session],
        activeId: session.id,
        groups: [makeGroup()],
        onCloseRequest: closeRequest,
      },
      global: { stubs: contextMenuStubs },
    });

    const requestCtx = browser.get(`[data-request-context="${session.id}"]`);
    const items = requestCtx
      .findAll("button")
      .filter((b) => b.text() === "Close");
    expect(items.length).toBeGreaterThan(0);
    await items[0].trigger("click");

    expect(closeRequest).toHaveBeenCalledWith(session.id);
  });

  it("context menu 'No auth' item emits setRequestLocalAuth with {type:'none'}", async () => {
    const session = createSession();
    session.groupId = 1;
    const setRequestLocalAuth = vi.fn();
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [session],
        activeId: session.id,
        groups: [makeGroup()],
        onSetRequestLocalAuth: setRequestLocalAuth,
      },
      global: { stubs: contextMenuStubs },
    });

    const requestCtx = browser.get(`[data-request-context="${session.id}"]`);
    const items = requestCtx
      .findAll("button")
      .filter((b) => b.text() === "No auth");
    expect(items.length).toBeGreaterThan(0);
    await items[0].trigger("click");

    expect(setRequestLocalAuth).toHaveBeenCalledWith(session.id, {
      type: "none",
    });
  });

  // ── Right-click selection behaviour ─────────────────────────────────────

  it("right-clicking a request not in selection selects it first", async () => {
    const session = createSession();
    session.groupId = 1;
    const other = createSession();
    other.groupId = 1;
    const select = vi.fn();
    const updateSelection = vi.fn();
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [session, other],
        activeId: other.id,
        // other is the only selected one
        selectedIds: [other.id],
        selectionAnchorId: other.id,
        groups: [makeGroup()],
        onSelect: select,
        onUpdateSelection: updateSelection,
      },
      global: { stubs: contextMenuStubs },
    });

    await browser
      .get(`[data-request-id="${session.id}"]`)
      .trigger("contextmenu");

    expect(select).toHaveBeenCalledWith(session.id);
    expect(updateSelection).toHaveBeenCalledWith([session.id], session.id);
  });

  it("right-clicking a request already in multi-selection retains selection", async () => {
    const s1 = createSession();
    s1.groupId = 1;
    const s2 = createSession();
    s2.groupId = 1;
    const select = vi.fn();
    const updateSelection = vi.fn();
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [s1, s2],
        activeId: s1.id,
        selectedIds: [s1.id, s2.id],
        selectionAnchorId: s1.id,
        groups: [makeGroup()],
        onSelect: select,
        onUpdateSelection: updateSelection,
      },
      global: { stubs: contextMenuStubs },
    });

    // Right-click on s2 which IS in the multi-selection → selection unchanged
    await browser.get(`[data-request-id="${s2.id}"]`).trigger("contextmenu");

    expect(select).not.toHaveBeenCalled();
    expect(updateSelection).not.toHaveBeenCalled();
  });

  // ── Auth indicator ───────────────────────────────────────────────────────

  it("lock icon appears on group row when group has bearer localAuth", () => {
    const session = createSession();
    session.groupId = 1;
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [session],
        activeId: session.id,
        groups: [
          makeGroup({
            localAuth: { type: "bearer", token: "secret" },
          }),
        ],
      },
      global: { stubs: contextMenuStubs },
    });

    const groupRow = browser.get('[data-group-id="1"]');
    expect(groupRow.find("[data-auth-indicator]").exists()).toBe(true);
  });

  it("lock icon appears on request row when auth is inherited from group bearer", () => {
    const session = createSession();
    session.groupId = 1;
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [session],
        activeId: session.id,
        groups: [
          makeGroup({
            localAuth: { type: "bearer", token: "inherited" },
          }),
        ],
      },
      global: { stubs: contextMenuStubs },
    });

    const requestEl = browser.get(`[data-request-id="${session.id}"]`);
    expect(requestEl.find("[data-auth-indicator]").exists()).toBe(true);
  });

  it("lock icon absent when effective auth is none", () => {
    const session = createSession();
    session.groupId = 1;
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [session],
        activeId: session.id,
        groups: [makeGroup()], // no localAuth → none
      },
      global: { stubs: contextMenuStubs },
    });

    expect(browser.find("[data-auth-indicator]").exists()).toBe(false);
  });
});
