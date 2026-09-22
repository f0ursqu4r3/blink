import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import RequestWorkspace from "../RequestWorkspace.vue";
import { createSession } from "@/lib/session";

vi.stubGlobal(
  "fetch",
  vi.fn(() => new Promise(() => {})),
);

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => wrappers.splice(0).forEach((w) => w.unmount()));

function makeWorkspace() {
  const session = createSession();
  const wrapper = mount(RequestWorkspace, {
    attachTo: document.body,
    props: { active: true, session },
  });
  wrappers.push(wrapper);
  return { wrapper, session };
}

describe("RequestWorkspace context menu – request bar", () => {
  it("right-clicking the request bar opens the context menu", async () => {
    const { wrapper } = makeWorkspace();
    await wrapper.get(".request-bar").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-focus-url']"),
    ).not.toBeNull();
  });

  it("has a 'Send' menu item on the request bar", async () => {
    const { wrapper } = makeWorkspace();
    await wrapper.get(".request-bar").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-send']"),
    ).not.toBeNull();
  });

  it("has a 'Focus URL' menu item on the request bar", async () => {
    const { wrapper } = makeWorkspace();
    await wrapper.get(".request-bar").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-focus-url']"),
    ).not.toBeNull();
  });

  it("has a 'cURL' toggle menu item on the request bar", async () => {
    const { wrapper } = makeWorkspace();
    await wrapper.get(".request-bar").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-show-curl']"),
    ).not.toBeNull();
  });

  it("clicking 'cURL' shows the curl preview section", async () => {
    const { wrapper } = makeWorkspace();
    // Give the session a valid URL so curl is generated
    wrapper.vm.session.draft.url = "https://example.com";
    await nextTick();
    await wrapper.get(".request-bar").trigger("contextmenu");
    const item = document.body.querySelector(
      "[data-testid='ctx-show-curl']",
    ) as HTMLElement;
    expect(item).not.toBeNull();
    item.click();
    await nextTick();
    expect(wrapper.find("[data-curl-preview]").exists()).toBe(true);
  });

  it("clicking 'cURL' a second time hides the curl preview", async () => {
    const { wrapper } = makeWorkspace();
    wrapper.vm.session.draft.url = "https://example.com";
    await nextTick();
    // Open via context menu
    await wrapper.get(".request-bar").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-show-curl']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(wrapper.find("[data-curl-preview]").exists()).toBe(true);
    // Toggle again
    await wrapper.get(".request-bar").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-show-curl']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(wrapper.find("[data-curl-preview]").exists()).toBe(false);
  });
});

describe("RequestWorkspace context menu – cURL preview", () => {
  it("right-clicking the curl preview section shows Copy cURL and Close items", async () => {
    const { wrapper } = makeWorkspace();
    wrapper.vm.session.draft.url = "https://example.com";
    await nextTick();
    // Show curl preview first
    await wrapper.get(".request-bar").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-show-curl']",
      ) as HTMLElement
    ).click();
    await nextTick();
    // Right-click the preview
    await wrapper.get("[data-curl-preview]").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-copy-curl']"),
    ).not.toBeNull();
    expect(
      document.body.querySelector("[data-testid='ctx-close-curl']"),
    ).not.toBeNull();
  });

  it("clicking 'Close' in the curl preview context menu hides the preview", async () => {
    const { wrapper } = makeWorkspace();
    wrapper.vm.session.draft.url = "https://example.com";
    await nextTick();
    await wrapper.get(".request-bar").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-show-curl']",
      ) as HTMLElement
    ).click();
    await nextTick();
    await wrapper.get("[data-curl-preview]").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-close-curl']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(wrapper.find("[data-curl-preview]").exists()).toBe(false);
  });

  it("clicking 'Copy cURL' calls clipboard writeText", async () => {
    const written: string[] = [];
    vi.stubGlobal("navigator", {
      clipboard: {
        writeText: vi.fn((t: string) => {
          written.push(t);
          return Promise.resolve();
        }),
      },
      platform: "MacIntel",
    });
    const { wrapper } = makeWorkspace();
    wrapper.vm.session.draft.url = "https://example.com";
    await nextTick();
    await wrapper.get(".request-bar").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-show-curl']",
      ) as HTMLElement
    ).click();
    await nextTick();
    await wrapper.get("[data-curl-preview]").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-copy-curl']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(written.length).toBeGreaterThan(0);
  });
});
