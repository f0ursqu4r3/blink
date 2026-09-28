import { describe, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import ResponsePanel from "../ResponsePanel.vue";
vi.mock("@/lib/response-body", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/response-body")>()),
  saveResponse: vi.fn(),
}));
import { saveResponse } from "@/lib/response-body";
import type { ApiResponse } from "@/lib/request";

describe("large JSON inspection", () => {
  it("keeps large JSON available in structured and raw views", async () => {
    const body = JSON.stringify({ value: "x".repeat(64_001) });
    const panel = mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: {
          status: 200,
          statusText: "OK",
          durationMs: 1,
          sizeBytes: body.length,
          headers: [],
          body,
        },
      },
    });
    try {
      expect(panel.get("[data-response-body]").text()).toContain('"value": "');
      const pretty = panel
        .findAll("button")
        .find((button) => button.text() === "Pretty")!;
      await pretty.trigger("click");
      expect(panel.get("[data-response-body]").text()).toBe(body);
    } finally {
      panel.unmount();
    }
  });

  it("filters a collapsible JSON response tree", async () => {
    const panel = mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: {
          status: 200,
          statusText: "OK",
          durationMs: 1,
          sizeBytes: 72,
          headers: [{ key: "content-type", value: "application/json" }],
          body: JSON.stringify({
            profile: { name: "Grace", role: "admin" },
            active: true,
          }),
        },
      },
    });
    try {
      expect(panel.get("[data-json-tree]").text()).toContain("Grace");
      await panel.get('[aria-label="Collapse profile"]').trigger("click");
      expect(panel.text()).not.toContain("Grace");
      await panel.get('[aria-label="Expand profile"]').trigger("click");
      await panel.get('[aria-label="Find response"]').trigger("click");
      await panel.get("[data-response-search]").setValue("Grace");
      expect(panel.get("[data-json-tree]").text()).toContain("Grace");
      expect(panel.get("[data-json-tree]").text()).not.toContain("active");
    } finally {
      panel.unmount();
    }
  });

  it("selects response syntax highlighting from Content-Type", () => {
    const panel = mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: {
          status: 200,
          statusText: "OK",
          durationMs: 1,
          sizeBytes: 24,
          headers: [{ key: "content-type", value: "text/html; charset=utf-8" }],
          body: "<main>Hello</main>",
        },
      },
    });
    try {
      expect(
        panel.get("[data-response-body]").attributes("data-language"),
      ).toBe("xml");
    } finally {
      panel.unmount();
    }
  });

  it("runs jq queries against JSON responses", async () => {
    const panel = mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: {
          status: 200,
          statusText: "OK",
          durationMs: 1,
          sizeBytes: 28,
          headers: [{ key: "content-type", value: "application/json" }],
          body: '{"profiles":[{"name":"Grace"}]}',
        },
      },
    });
    try {
      await panel.get('[aria-label="Find response"]').trigger("click");
      await panel.get("[data-response-jq]").setValue(".profiles[0].name");
      await panel.get("[data-run-jq]").trigger("click");
      await vi.waitFor(() =>
        expect(panel.get("[data-response-body]").text()).toContain("Grace"),
      );
    } finally {
      panel.unmount();
    }
  });

  it("keeps response tools hidden until the find control opens them", async () => {
    const panel = mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: {
          status: 200,
          statusText: "OK",
          durationMs: 1,
          sizeBytes: 17,
          headers: [{ key: "content-type", value: "application/json" }],
          body: '{"name":"Grace"}',
        },
      },
    });
    try {
      expect(panel.find("[data-response-search]").exists()).toBe(false);
      await panel.get('[aria-label="Find response"]').trigger("click");
      expect(panel.get("[data-response-search]")).toBeTruthy();
      await panel.get('[aria-label="Find response"]').trigger("click");
      expect(panel.find("[data-response-search]").exists()).toBe(false);
    } finally {
      panel.unmount();
    }
  });

  it("wraps long JSON values in the structured view", async () => {
    const panel = mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: {
          status: 200,
          statusText: "OK",
          durationMs: 1,
          sizeBytes: 100,
          headers: [{ key: "content-type", value: "application/json" }],
          body: JSON.stringify({ description: "x".repeat(80) }),
        },
      },
    });
    try {
      await panel.get('[aria-label="Wrap lines"]').trigger("click");
      expect(panel.get("[data-json-tree]").classes()).toContain("wrapped");
    } finally {
      panel.unmount();
    }
  });
});

describe("stored bodies", () => {
  const response = (extra: Partial<ApiResponse> = {}): ApiResponse => ({
    status: 200,
    statusText: "OK",
    durationMs: 1,
    sizeBytes: 3,
    headers: [{ key: "Content-Type", value: "image/png" }],
    body: "abc",
    ...extra,
  });
  const render = (value: ApiResponse) =>
    mount(ResponsePanel, {
      props: {
        busy: false,
        error: "",
        elapsed: 0,
        response: value,
        requestUrl: "https://x.test/logo.png",
      },
    });

  it("shows a binary notice in place of the editor", () => {
    const panel = render(
      response({ binary: true, body: "", sizeBytes: 2048, bodyId: "b" }),
    );
    expect(panel.get("[data-response-binary]").text()).toContain(
      "Binary response · 2.0 KiB · image/png",
    );
    expect(panel.find("[data-response-body]").exists()).toBe(false);
    expect(panel.find('[aria-label="Wrap lines"]').exists()).toBe(false);
    panel.unmount();
  });

  it("marks a truncated preview and disables formatting", () => {
    const panel = render(
      response({
        truncated: true,
        body: '{"a":1',
        sizeBytes: 5 * 1024 * 1024,
        bodyId: "b",
        headers: [{ key: "Content-Type", value: "application/json" }],
      }),
    );
    expect(panel.get("[data-response-truncated]").text()).toContain(
      "Preview shows the first 6 B of 5.00 MiB.",
    );
    const pretty = panel.findAll("button").find((b) => b.text() === "Raw")!;
    expect(pretty.attributes("disabled")).toBeDefined();
    expect(pretty.attributes("title")).toBe(
      "Unavailable for truncated responses",
    );
    panel.unmount();
  });

  it("shows a restored-preview notice when a truncated response survived a restart", () => {
    const panel = render(
      response({
        truncated: true,
        body: "",
        sizeBytes: 5 * 1024 * 1024,
        headers: [{ key: "Content-Type", value: "application/json" }],
      }),
    );
    expect(panel.get("[data-response-truncated]").text()).toBe(
      "Preview is not kept after a restart. Send the request again to inspect it.",
    );
    expect(
      panel.findAll("button").some((button) => button.text() === "Save…"),
    ).toBe(false);
    expect(panel.find("[data-response-body]").exists()).toBe(false);
    expect(panel.text()).not.toContain("Empty response body.");
    panel.unmount();
  });

  it("disables Save when the stored body is gone", () => {
    const panel = render(response({ truncated: true }));
    const save = panel.get("[data-save-response]");
    expect(save.attributes("disabled")).toBeDefined();
    expect(save.attributes("title")).toBe(
      "Body is no longer available. Send the request again.",
    );
    panel.unmount();
  });

  it("saves and reports a failure inline", async () => {
    vi.mocked(saveResponse).mockRejectedValueOnce("disk full");
    const value = response({ bodyId: "b" });
    const panel = render(value);
    await panel.get("[data-save-response]").trigger("click");
    await flushPromises();
    expect(vi.mocked(saveResponse)).toHaveBeenCalledWith(
      value,
      "https://x.test/logo.png",
    );
    expect(panel.get("[data-save-error]").text()).toBe(
      "Cannot save the file: disk full.",
    );
    panel.unmount();
  });

  it("shows nothing when the user cancels", async () => {
    vi.mocked(saveResponse).mockResolvedValueOnce(false);
    const panel = render(response({ bodyId: "b" }));
    await panel.get("[data-save-response]").trigger("click");
    await flushPromises();
    expect(panel.find("[data-save-error]").exists()).toBe(false);
    panel.unmount();
  });

  it("shows the redirect target", () => {
    const desktop = render(
      response({ finalUrl: "https://final.test/", redirectCount: 2 }),
    );
    expect(desktop.get("[data-response-redirect]").text()).toBe(
      "→ https://final.test/ · 2 redirects",
    );
    desktop.unmount();
    const browser = render(response({ finalUrl: "https://final.test/" }));
    expect(browser.get("[data-response-redirect]").text()).toBe(
      "→ https://final.test/ · redirected",
    );
    browser.unmount();
  });

  it("uses the configured timeout while waiting", () => {
    const panel = mount(ResponsePanel, {
      props: {
        busy: true,
        error: "",
        elapsed: 1500,
        response: null,
        timeoutSeconds: 90,
      },
    });
    expect(panel.text()).toContain("1.5 s elapsed · 90 s timeout");
    panel.unmount();
  });
});
