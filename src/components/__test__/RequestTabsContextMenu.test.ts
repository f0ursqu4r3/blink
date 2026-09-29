import { afterEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import RequestTabs from "../RequestTabs.vue";
import { createSession } from "@/lib/session";
import { menuLabels } from "./menu-test-utils";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  document.body.innerHTML = "";
});

function render(count = 2) {
  const sessions = Array.from({ length: count }, () => createSession());
  const wrapper = mount(RequestTabs, {
    props: {
      sessions,
      activeId: sessions[0].id,
      curlFor: (id: number) => (id === sessions[0].id ? "curl https://a" : ""),
    },
    attachTo: document.body,
  });
  wrappers.push(wrapper);
  return { wrapper, sessions };
}

// ── Tab strip blank area context menu ──────────────────────────────────────

describe("RequestTabs tab-strip context menu", () => {
  it("opens on right-click of tab-strip trigger", async () => {
    const { wrapper } = render();
    await wrapper
      .get('[data-testid="tab-strip-ctx-trigger"]')
      .trigger("contextmenu");
    expect(
      document.body.querySelector('[data-testid="tab-strip-ctx-new"]'),
    ).not.toBeNull();
  });

  it("'New request' emits create", async () => {
    const { wrapper } = render();
    await wrapper
      .get('[data-testid="tab-strip-ctx-trigger"]')
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="tab-strip-ctx-new"]',
      ) as HTMLElement
    ).click();
    expect(wrapper.emitted("create")).toBeTruthy();
  });
});

// ── Tab cell context menu ──────────────────────────────────────────────────

describe("RequestTabs tab-cell context menu", () => {
  it("opens on right-click of first tab cell trigger", async () => {
    const { wrapper, sessions } = render();
    await wrapper
      .get(`[data-testid="tab-ctx-trigger-${sessions[0].id}"]`)
      .trigger("contextmenu");
    expect(
      document.body.querySelector(
        `[data-testid="tab-ctx-close-${sessions[0].id}"]`,
      ),
    ).not.toBeNull();
  });

  it("'Close' emits close with session id", async () => {
    const { wrapper, sessions } = render();
    await wrapper
      .get(`[data-testid="tab-ctx-trigger-${sessions[0].id}"]`)
      .trigger("contextmenu");
    (
      document.body.querySelector(
        `[data-testid="tab-ctx-close-${sessions[0].id}"]`,
      ) as HTMLElement
    ).click();
    expect(wrapper.emitted("close")).toBeTruthy();
    expect(wrapper.emitted("close")![0]).toEqual([sessions[0].id]);
  });

  it("'Close' stays enabled when session is busy", async () => {
    const sessions = [{ ...createSession(), busy: true }, createSession()];
    const wrapper = mount(RequestTabs, {
      props: { sessions, activeId: sessions[0].id },
      attachTo: document.body,
    });
    wrappers.push(wrapper);
    await wrapper
      .get(`[data-testid="tab-ctx-trigger-${sessions[0].id}"]`)
      .trigger("contextmenu");
    const closeItem = document.body.querySelector(
      `[data-testid="tab-ctx-close-${sessions[0].id}"]`,
    ) as HTMLElement;
    expect(closeItem.hasAttribute("data-disabled")).toBe(false);
  });
});

// ── VS Code tab actions ────────────────────────────────────────────────────

async function openTabMenu(wrapper: ReturnType<typeof mount>, id: number) {
  await wrapper
    .get(`[data-testid="tab-ctx-trigger-${id}"]`)
    .trigger("contextmenu");
}
const item = (testid: string) =>
  document.body.querySelector(`[data-testid="${testid}"]`) as HTMLElement;

describe("RequestTabs tab actions", () => {
  it("lists the VS Code tab actions in order", async () => {
    const { wrapper, sessions } = render(3);
    await openTabMenu(wrapper, sessions[1].id);
    expect(menuLabels()).toEqual([
      "Close",
      "Close others",
      "Close to the right",
      "Close all",
      "Duplicate",
      "Copy URL",
      "Copy as cURL",
      "Reveal in Browser",
    ]);
  });

  it("Close others emits closeMany with every other tab", async () => {
    const { wrapper, sessions } = render(3);
    await openTabMenu(wrapper, sessions[1].id);
    item(`tab-ctx-close-others-${sessions[1].id}`).click();
    expect(wrapper.emitted("closeMany")![0]).toEqual([
      [sessions[0].id, sessions[2].id],
    ]);
  });

  it("Close to the right emits the tabs after the target", async () => {
    const { wrapper, sessions } = render(3);
    await openTabMenu(wrapper, sessions[0].id);
    item(`tab-ctx-close-right-${sessions[0].id}`).click();
    expect(wrapper.emitted("closeMany")![0]).toEqual([
      [sessions[1].id, sessions[2].id],
    ]);
  });

  it("disables Close to the right and Close others when there is one tab", async () => {
    const { wrapper, sessions } = render(1);
    await openTabMenu(wrapper, sessions[0].id);
    expect(
      item(`tab-ctx-close-right-${sessions[0].id}`).hasAttribute(
        "data-disabled",
      ),
    ).toBe(true);
    expect(
      item(`tab-ctx-close-others-${sessions[0].id}`).hasAttribute(
        "data-disabled",
      ),
    ).toBe(true);
  });

  it("Duplicate duplicates the target tab, not the active tab", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[1].id);
    item(`tab-ctx-duplicate-${sessions[1].id}`).click();
    expect(wrapper.emitted("duplicate")![0]).toEqual([sessions[1].id]);
    expect(wrapper.emitted("select")).toBeFalsy();
  });

  it("Copy URL emits copy with the draft URL", async () => {
    const { wrapper, sessions } = render(2);
    sessions[0].draft.url = "https://a";
    await openTabMenu(wrapper, sessions[0].id);
    item(`tab-ctx-copy-url-${sessions[0].id}`).click();
    expect(wrapper.emitted("copy")![0]).toEqual(["https://a"]);
  });

  it("Copy as cURL emits the command", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[0].id);
    item(`tab-ctx-copy-curl-${sessions[0].id}`).click();
    expect(wrapper.emitted("copy")![0]).toEqual(["curl https://a"]);
  });

  it("disables Copy as cURL when the draft does not build", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[1].id);
    expect(
      item(`tab-ctx-copy-curl-${sessions[1].id}`).hasAttribute("data-disabled"),
    ).toBe(true);
  });

  it("Reveal in Browser emits reveal", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[0].id);
    item(`tab-ctx-reveal-${sessions[0].id}`).click();
    expect(wrapper.emitted("reveal")![0]).toEqual([sessions[0].id]);
  });

  it("shows the Close and Duplicate shortcuts", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[0].id);
    expect(
      document.body.querySelectorAll('[data-slot="context-menu-shortcut"]')
        .length,
    ).toBe(2);
  });
});

describe("RequestTabs tab-strip actions", () => {
  it("Close all emits closeMany with every tab", async () => {
    const { wrapper, sessions } = render(2);
    await wrapper
      .get('[data-testid="tab-strip-ctx-trigger"]')
      .trigger("contextmenu");
    item("tab-strip-ctx-close-all").click();
    expect(wrapper.emitted("closeMany")![0]).toEqual([
      sessions.map((s) => s.id),
    ]);
  });
});
