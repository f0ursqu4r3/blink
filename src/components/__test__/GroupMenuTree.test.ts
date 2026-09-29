import { afterEach, describe, expect, it } from "vitest";
import { enableAutoUnmount, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import GroupMenuTree from "../GroupMenuTree.vue";
import type { RequestGroup } from "@/lib/groups";
import { menuLabels, openSubmenu } from "./menu-test-utils";

enableAutoUnmount(afterEach);

const groups: RequestGroup[] = [
  { id: 1, name: "Platform", parentId: null, collapsed: false },
  { id: 2, name: "Identity", parentId: 1, collapsed: false },
  { id: 3, name: "Billing", parentId: null, collapsed: false },
];

async function openTree(props: Record<string, unknown>) {
  const picks: (number | null)[] = [];
  const wrapper = mount(
    defineComponent({
      setup: () => () =>
        h(ContextMenu, null, {
          default: () => [
            h(
              ContextMenuTrigger,
              { asChild: true },
              { default: () => h("button", { "data-trigger": "" }, "t") },
            ),
            h(ContextMenuContent, null, {
              default: () =>
                h(GroupMenuTree, {
                  groups,
                  rootLabel: "Ungrouped",
                  ...props,
                  onPick: (id: number | null) => picks.push(id),
                }),
            }),
          ],
        }),
    }),
    { attachTo: document.body },
  );
  await wrapper.get("[data-trigger]").trigger("contextmenu");
  return { wrapper, picks };
}
const target = (id: string | number) =>
  document.body.querySelector(`[data-move-target="${id}"]`) as HTMLElement;

describe("GroupMenuTree", () => {
  it("lists the root, then top-level groups; a group with children is a submenu", async () => {
    await openTree({});
    expect(menuLabels()).toEqual(["Ungrouped", "Platform", "Billing"]);
    expect(target(1).getAttribute("aria-haspopup")).toBe("menu");
    expect(target(3).getAttribute("aria-haspopup")).toBeNull();
  });

  it("opens a submenu with 'Move into' first, then the children", async () => {
    const { picks } = await openTree({});
    await openSubmenu(target(1));
    const submenu = document.body.querySelectorAll(
      '[data-surface="context-menu"]',
    )[1];
    expect(menuLabels(submenu)).toEqual(["Move into Platform", "Identity"]);
    (
      document.body.querySelector('[data-move-into="1"]') as HTMLElement
    ).click();
    expect(picks).toEqual([1]);
  });

  it("marks the current location as checked and disabled", async () => {
    await openTree({ currentId: 3 });
    expect(target(3).getAttribute("aria-checked")).toBe("true");
    expect(target(3).hasAttribute("data-disabled")).toBe(true);
  });

  it("marks the root as current when currentId is null", async () => {
    await openTree({ currentId: null });
    expect(target("root").getAttribute("aria-checked")).toBe("true");
  });

  it("hides the excluded group and its subtree", async () => {
    await openTree({ excludeId: 1, rootLabel: "Top level" });
    expect(menuLabels()).toEqual(["Top level", "Billing"]);
  });

  it("emits pick with null for the root", async () => {
    const { picks } = await openTree({});
    target("root").click();
    expect(picks).toEqual([null]);
  });
});
