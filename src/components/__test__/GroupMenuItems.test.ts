import { afterEach, describe, expect, it } from "vitest";
import { enableAutoUnmount, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import GroupMenuItems from "../GroupMenuItems.vue";
import type { RequestGroup } from "@/lib/groups";
import { menuLabels, openSubmenu } from "./menu-test-utils";

enableAutoUnmount(afterEach);

const groups: RequestGroup[] = [
  { id: 1, name: "Platform", parentId: null, collapsed: false },
  { id: 2, name: "Identity", parentId: 1, collapsed: true },
  { id: 3, name: "Billing", parentId: null, collapsed: false },
];

async function open(groupIndex: number, canMoveSelection = false) {
  const actions: string[] = [];
  const moves: (number | null)[] = [];
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
                h(GroupMenuItems, {
                  group: groups[groupIndex],
                  groups,
                  canMoveSelection,
                  onAction: (action: string) => actions.push(action),
                  onMoveTo: (id: number | null) => moves.push(id),
                }),
            }),
          ],
        }),
    }),
    { attachTo: document.body },
  );
  await wrapper.get("[data-trigger]").trigger("contextmenu");
  return { actions, moves };
}
const find = (selector: string) =>
  document.body.querySelector(selector) as HTMLElement;

describe("GroupMenuItems", () => {
  it("lists the group actions in order", async () => {
    await open(0);
    expect(menuLabels()).toEqual([
      "New request",
      "New group",
      "Rename",
      "Settings…",
      "Collapse",
      "Collapse all",
      "Move to",
      "Delete group",
    ]);
  });

  it("offers Move selection here only with a movable selection", async () => {
    await open(0, true);
    expect(menuLabels()).toContain("Move selection here");
  });

  it("labels a collapsed group Expand", async () => {
    await open(1);
    expect(menuLabels()).toContain("Expand");
  });

  it("marks Delete group destructive and emits delete", async () => {
    const { actions } = await open(0);
    const del = find("[data-group-delete]");
    expect(del.getAttribute("data-variant")).toBe("destructive");
    del.click();
    expect(actions).toEqual(["delete"]);
  });

  it("does not offer the group or its subtree as a Move to target", async () => {
    const { moves } = await open(0);
    await openSubmenu(find("[data-group-move-menu]"));
    expect(find('[data-move-target="1"]')).toBeNull();
    expect(find('[data-move-target="2"]')).toBeNull();
    find('[data-move-target="3"]').click();
    expect(moves).toEqual([3]);
  });

  it("marks Top level as current for a top-level group", async () => {
    await open(0);
    await openSubmenu(find("[data-group-move-menu]"));
    expect(find('[data-move-target="root"]').getAttribute("aria-checked")).toBe(
      "true",
    );
  });
});
