import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import ResponsePanel from "../ResponsePanel.vue";

describe("large JSON inspection", () => {
  it("explains disabled highlighting while keeping formatting and raw data available", async () => {
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
      expect(panel.text()).toContain("HIGHLIGHT OFF");
      expect(panel.get("[data-response-body]").text()).toContain('"value": "');
      const pretty = panel
        .findAll("button")
        .find((button) => button.text() === "Pretty")!;
      await pretty.trigger("click");
      expect(panel.text()).not.toContain("HIGHLIGHT OFF");
      expect(panel.get("[data-response-body]").text()).toBe(body);
    } finally {
      panel.unmount();
    }
  });
});
