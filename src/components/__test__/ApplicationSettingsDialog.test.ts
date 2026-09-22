import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import ApplicationSettingsDialog from "../ApplicationSettingsDialog.vue";

describe("ApplicationSettingsDialog", () => {
  it("edits workspace-global definitions as a JSON object", async () => {
    const wrapper = mount(ApplicationSettingsDialog, {
      props: {
        definitions: { apiHost: "api.example.test" },
        open: true,
      },
      attachTo: document.body,
    });

    const editor = wrapper.get<HTMLTextAreaElement>("[data-global-token-json]");
    expect(editor.element.value).toContain('"apiHost"');
    await editor.setValue('{"apiHost":"api.internal.test"}');
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.emitted("save")?.[0]).toEqual([
      { apiHost: "api.internal.test" },
    ]);
  });

  it("rejects non-string token values without emitting a save", async () => {
    const wrapper = mount(ApplicationSettingsDialog, {
      props: { definitions: {}, open: true },
      attachTo: document.body,
    });

    await wrapper.get("[data-global-token-json]").setValue('{"port":443}');
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.get("[data-token-json-error]").text()).toContain(
      "string values",
    );
    expect(wrapper.emitted("save")).toBeFalsy();
  });
});
