import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import App from "../App.vue";
beforeEach(() => localStorage.clear());

const wrappers: ReturnType<typeof mount>[] = [];
function render() {
  const wrapper = mount(App, { attachTo: document.body });
  wrappers.push(wrapper);
  return wrapper;
}
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  vi.unstubAllGlobals();
});

describe("compact request console", () => {
  it("starts blank without fake navigation or network traffic", () => {
    const fetch = vi.fn();
    vi.stubGlobal("fetch", fetch);
    const app = render();
    expect(app.get<HTMLInputElement>("[data-request-url]").element.value).toBe(
      "",
    );
    expect(app.text()).not.toContain("Session activity");
    expect(app.text()).not.toContain("Settings");
    expect(app.get("[data-send]").attributes("disabled")).toBeDefined();
    expect(fetch).not.toHaveBeenCalled();
  });

  it("organizes the active request in a browser group", async () => {
    const app = render();

    await app.get('[aria-label="Add top-level group"]').trigger("click");
    await app.get('[aria-label="Top-level group name"]').setValue("Platform");
    await app.get(".top-level-form").trigger("submit");
    await app
      .get('[aria-label="Move active request to Platform"]')
      .trigger("click");

    expect(
      app
        .get('[aria-label="Move active request to Platform"]')
        .attributes("disabled"),
    ).toBeDefined();
  });

  it("collapses and restores the request browser", async () => {
    const app = render();
    const toggle = app.get('[aria-label="Collapse request browser"]');

    expect(toggle.attributes("aria-expanded")).toBe("true");
    await toggle.trigger("click");

    expect(app.get('[aria-label="Expand request browser"]')).toBeTruthy();
    expect(app.get("[data-request-browser]").attributes("data-collapsed")).toBe(
      "true",
    );
  });

  it("keeps the shell free of promotional labels", () => {
    const app = render();

    expect(app.text()).not.toContain("HTTP OPERATIONS");
    expect(app.text()).not.toContain("BROWSER PREVIEW");
    expect(app.text()).not.toContain("Enter an endpoint. Send. Inspect.");
  });

  it("sends a GET without a body and exposes actual response headers", async () => {
    const fetch = vi.fn().mockResolvedValue(
      new Response('{"ok":true}', {
        status: 200,
        headers: {
          "content-type": "application/json",
          "x-request-id": "test-42",
        },
      }),
    );
    vi.stubGlobal("fetch", fetch);
    const app = render();
    await app.get("[data-request-url]").setValue("https://example.test/check");
    await app.get("[data-send]").trigger("click");
    await flushPromises();
    expect(fetch).toHaveBeenCalledOnce();
    expect(fetch.mock.calls[0][1].body).toBeNull();
    expect(app.get("[data-response-body]").text()).toContain('"ok": true');
    await app
      .get("[data-response-headers]")
      .trigger("mousedown", { button: 0, ctrlKey: false });
    expect(app.text()).toContain("test-42");
  });

  it("renders HTTP failures as responses, not transport failures", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          new Response("Denied", { status: 401, statusText: "Unauthorized" }),
        ),
    );
    const app = render();
    await app.get("[data-request-url]").setValue("https://example.test");
    await app.get("[data-send]").trigger("click");
    await flushPromises();
    expect(app.get("[data-response-status]").attributes("data-tone")).toBe(
      "error",
    );
    expect(app.get("[data-response-body]").text()).toContain("Denied");
  });

  it("locks the draft during a request and handles an empty response", async () => {
    let resolve!: (response: Response) => void;
    const fetch = vi.fn(
      () =>
        new Promise<Response>((done) => {
          resolve = done;
        }),
    );
    vi.stubGlobal("fetch", fetch);
    const app = render();
    await app.get("[data-request-url]").setValue("https://example.test");
    await app.get("[data-send]").trigger("click");
    expect(app.get("[data-request-url]").attributes("disabled")).toBeDefined();
    expect(app.get("[data-send]").attributes("disabled")).toBeDefined();
    await app.get(".request-bar").trigger("submit");
    expect(fetch).toHaveBeenCalledOnce();
    resolve(new Response(null, { status: 204, statusText: "No Content" }));
    await flushPromises();
    expect(app.text()).toContain("Empty response body");
    expect(
      app.get("[data-request-url]").attributes("disabled"),
    ).toBeUndefined();
  });
  it("reports clipboard failures rather than claiming a copy succeeded", async () => {
    vi.stubGlobal("navigator", {
      ...navigator,
      clipboard: { writeText: vi.fn().mockRejectedValue(new Error("Denied")) },
    });
    const app = render();
    await app.get("[data-request-url]").setValue("https://example.test");
    await app.get(".curl-button").trigger("click");
    await app.get("[data-curl-preview] button").trigger("click");
    await flushPromises();
    expect(app.get('[role="alert"]').text()).toContain("Clipboard unavailable");
    expect(app.get("[data-curl-preview] button").text()).not.toContain(
      "Copied",
    );
  });
  it("keeps request errors visible and supports retry", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockRejectedValue(new Error("Connection refused")),
    );
    const app = render();
    await app.get("[data-request-url]").setValue("http://localhost:9876");
    await app.get("[data-send]").trigger("click");
    await flushPromises();
    expect(app.get('[role="alert"]').text()).toContain("Connection refused");
    expect(app.get("[data-send]").attributes("disabled")).toBeUndefined();
  });
});
