import { afterEach, describe, expect, it, vi } from "vitest";
import { enableAutoUnmount, mount } from "@vue/test-utils";
import RequestBrowser from "../RequestBrowser.vue";
import { createSession, type RequestSession } from "@/lib/session";
import type { RequestGroup } from "@/lib/groups";
import { openSubmenu } from "./menu-test-utils";

enableAutoUnmount(afterEach);

function mountBrowser(props: {
  sessions: RequestSession[];
  groups?: RequestGroup[];
  selectedIds?: number[];
  [key: string]: unknown;
}) {
  return mount(RequestBrowser, {
    props: { activeId: props.sessions[0].id, groups: [], ...props },
    attachTo: document.body,
  });
}
async function openRequestMenu(
  browser: ReturnType<typeof mountBrowser>,
  id: number,
) {
  await browser.get(`[data-request-id="${id}"]`).trigger("contextmenu");
}
const find = (selector: string) =>
  document.body.querySelector(selector) as HTMLElement;

describe("Browser request menu", () => {
  it("No auth sets a local none configuration", async () => {
    const s = createSession();
    const onSetRequestLocalAuth = vi.fn();
    const browser = mountBrowser({ sessions: [s], onSetRequestLocalAuth });
    await openRequestMenu(browser, s.id);
    await openSubmenu(find("[data-auth-menu]"));
    find('[data-auth-mode="none"]').click();
    expect(onSetRequestLocalAuth).toHaveBeenCalledWith(s.id, { type: "none" });
  });

  it("keeps a bearer token when Bearer is chosen again", async () => {
    const s = createSession();
    s.draft.localAuth = { type: "bearer", token: "keep" };
    const onSetRequestLocalAuth = vi.fn();
    const browser = mountBrowser({ sessions: [s], onSetRequestLocalAuth });
    await openRequestMenu(browser, s.id);
    await openSubmenu(find("[data-auth-menu]"));
    find('[data-auth-mode="bearer"]').click();
    expect(onSetRequestLocalAuth).not.toHaveBeenCalled();
  });

  it("checks the current authorization mode", async () => {
    const s = createSession();
    s.draft.localAuth = { type: "none" };
    const browser = mountBrowser({ sessions: [s] });
    await openRequestMenu(browser, s.id);
    await openSubmenu(find("[data-auth-menu]"));
    expect(find('[data-auth-mode="none"]').getAttribute("aria-checked")).toBe(
      "true",
    );
  });

  it("labels and deletes a multi-selection after one confirmation", async () => {
    const [a, b, c] = [createSession(), createSession(), createSession()];
    const onDeleteRequest = vi.fn();
    const browser = mountBrowser({
      sessions: [a, b, c],
      selectedIds: [a.id, b.id],
      onDeleteRequest,
    });
    await openRequestMenu(browser, a.id);
    const del = find("[data-request-delete]");
    expect(del.textContent?.trim()).toBe("Delete 2 requests");
    del.click();
    await browser.vm.$nextTick();
    expect(onDeleteRequest).not.toHaveBeenCalled();
    expect(browser.text()).toContain("Delete 2 requests?");
    await browser.get("[data-confirm-delete-request]").trigger("click");
    expect(onDeleteRequest.mock.calls.map((call) => call[0])).toEqual([
      a.id,
      b.id,
    ]);
  });

  it("disables a multi-selection delete when one request is running", async () => {
    const [a, b] = [createSession(), createSession()];
    b.busy = true;
    const browser = mountBrowser({
      sessions: [a, b],
      selectedIds: [a.id, b.id],
    });
    await openRequestMenu(browser, a.id);
    expect(find("[data-request-delete]").hasAttribute("data-disabled")).toBe(
      true,
    );
  });

  it("moves the selection through the nested tree", async () => {
    const [a, b] = [createSession(), createSession()];
    const onMoveRequests = vi.fn();
    const browser = mountBrowser({
      sessions: [a, b],
      groups: [{ id: 1, name: "API", parentId: null, collapsed: false }],
      selectedIds: [a.id, b.id],
      onMoveRequests,
    });
    await openRequestMenu(browser, a.id);
    const moveMenu = find("[data-move-menu]");
    expect(moveMenu.textContent?.trim()).toBe("Move 2 requests to");
    await openSubmenu(moveMenu);
    find('[data-move-target="1"]').click();
    expect(onMoveRequests).toHaveBeenCalledWith([a.id, b.id], 1, null);
  });

  it("marks a single request's group as the current Move to location", async () => {
    const s = createSession();
    s.groupId = 1;
    const browser = mountBrowser({
      sessions: [s],
      groups: [{ id: 1, name: "API", parentId: null, collapsed: false }],
    });
    await openRequestMenu(browser, s.id);
    expect(find("[data-move-menu]").textContent?.trim()).toBe("Move to");
    await openSubmenu(find("[data-move-menu]"));
    expect(find('[data-move-target="1"]').getAttribute("aria-checked")).toBe(
      "true",
    );
  });

  it("copies the target URL", async () => {
    const s = createSession();
    s.draft.url = "https://api.example.test/x";
    const onCopy = vi.fn();
    const browser = mountBrowser({ sessions: [s], onCopy });
    await openRequestMenu(browser, s.id);
    find("[data-request-copy-url]").click();
    expect(onCopy).toHaveBeenCalledWith("https://api.example.test/x");
  });

  it("copies the target cURL and hides copy items for a multi-selection", async () => {
    const [a, b] = [createSession(), createSession()];
    const onCopy = vi.fn();
    const browser = mountBrowser({
      sessions: [a, b],
      curlFor: () => "curl https://x",
      onCopy,
    });
    await openRequestMenu(browser, a.id);
    find("[data-request-copy-curl]").click();
    expect(onCopy).toHaveBeenCalledWith("curl https://x");
    await browser.setProps({ selectedIds: [a.id, b.id] });
    await openRequestMenu(browser, a.id);
    expect(find("[data-request-copy-curl]")).toBeNull();
  });
});
