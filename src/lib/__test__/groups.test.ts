import { describe, expect, it } from "vitest";
import {
  canNestGroup,
  createGroup,
  deleteGroupAndPromoteContents,
  groupSubtree,
  reserveGroupId,
} from "../groups";

describe("request groups", () => {
  it("collects a group and all of its descendants", () => {
    const group = (id: number, parentId: number | null) => ({
      id,
      name: `Group ${id}`,
      parentId,
      collapsed: false,
    });
    const groups = [group(1, null), group(2, 1), group(3, 2), group(4, null)];

    expect(groupSubtree(groups, 1)).toEqual(new Set([1, 2, 3]));
    expect(groupSubtree(groups, 2)).toEqual(new Set([2, 3]));
    expect(groupSubtree(groups, 4)).toEqual(new Set([4]));
    expect(groupSubtree(groups, 9)).toEqual(new Set());
  });

  it("creates groups with distinct identifiers and explicit parents", () => {
    const root = createGroup("Platform");
    const child = createGroup("Identity", root.id);

    expect(root).toMatchObject({ name: "Platform", parentId: null });
    expect(child).toMatchObject({ name: "Identity", parentId: root.id });
    expect(child.id).toBeGreaterThan(root.id);
  });

  it("rejects moves that make a group its own ancestor", () => {
    const parent = {
      id: 1,
      name: "Platform",
      parentId: null,
      collapsed: false,
    };
    const child = { id: 2, name: "Identity", parentId: 1, collapsed: false };
    const grandchild = {
      id: 3,
      name: "Sessions",
      parentId: 2,
      collapsed: false,
    };

    expect(
      canNestGroup([parent, child, grandchild], parent.id, grandchild.id),
    ).toBe(false);
    expect(
      canNestGroup([parent, child, grandchild], grandchild.id, parent.id),
    ).toBe(true);
  });

  it("promotes child groups and moves requests to the deleted group's parent", () => {
    reserveGroupId(3);
    const parent = {
      id: 1,
      name: "Platform",
      parentId: null,
      collapsed: false,
    };
    const removed = { id: 2, name: "Identity", parentId: 1, collapsed: false };
    const child = { id: 3, name: "Sessions", parentId: 2, collapsed: false };

    expect(
      deleteGroupAndPromoteContents(
        [parent, removed, child],
        [
          { id: 10, groupId: 2 },
          { id: 11, groupId: 3 },
        ],
        removed.id,
      ),
    ).toEqual({
      groups: [parent, { ...child, parentId: 1 }],
      sessions: [
        { id: 10, groupId: 1 },
        { id: 11, groupId: 3 },
      ],
    });
  });
});
