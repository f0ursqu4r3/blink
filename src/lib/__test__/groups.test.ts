import { describe, expect, it } from "vitest";
import {
  canNestGroup,
  createGroup,
  deleteGroupAndPromoteContents,
  reserveGroupId,
} from "../groups";

describe("request groups", () => {
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
