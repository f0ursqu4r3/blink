import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import ApplicationSettingsDialog from "../ApplicationSettingsDialog.vue";

describe("ApplicationSettingsDialog", () => {
  it("moves token syntax help into a labeled tooltip trigger", async () => {
    const wrapper = mount(ApplicationSettingsDialog, {
      props: { definitions: {}, open: true },
      attachTo: document.body,
      global: {
        stubs: {
          DialogPortal: { template: "<slot />" },
          TooltipPortal: { template: "<slot />" },
        },
      },
    });
    await nextTick();

    const help = wrapper.get('[data-token-help="global"]');
    expect(help.attributes("aria-label")).toBe("Token syntax help");
    await help.trigger("focus");
    expect(wrapper.text()).toContain("{{_.name}}");
    expect(wrapper.find(".help-text").exists()).toBe(false);
  });

  it("edits workspace-global definitions as a JSON object", async () => {
    const wrapper = mount(ApplicationSettingsDialog, {
      props: {
        definitions: { apiHost: "api.example.test" },
        open: true,
      },
      attachTo: document.body,
      global: {
        stubs: {
          DialogPortal: { template: "<slot />" },
          TooltipPortal: { template: "<slot />" },
        },
      },
    });
    await nextTick();

    const editor = wrapper.get<HTMLTextAreaElement>("[data-global-token-json]");
    expect(editor.element.value).toContain('"apiHost"');
    await editor.setValue('{"apiHost":"api.internal.test"}');
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.emitted("save")?.[0]).toEqual([
      { apiHost: "api.internal.test" },
      expect.objectContaining({ defaultMethod: "GET" }),
    ]);
  });

  it("rejects non-string token values without emitting a save", async () => {
    const wrapper = mount(ApplicationSettingsDialog, {
      props: { definitions: {}, open: true },
      attachTo: document.body,
      global: {
        stubs: {
          DialogPortal: { template: "<slot />" },
          TooltipPortal: { template: "<slot />" },
        },
      },
    });
    await nextTick();

    await wrapper.get("[data-global-token-json]").setValue('{"port":443}');
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.get("[data-token-json-error]").text()).toContain(
      "string values",
    );
    expect(wrapper.emitted("save")).toBeFalsy();
  });
});
