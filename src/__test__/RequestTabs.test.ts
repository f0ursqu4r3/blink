import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import App from "../App.vue";

const wrappers: ReturnType<typeof mount>[] = [];
beforeEach(() => localStorage.clear());
function render() {
  const app = mount(App, { attachTo: document.body });
  wrappers.push(app);
  return app;
}
const pane = (app: ReturnType<typeof mount>) =>
  app.get('[data-request-pane][data-active="true"]');
const requestTabs = (app: ReturnType<typeof mount>) =>
  app.findAll('[role="tablist"][aria-label="Requests"] [role="tab"]');
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  vi.unstubAllGlobals();
});

describe("independent request tabs", () => {
  it("creates and switches tabs without losing draft or editor state", async () => {
    const app = render();
    await pane(app)
      .get("[data-request-url]")
      .setValue("https://example.test/first");
    await pane(app).get("[data-method]").setValue("POST");
    await app.get("[data-new-request]").trigger("click");
    expect(requestTabs(app)).toHaveLength(2);
    expect(
      pane(app).get<HTMLInputElement>("[data-request-url]").element.value,
    ).toBe("");
    await pane(app)
      .get("[data-request-url]")
      .setValue("https://example.test/second");
    await requestTabs(app)[0].trigger("click");
    expect(
      pane(app).get<HTMLInputElement>("[data-request-url]").element.value,
    ).toBe("https://example.test/first");
    expect(
      pane(app).get<HTMLSelectElement>("[data-method]").element.value,
    ).toBe("POST");
  });
  it("routes late responses to their owning tab, not the active tab", async () => {
    let resolve!: (response: Response) => void;
    vi.stubGlobal(
      "fetch",
      vi.fn(
        () =>
          new Promise<Response>((done) => {
            resolve = done;
          }),
      ),
    );
    const app = render();
    await pane(app)
      .get("[data-request-url]")
      .setValue("https://example.test/slow");
    await pane(app).get("[data-send]").trigger("click");
    await app.get("[data-new-request]").trigger("click");
    resolve(new Response('{"owner":"first"}', { status: 200 }));
    await flushPromises();
    expect(pane(app).find("[data-response-body]").exists()).toBe(false);
    await requestTabs(app)[0].trigger("click");
    expect(pane(app).get("[data-response-body]").text()).toContain("first");
  });
  it("duplicates drafts by value without sharing query rows", async () => {
    const app = render();
    await pane(app)
      .get("[data-request-url]")
      .setValue("https://example.test/source");
    await pane(app).get('[aria-label="Query name 1"]').setValue("original");
    await app.get("[data-duplicate-request]").trigger("click");
    expect(requestTabs(app)).toHaveLength(2);
    await pane(app).get('[aria-label="Query name 1"]').setValue("copy");
    await requestTabs(app)[0].trigger("click");
    expect(
      pane(app).get<HTMLInputElement>('[aria-label="Query name 1"]').element
        .value,
    ).toBe("original");
  });
  it("closing a tab keeps the request in the browser and reopens it intact", async () => {
    const app = render();
    await pane(app).get("[data-request-url]").setValue("https://example.test");
    const id = Number(
      app.get("[data-request-id]").attributes("data-request-id"),
    );
    await app.get("[data-close-request]").trigger("click");
    expect(requestTabs(app)).toHaveLength(0);
    expect(app.find("[data-no-open-requests]").exists()).toBe(true);
    expect(app.find(`[data-request-id="${id}"]`).exists()).toBe(true);
    await app.get(`[data-request-id="${id}"]`).trigger("click");
    expect(requestTabs(app)).toHaveLength(1);
    expect(
      pane(app).get<HTMLInputElement>("[data-request-url]").element.value,
    ).toBe("https://example.test");
  });
  it("closing an in-flight tab keeps the request running", async () => {
    let resolve!: (response: Response) => void;
    vi.stubGlobal(
      "fetch",
      vi.fn(
        () =>
          new Promise<Response>((done) => {
            resolve = done;
          }),
      ),
    );
    const app = render();
    await pane(app).get("[data-request-url]").setValue("https://example.test");
    await pane(app).get("[data-send]").trigger("click");
    await app.get("[data-close-request]").trigger("click");
    expect(requestTabs(app)).toHaveLength(0);
    resolve(new Response("done"));
    await flushPromises();
    await app.get("[data-request-id]").trigger("click");
    expect(pane(app).get("[data-response-body]").text()).toContain("done");
  });
});
