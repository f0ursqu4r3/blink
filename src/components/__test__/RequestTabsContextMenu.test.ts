import { afterEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import RequestTabs from "../RequestTabs.vue";
import { createSession } from "@/lib/session";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  document.body.innerHTML = "";
});

function render(count = 2) {
  const sessions = Array.from({ length: count }, () => createSession());
  const wrapper = mount(RequestTabs, {
    props: { sessions, activeId: sessions[0].id },
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

  it("'Duplicate active' emits duplicate", async () => {
    const { wrapper } = render();
    await wrapper
      .get('[data-testid="tab-strip-ctx-trigger"]')
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="tab-strip-ctx-duplicate"]',
      ) as HTMLElement
    ).click();
    expect(wrapper.emitted("duplicate")).toBeTruthy();
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
        `[data-testid="tab-ctx-select-${sessions[0].id}"]`,
      ),
    ).not.toBeNull();
  });

  it("'Select' emits select with session id", async () => {
    const { wrapper, sessions } = render();
    await wrapper
      .get(`[data-testid="tab-ctx-trigger-${sessions[0].id}"]`)
      .trigger("contextmenu");
    (
      document.body.querySelector(
        `[data-testid="tab-ctx-select-${sessions[0].id}"]`,
      ) as HTMLElement
    ).click();
    expect(wrapper.emitted("select")).toBeTruthy();
    expect(wrapper.emitted("select")![0]).toEqual([sessions[0].id]);
  });

  it("'Duplicate' emits select then duplicate", async () => {
    const { wrapper, sessions } = render();
    await wrapper
      .get(`[data-testid="tab-ctx-trigger-${sessions[0].id}"]`)
      .trigger("contextmenu");
    (
      document.body.querySelector(
        `[data-testid="tab-ctx-duplicate-${sessions[0].id}"]`,
      ) as HTMLElement
    ).click();
    expect(wrapper.emitted("select")).toBeTruthy();
    expect(wrapper.emitted("duplicate")).toBeTruthy();
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
