import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import ResponsePanel from "../ResponsePanel.vue";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => wrappers.splice(0).forEach((w) => w.unmount()));

const jsonResponse = {
  status: 200,
  statusText: "OK",
  durationMs: 1,
  sizeBytes: 20,
  headers: [
    { key: "content-type", value: "application/json" },
    { key: "x-request-id", value: "abc123" },
  ],
  body: '{"a":1}',
};

function makePanel(response = jsonResponse) {
  const wrapper = mount(ResponsePanel, {
    attachTo: document.body,
    props: { busy: false, error: "", elapsed: 0, response },
  });
  wrappers.push(wrapper);
  return wrapper;
}

let written: string[] = [];
beforeEach(() => {
  written = [];
  vi.stubGlobal("navigator", {
    clipboard: {
      writeText: vi.fn((t: string) => {
        written.push(t);
        return Promise.resolve();
      }),
    },
    platform: "MacIntel",
  });
});

describe("ResponsePanel context menu – response toolbar", () => {
  it("right-clicking the response toolbar opens a context menu", async () => {
    const wrapper = makePanel();
    await wrapper.get(".response-toolbar").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-copy-response']"),
    ).not.toBeNull();
  });

  it("toolbar menu has Pretty/Raw toggle when body tab active and JSON parsed", async () => {
    const wrapper = makePanel();
    await wrapper.get(".response-toolbar").trigger("contextmenu");
    const prettyItem = document.body.querySelector(
      "[data-testid='ctx-toolbar-pretty']",
    ) as HTMLElement;
    expect(prettyItem).not.toBeNull();
  });

  it("toolbar menu has Wrap item when body tab active", async () => {
    const wrapper = makePanel();
    await wrapper.get(".response-toolbar").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-toolbar-wrap']"),
    ).not.toBeNull();
  });

  it("toolbar menu has Find item when body tab active", async () => {
    const wrapper = makePanel();
    await wrapper.get(".response-toolbar").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-toolbar-find']"),
    ).not.toBeNull();
  });

  it("toolbar Copy calls clipboard", async () => {
    const wrapper = makePanel();
    await wrapper.get(".response-toolbar").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-copy-response']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(written.length).toBeGreaterThan(0);
  });

  it("toolbar Pretty toggle switches pretty mode", async () => {
    const wrapper = makePanel();
    // Default pretty=true; ctx item shows "Pretty" (the current state label)
    // Clicking it toggles to pretty=false, button label changes to "Raw"
    await wrapper.get(".response-toolbar").trigger("contextmenu");
    const item = document.body.querySelector(
      "[data-testid='ctx-toolbar-pretty']",
    ) as HTMLElement;
    // Item label reflects current state: "Pretty" when pretty=true
    expect(item.textContent?.trim()).toBe("Pretty");
    item.click();
    await nextTick();
    // After toggle, pretty=false, toolbar button should show "Raw"
    const toolbar = wrapper.find(".response-toolbar");
    expect(toolbar.text()).toContain("Raw");
  });
});

describe("ResponsePanel context menu – JSON rows", () => {
  it("right-clicking a JSON row opens its value actions", async () => {
    const wrapper = makePanel();
    await wrapper.get(".tree-row").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-copy-path']"),
    ).not.toBeNull();
    expect(
      document.body.querySelector("[data-testid='ctx-copy-value']"),
    ).not.toBeNull();
  });

  it("container rows offer a collapse action", async () => {
    const wrapper = makePanel();
    await wrapper.get(".tree-row").trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-toggle']"),
    ).not.toBeNull();
  });

  it("Copy value writes to the clipboard", async () => {
    const wrapper = makePanel();
    await wrapper.get(".tree-row").trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-copy-value']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(written.length).toBeGreaterThan(0);
  });
});

describe("ResponsePanel context menu – header rows", () => {
  it("right-clicking a header row shows Copy name, Copy value, Copy name: value", async () => {
    const wrapper = makePanel();
    // Switch to headers tab first
    const headerTab = wrapper.get("[data-response-headers]");
    await headerTab.trigger("mousedown", { button: 0, ctrlKey: false });
    await nextTick();
    const rows = wrapper.findAll("tr").filter((r) => r.find("td").exists());
    expect(rows.length).toBeGreaterThan(0);
    await rows[0].trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='ctx-header-copy-name']"),
    ).not.toBeNull();
    expect(
      document.body.querySelector("[data-testid='ctx-header-copy-value']"),
    ).not.toBeNull();
    expect(
      document.body.querySelector("[data-testid='ctx-header-copy-pair']"),
    ).not.toBeNull();
  });

  it("Copy name writes the header key to clipboard", async () => {
    const wrapper = makePanel();
    const headerTab = wrapper.get("[data-response-headers]");
    await headerTab.trigger("mousedown", { button: 0, ctrlKey: false });
    await nextTick();
    const rows = wrapper.findAll("tr").filter((r) => r.find("td").exists());
    expect(rows.length).toBeGreaterThan(0);
    await rows[0].trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-header-copy-name']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(written).toContain("content-type");
  });

  it("Copy value writes the header value to clipboard", async () => {
    const wrapper = makePanel();
    const headerTab = wrapper.get("[data-response-headers]");
    await headerTab.trigger("mousedown", { button: 0, ctrlKey: false });
    await nextTick();
    const rows = wrapper.findAll("tr").filter((r) => r.find("td").exists());
    expect(rows.length).toBeGreaterThan(0);
    await rows[0].trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-header-copy-value']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(written).toContain("application/json");
  });

  it("Copy name: value writes the full header pair to clipboard", async () => {
    const wrapper = makePanel();
    const headerTab = wrapper.get("[data-response-headers]");
    await headerTab.trigger("mousedown", { button: 0, ctrlKey: false });
    await nextTick();
    const rows = wrapper.findAll("tr").filter((r) => r.find("td").exists());
    expect(rows.length).toBeGreaterThan(0);
    await rows[0].trigger("contextmenu");
    (
      document.body.querySelector(
        "[data-testid='ctx-header-copy-pair']",
      ) as HTMLElement
    ).click();
    await nextTick();
    expect(written).toContain("content-type: application/json");
  });
});
