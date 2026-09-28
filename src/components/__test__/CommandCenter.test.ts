import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import CommandCenter from "../CommandCenter.vue";
import { createSession } from "@/lib/session";
import type { RequestGroup } from "@/lib/groups";

const groups: RequestGroup[] = [
  { id: 1, name: "Platform", parentId: null, collapsed: false },
];
function sessions() {
  const users = createSession();
  users.draft.url = "https://api.example.test/users";
  users.groupId = 1;
  const health = createSession();
  health.draft.url = "https://api.example.test/health";
  return [users, health];
}

function render() {
  const list = sessions();
  const wrapper = mount(CommandCenter, {
    props: { sessions: list, groups },
    attachTo: document.body,
  });
  return { wrapper, list };
}

describe("CommandCenter", () => {
  it("opens from the trigger and focuses the search input", async () => {
    const { wrapper } = render();
    await wrapper.get("[data-command-center-trigger]").trigger("click");
    const input = wrapper.get('[role="combobox"]');
    expect(document.activeElement).toBe(input.element);
    expect(wrapper.findAll('[role="option"]')).toHaveLength(2);
    wrapper.unmount();
  });

  it("moves with arrow keys and selects with Enter", async () => {
    const { wrapper, list } = render();
    await (wrapper.vm as unknown as { show(): Promise<void> }).show();
    const input = wrapper.get('[role="combobox"]');
    await input.trigger("keydown", { key: "ArrowDown" });
    expect(input.attributes("aria-activedescendant")).toBe(
      `command-center-option-${list[1].id}`,
    );
    await input.trigger("keydown", { key: "ArrowDown" });
    expect(input.attributes("aria-activedescendant")).toBe(
      `command-center-option-${list[0].id}`,
    );
    await input.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("select")).toEqual([[list[0].id]]);
    expect(wrapper.find('[role="combobox"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("filters by group, shows an empty result, and closes on Escape", async () => {
    const { wrapper, list } = render();
    await wrapper.get("[data-command-center-trigger]").trigger("click");
    const input = wrapper.get('[role="combobox"]');
    await input.setValue("platform");
    expect(wrapper.findAll('[role="option"]')).toHaveLength(1);
    expect(wrapper.get('[role="option"]').text()).toContain("Platform");
    await input.setValue("zzz");
    expect(wrapper.text()).toContain("No matching requests");
    await input.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("select")).toBeUndefined();
    await input.trigger("keydown", { key: "Escape" });
    expect(wrapper.find('[role="combobox"]').exists()).toBe(false);
    await wrapper.vm.$nextTick();
    expect(document.activeElement).toBe(
      wrapper.get("[data-command-center-trigger]").element,
    );
    expect(list).toHaveLength(2);
    wrapper.unmount();
  });
});
