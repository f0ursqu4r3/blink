import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuCheckboxItem,
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "../context-menu";
import { openSubmenu } from "@/components/__test__/menu-test-utils";

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

function mountTemplate(template: string) {
  const wrapper = mount(
    defineComponent({
      components: {
        ContextMenu,
        ContextMenuTrigger,
        ContextMenuContent,
        ContextMenuItem,
        ContextMenuCheckboxItem,
        ContextMenuRadioGroup,
        ContextMenuRadioItem,
        ContextMenuShortcut,
        ContextMenuSub,
        ContextMenuSubTrigger,
        ContextMenuSubContent,
      },
      template: `<ContextMenu>
        <ContextMenuTrigger as-child><div data-trigger tabindex="0"><input data-field /></div></ContextMenuTrigger>
        <ContextMenuContent>${template}</ContextMenuContent>
      </ContextMenu>`,
    }),
    { attachTo: document.body },
  );
  wrappers.push(wrapper);
  return wrapper;
}
const open = (wrapper: ReturnType<typeof mount>) =>
  wrapper.get("[data-trigger]").trigger("contextmenu");

describe("ContextMenu parts", () => {
  it("renders a shortcut hint inside an item", async () => {
    const w = mountTemplate(
      `<ContextMenuItem>Close<ContextMenuShortcut>⌘W</ContextMenuShortcut></ContextMenuItem>`,
    );
    await open(w);
    expect(
      document.body.querySelector('[data-slot="context-menu-shortcut"]')
        ?.textContent,
    ).toBe("⌘W");
  });

  it("marks a destructive item", async () => {
    const w = mountTemplate(
      `<ContextMenuItem variant="destructive">Delete</ContextMenuItem>`,
    );
    await open(w);
    const item = document.body.querySelector('[role="menuitem"]')!;
    expect(item.getAttribute("data-variant")).toBe("destructive");
    expect(item.hasAttribute("variant")).toBe(false);
  });

  it("shows the checked state of checkbox and radio items", async () => {
    const w = mountTemplate(`
      <ContextMenuCheckboxItem :model-value="true">Wrap lines</ContextMenuCheckboxItem>
      <ContextMenuRadioGroup model-value="json">
        <ContextMenuRadioItem value="none">None</ContextMenuRadioItem>
        <ContextMenuRadioItem value="json">JSON</ContextMenuRadioItem>
      </ContextMenuRadioGroup>`);
    await open(w);
    expect(
      document.body
        .querySelector('[role="menuitemcheckbox"]')
        ?.getAttribute("aria-checked"),
    ).toBe("true");
    const radios = [
      ...document.body.querySelectorAll('[role="menuitemradio"]'),
    ];
    expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual([
      "false",
      "true",
    ]);
  });

  it("gives a sub-trigger a chevron and a sub-content the menu surface", async () => {
    const w = mountTemplate(`
      <ContextMenuSub>
        <ContextMenuSubTrigger data-sub>Move to</ContextMenuSubTrigger>
        <ContextMenuSubContent><ContextMenuItem>Inside</ContextMenuItem></ContextMenuSubContent>
      </ContextMenuSub>`);
    await open(w);
    const sub = document.body.querySelector("[data-sub]") as HTMLElement;
    expect(sub.querySelector("svg")).not.toBeNull();
    await openSubmenu(sub);
    expect(
      document.body.querySelectorAll('[data-surface="context-menu"]').length,
    ).toBe(2);
  });

  it("opens with Shift+F10 on the focused trigger", async () => {
    const w = mountTemplate(
      `<ContextMenuItem data-testid="kb">Item</ContextMenuItem>`,
    );
    await w
      .get("[data-trigger]")
      .trigger("keydown", { key: "F10", shiftKey: true });
    expect(document.body.querySelector('[data-testid="kb"]')).not.toBeNull();
  });

  it("does not open with Shift+F10 inside a text input", async () => {
    const w = mountTemplate(
      `<ContextMenuItem data-testid="kb">Item</ContextMenuItem>`,
    );
    await w
      .get("[data-field]")
      .trigger("keydown", { key: "F10", shiftKey: true });
    expect(document.body.querySelector('[data-testid="kb"]')).toBeNull();
  });
});
