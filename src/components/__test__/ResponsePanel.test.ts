import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import ResponsePanel from "../ResponsePanel.vue";

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
