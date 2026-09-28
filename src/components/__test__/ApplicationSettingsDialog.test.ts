import { beforeEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import ApplicationSettingsDialog from "../ApplicationSettingsDialog.vue";
import { useTheme } from "@/composables/useTheme";
import { THEME_KEY } from "@/lib/theme";

beforeEach(() => {
  localStorage.clear();
  useTheme().reload();
});

function renderOpen() {
  return mount(ApplicationSettingsDialog, {
    props: { definitions: {}, open: true },
    attachTo: document.body,
    global: {
      stubs: {
        DialogPortal: { template: "<slot />" },
        TooltipPortal: { template: "<slot />" },
      },
    },
  });
}
const paper = "background = #ffffff\nforeground = #111111";

describe("ApplicationSettingsDialog", () => {
  it("moves token syntax help into a labeled tooltip trigger", async () => {
    const wrapper = renderOpen();
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
    const wrapper = renderOpen();
    await nextTick();

    await wrapper.get("[data-global-token-json]").setValue('{"port":443}');
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.get("[data-token-json-error]").text()).toContain(
      "string values",
    );
    expect(wrapper.emitted("save")).toBeFalsy();
  });

  it("restores the saved theme when closed without saving", async () => {
    const wrapper = renderOpen();
    await nextTick();
    await wrapper.get("[data-theme-colors]").setValue(paper);
    expect(document.documentElement.dataset.theme).toBe("ghostty");

    const cancel = wrapper
      .findAll("button")
      .find((b) => b.text() === "Cancel")!;
    await cancel.trigger("click");
    const openEvents = wrapper.emitted("update:open") ?? [];
    expect(openEvents[openEvents.length - 1]).toEqual([false]);
    await wrapper.setProps({ open: false });

    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(localStorage.getItem(THEME_KEY)).toBeNull();
    wrapper.unmount();
  });

  it("stores the theme on save", async () => {
    const wrapper = renderOpen();
    await nextTick();
    await wrapper.get("[data-theme-colors]").setValue(paper);
    await wrapper.get("[data-save-application-settings]").trigger("click");
    await wrapper.setProps({ open: false });

    expect(JSON.parse(localStorage.getItem(THEME_KEY)!)).toMatchObject({
      name: "Custom",
      text: paper,
      accent: 3,
    });
    expect(document.documentElement.dataset.theme).toBe("ghostty");
    wrapper.unmount();
  });

  it("does not save while the Ghostty text is invalid", async () => {
    const wrapper = renderOpen();
    await nextTick();
    await wrapper.get("[data-theme-colors]").setValue("foreground = white");
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.emitted("save")).toBeUndefined();
    expect(wrapper.get("[data-theme-error]").text()).toContain("line 1");
    wrapper.unmount();
  });
});
