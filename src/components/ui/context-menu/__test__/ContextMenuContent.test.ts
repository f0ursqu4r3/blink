import { afterEach, describe, expect, it } from "vitest";
import { mount, shallowMount } from "@vue/test-utils";
import { defineComponent } from "vue";
import ContextMenu from "../ContextMenu.vue";
import ContextMenuContent from "../ContextMenuContent.vue";
import ContextMenuSubContent from "../ContextMenuSubContent.vue";
import ContextMenuTrigger from "../ContextMenuTrigger.vue";

const wrappers: ReturnType<typeof mount>[] = [];

afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount());
  document.body.innerHTML = "";
});

describe("ContextMenuContent", () => {
  it("marks opened menu content as an opaque context-menu surface", async () => {
    const wrapper = mount(
      defineComponent({
        components: { ContextMenu, ContextMenuContent, ContextMenuTrigger },
        template: `
          <ContextMenu>
            <ContextMenuTrigger as-child>
              <button data-menu-trigger type="button">Open</button>
            </ContextMenuTrigger>
            <ContextMenuContent>Menu item</ContextMenuContent>
          </ContextMenu>
        `,
      }),
      {
        attachTo: document.body,
      },
    );
    wrappers.push(wrapper);

    await wrapper.get("[data-menu-trigger]").trigger("contextmenu");

    expect(
      document.body.querySelector('[data-surface="context-menu"]'),
    ).not.toBeNull();
  });

  it("marks submenu content as an opaque context-menu surface", () => {
    const wrapper = shallowMount(ContextMenuSubContent, {
      slots: { default: "Submenu item" },
    });
    wrappers.push(wrapper);

    expect(wrapper.get('[data-surface="context-menu"]')).toBeDefined();
  });
});
