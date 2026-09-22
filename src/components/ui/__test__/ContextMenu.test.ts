import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "../context-menu";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
});

function makeMenu(
  items: Array<{ label: string; onSelect?: () => void; disabled?: boolean }>,
) {
  const comp = defineComponent({
    setup() {
      return () =>
        h(ContextMenu, null, {
          default: () => [
            h(
              ContextMenuTrigger,
              { "data-testid": "trigger" },
              { default: () => h("button", null, "Right-click me") },
            ),
            h(ContextMenuContent, null, {
              default: () =>
                items.map((item, i) =>
                  item.label === "---"
                    ? h(ContextMenuSeparator, { key: i })
                    : h(
                        ContextMenuItem,
                        {
                          key: i,
                          disabled: item.disabled,
                          onSelect: item.onSelect,
                          "data-testid": `item-${item.label}`,
                        },
                        { default: () => item.label },
                      ),
                ),
            }),
          ],
        });
    },
  });
  const wrapper = mount(comp, { attachTo: document.body });
  wrappers.push(wrapper);
  return wrapper;
}

describe("ContextMenu primitive", () => {
  it("does not render menu content initially", () => {
    makeMenu([{ label: "Copy" }]);
    expect(document.body.querySelector("[data-testid='item-Copy']")).toBeNull();
  });

  it("opens on contextmenu event (right-click)", async () => {
    const wrapper = makeMenu([{ label: "Copy" }]);
    const trigger = wrapper.get("[data-testid='trigger']");
    await trigger.trigger("contextmenu");
    expect(
      document.body.querySelector("[data-testid='item-Copy']"),
    ).not.toBeNull();
  });

  it("calls onSelect when an item is clicked", async () => {
    const onSelect = vi.fn();
    const wrapper = makeMenu([{ label: "Paste", onSelect }]);
    await wrapper.get("[data-testid='trigger']").trigger("contextmenu");
    const item = document.body.querySelector(
      "[data-testid='item-Paste']",
    ) as HTMLElement;
    expect(item).not.toBeNull();
    item.click();
    expect(onSelect).toHaveBeenCalledOnce();
  });

  it("renders a separator between items", async () => {
    const wrapper = makeMenu([
      { label: "Copy" },
      { label: "---" },
      { label: "Paste" },
    ]);
    await wrapper.get("[data-testid='trigger']").trigger("contextmenu");
    const sep = document.body.querySelector("[role='separator']");
    expect(sep).not.toBeNull();
  });

  it("disabled items are not interactive", async () => {
    const onSelect = vi.fn();
    const wrapper = makeMenu([{ label: "Grayed", onSelect, disabled: true }]);
    await wrapper.get("[data-testid='trigger']").trigger("contextmenu");
    const item = document.body.querySelector(
      "[data-testid='item-Grayed']",
    ) as HTMLElement;
    expect(item).not.toBeNull();
    // reka-ui sets aria-disabled on disabled items
    expect(
      item.getAttribute("aria-disabled") ?? item.getAttribute("data-disabled"),
    ).toBeTruthy();
  });
});
