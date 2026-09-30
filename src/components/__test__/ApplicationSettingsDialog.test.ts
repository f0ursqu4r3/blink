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

  it("edits workspace-global definitions as key-value rows", async () => {
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

    const tokens = wrapper.get("[data-global-tokens]");
    const name = tokens.get<HTMLInputElement>(
      '[aria-label="Global tokens name 1"]',
    );
    expect(name.element.value).toBe("apiHost");
    expect(tokens.find('input[type="checkbox"]').exists()).toBe(false);
    await tokens
      .get('[aria-label="Global tokens value 1"]')
      .setValue("api.internal.test");
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.emitted("save")?.[0]).toEqual([
      { apiHost: "api.internal.test" },
      expect.objectContaining({ defaultMethod: "GET" }),
    ]);
  });

  it("rejects reserved token names without emitting a save", async () => {
    const wrapper = renderOpen();
    await nextTick();

    await wrapper.get('[aria-label="Global tokens name 1"]').setValue("_port");
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.get("[data-token-error]").text()).toContain(
      "must not start with _",
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
      accent: 4,
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

  describe("request settings", () => {
    it("saves transport settings", async () => {
      const wrapper = renderOpen();
      await nextTick();
      await wrapper.get("#app-timeout").setValue("90");
      await wrapper.get("#app-connect-timeout").setValue("5");
      await wrapper.get("#app-follow-redirects").setValue(true);
      await wrapper.get("#app-max-redirects").setValue("3");
      await wrapper.get("#app-inspection-limit").setValue("8");
      await wrapper.get("form").trigger("submit");
      const [, preferences] = wrapper.emitted("save")![0] as [unknown, object];
      expect(preferences).toMatchObject({
        timeoutSeconds: 90,
        connectTimeoutSeconds: 5,
        followRedirects: true,
        maxRedirects: 3,
        inspectionLimitMiB: 8,
      });
      wrapper.unmount();
    });

    it("disables max redirects while redirects are not followed", async () => {
      const wrapper = renderOpen();
      await nextTick();
      expect(
        (wrapper.get("#app-max-redirects").element as HTMLInputElement)
          .disabled,
      ).toBe(true);
      await wrapper.get("#app-follow-redirects").setValue(true);
      expect(
        (wrapper.get("#app-max-redirects").element as HTMLInputElement)
          .disabled,
      ).toBe(false);
      wrapper.unmount();
    });

    it("resets an invalid max redirects to the default when redirects are off", async () => {
      const wrapper = renderOpen();
      await nextTick();
      await wrapper.get("#app-follow-redirects").setValue(true);
      await wrapper.get("#app-max-redirects").setValue("0");
      await wrapper.get("#app-follow-redirects").setValue(false);
      await wrapper.get("form").trigger("submit");
      const [, preferences] = wrapper.emitted("save")![0] as [unknown, object];
      expect(preferences).toMatchObject({
        followRedirects: false,
        maxRedirects: 10,
      });
      expect(wrapper.find("#app-max-redirects-error").exists()).toBe(false);
      wrapper.unmount();
    });

    it.each(["", "2.5", "0", "601"])(
      "blocks the save for timeout %j",
      async (value) => {
        const wrapper = renderOpen();
        await nextTick();
        await wrapper.get("#app-timeout").setValue(value);
        await wrapper.get("form").trigger("submit");
        expect(wrapper.emitted("save")).toBeUndefined();
        expect(wrapper.get("#app-timeout-error").text()).toBe(
          "Enter a whole number from 1 to 600.",
        );
        expect(wrapper.get("#app-timeout").attributes("aria-invalid")).toBe(
          "true",
        );
        wrapper.unmount();
      },
    );
  });
});
