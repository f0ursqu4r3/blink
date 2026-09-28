import { afterEach, describe, expect, it, vi } from "vitest";
import { enableAutoUnmount, mount } from "@vue/test-utils";
import RequestBrowser from "../RequestBrowser.vue";
import { createSession } from "@/lib/session";
enableAutoUnmount(afterEach);

describe("request browser", () => {
  it("renders nested groups and moves the active request into a selected group", async () => {
    const active = createSession();
    active.draft.url = "https://api.example.test/users";
    const nested = createSession();
    nested.groupId = 2;
    const moveRequest = vi.fn();
    const browser = mount(RequestBrowser, {
      global: { stubs: { DropdownMenuPortal: { template: "<slot />" } } },
      props: {
        sessions: [active, nested],
        activeId: active.id,
        groups: [
          { id: 1, name: "Platform", parentId: null, collapsed: false },
          { id: 2, name: "Identity", parentId: 1, collapsed: false },
        ],
        onMoveRequest: moveRequest,
      },
    });

    expect(browser.text()).toContain("Platform");
    expect(browser.text()).toContain("Identity");
    expect(browser.text()).toContain("/users");
    await browser
      .get('[aria-label="More actions for Identity"]')
      .trigger("keydown", { key: "Enter" });
    const move = browser
      .findAll('[role="menuitem"]')
      .find((item) => item.text() === "Move selection here")!;
    await move.trigger("click");

    expect(moveRequest).toHaveBeenCalledWith(active.id, 2);
  });

  it("starts a nested group from the selected parent", async () => {
    const active = createSession();
    const createGroup = vi.fn();
    const browser = mount(RequestBrowser, {
      global: { stubs: { DropdownMenuPortal: { template: "<slot />" } } },
      props: {
        sessions: [active],
        activeId: active.id,
        groups: [{ id: 1, name: "Platform", parentId: null, collapsed: false }],
        onCreateGroup: createGroup,
      },
    });

    await browser
      .get('[aria-label="More actions for Platform"]')
      .trigger("keydown", { key: "Enter" });
    await browser
      .get('[aria-label="Add group inside Platform"]')
      .trigger("click");
    await browser
      .get('[aria-label="Group name in Platform"]')
      .setValue("Identity");
    await browser.get(".child-form").trigger("submit");

    expect(createGroup).toHaveBeenCalledWith("Identity", 1);
  });

  it("selects a request range and moves the selection into a group on drop", async () => {
    const first = createSession();
    first.draft.url = "https://api.example.test/first";
    const second = createSession();
    second.draft.url = "https://api.example.test/second";
    const third = createSession();
    third.draft.url = "https://api.example.test/third";
    const updateSelection = vi.fn();
    const moveRequests = vi.fn();
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [first, second, third],
        activeId: first.id,
        selectedIds: [first.id],
        selectionAnchorId: first.id,
        groups: [{ id: 1, name: "Platform", parentId: null, collapsed: false }],
        onUpdateSelection: updateSelection,
        onMoveRequests: moveRequests,
      },
    });

    await browser.get(`[data-request-id="${third.id}"]`).trigger("click", {
      shiftKey: true,
    });
    expect(updateSelection).toHaveBeenCalledWith(
      [first.id, second.id, third.id],
      first.id,
    );

    await browser.get('[data-group-id="1"]').trigger("drop", {
      dataTransfer: {
        types: ["application/x-blink-request-ids"],
        getData: () => JSON.stringify([first.id, second.id, third.id]),
      },
    });
    expect(moveRequests).toHaveBeenCalledWith(
      [first.id, second.id, third.id],
      1,
      null,
    );
  });
});
