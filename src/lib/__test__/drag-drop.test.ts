import { describe, expect, it } from "vitest";
import {
  hitZone,
  resolveTabDrop,
  resolveTreeDrop,
  stepRequests,
  stepTab,
  type Tree,
} from "@/lib/drag-drop";
import type { RequestGroup } from "@/lib/groups";

const box = { left: 0, top: 100, width: 200, height: 40 };
const group = (
  id: number,
  parentId: number | null,
  collapsed = false,
): RequestGroup => ({ id, name: `G${id}`, parentId, collapsed });

// Root: requests 1, 2. Group 10 (root) holds request 3 and group 11.
// Group 11 is empty. Group 12 (root) is collapsed and empty.
const tree: Tree = {
  sessions: [
    { id: 1, groupId: null },
    { id: 2, groupId: null },
    { id: 3, groupId: 10 },
  ],
  groups: [group(10, null), group(11, 10), group(12, null, true)],
};

describe("hitZone", () => {
  it("splits request rows in half", () => {
    expect(hitZone(box, { x: 5, y: 119 }, "request")).toBe("before");
    expect(hitZone(box, { x: 5, y: 120 }, "request")).toBe("after");
  });
  it("splits group rows 25/50/25", () => {
    expect(hitZone(box, { x: 5, y: 109 }, "group")).toBe("before");
    expect(hitZone(box, { x: 5, y: 110 }, "group")).toBe("into");
    expect(hitZone(box, { x: 5, y: 130 }, "group")).toBe("into");
    expect(hitZone(box, { x: 5, y: 131 }, "group")).toBe("after");
  });
  it("splits tabs left/right", () => {
    expect(hitZone(box, { x: 99, y: 0 }, "tab")).toBe("before");
    expect(hitZone(box, { x: 100, y: 0 }, "tab")).toBe("after");
  });
});

describe("resolveTreeDrop – requests", () => {
  const requests = (...ids: number[]) => ({ kind: "requests" as const, ids });

  it("drops before a request", () => {
    expect(
      resolveTreeDrop(requests(2), { type: "request", id: 1 }, "before", tree),
    ).toEqual({
      key: "request-1",
      zone: "before",
      command: { type: "moveRequests", ids: [2], groupId: null, beforeId: 1 },
    });
  });

  it("drops after the last request of a list", () => {
    expect(
      resolveTreeDrop(requests(1), { type: "request", id: 2 }, "after", tree)
        ?.command,
    ).toEqual({
      type: "moveRequests",
      ids: [1],
      groupId: null,
      beforeId: null,
    });
  });

  it("returns null when the order does not change", () => {
    expect(
      resolveTreeDrop(requests(1), { type: "request", id: 1 }, "after", tree),
    ).toBeNull();
    expect(
      resolveTreeDrop(requests(1), { type: "request", id: 2 }, "before", tree),
    ).toBeNull();
  });

  it("drops into a group on any zone of the group row", () => {
    for (const zone of ["before", "into", "after"] as const)
      expect(
        resolveTreeDrop(requests(1), { type: "group", id: 11 }, zone, tree),
      ).toEqual({
        key: "group-11",
        zone: "into",
        command: {
          type: "moveRequests",
          ids: [1],
          groupId: 11,
          beforeId: null,
        },
      });
  });

  it("moves a multi-selection across folders", () => {
    expect(
      resolveTreeDrop(
        requests(1, 3),
        { type: "request", id: 2 },
        "before",
        tree,
      )?.command,
    ).toEqual({
      type: "moveRequests",
      ids: [1, 3],
      groupId: null,
      beforeId: 2,
    });
  });

  it("drops at the start of root", () => {
    expect(
      resolveTreeDrop(
        requests(3),
        { type: "root", position: "start" },
        "into",
        tree,
      ),
    ).toEqual({
      key: "root",
      zone: "into",
      command: { type: "moveRequests", ids: [3], groupId: null, beforeId: 1 },
    });
  });

  it("drops unknown ids as null", () => {
    expect(
      resolveTreeDrop(requests(99), { type: "group", id: 10 }, "into", tree),
    ).toBeNull();
  });
});

describe("resolveTreeDrop – groups", () => {
  const folder = (id: number) => ({ kind: "group" as const, id });

  it("nests a group into another", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "group", id: 11 }, "into", tree),
    ).toEqual({
      key: "group-11",
      zone: "into",
      command: {
        type: "moveGroup",
        groupId: 12,
        parentId: 11,
        beforeGroupId: null,
      },
    });
  });

  it("reorders before a sibling", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "group", id: 10 }, "before", tree)
        ?.command,
    ).toEqual({
      type: "moveGroup",
      groupId: 12,
      parentId: null,
      beforeGroupId: 10,
    });
  });

  it("treats after on an expanded group with children as into", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "group", id: 10 }, "after", tree),
    ).toMatchObject({ zone: "into", command: { parentId: 10 } });
  });

  it("keeps after on a collapsed group", () => {
    expect(
      resolveTreeDrop(folder(10), { type: "group", id: 12 }, "after", tree),
    ).toMatchObject({
      zone: "after",
      command: { parentId: null, beforeGroupId: null },
    });
  });

  it("rejects a group into itself or its descendant", () => {
    expect(
      resolveTreeDrop(folder(10), { type: "group", id: 10 }, "into", tree),
    ).toBeNull();
    expect(
      resolveTreeDrop(folder(10), { type: "group", id: 11 }, "into", tree),
    ).toBeNull();
  });

  it("un-nests to root end", () => {
    expect(
      resolveTreeDrop(
        folder(11),
        { type: "root", position: "end" },
        "into",
        tree,
      ),
    ).toEqual({
      key: "root",
      zone: "into",
      command: {
        type: "moveGroup",
        groupId: 11,
        parentId: null,
        beforeGroupId: null,
      },
    });
  });

  it("drops onto a request row into that request's group", () => {
    expect(
      resolveTreeDrop(folder(12), { type: "request", id: 3 }, "before", tree),
    ).toMatchObject({
      key: "group-10",
      zone: "into",
      command: { parentId: 10 },
    });
  });

  it("returns null for a no-op", () => {
    expect(
      resolveTreeDrop(
        folder(12),
        { type: "root", position: "end" },
        "into",
        tree,
      ),
    ).toBeNull();
  });
});

describe("resolveTabDrop", () => {
  const requests = (...ids: number[]) => ({ kind: "requests" as const, ids });
  const open = [1, 2, 3];

  it("reorders a tab after another", () => {
    expect(resolveTabDrop(requests(1), 2, "after", open)).toEqual({
      key: "tab-2",
      zone: "after",
      ids: [1],
      beforeId: 3,
    });
  });

  it("opens a closed request before a tab", () => {
    expect(resolveTabDrop(requests(9), 1, "before", open)).toMatchObject({
      ids: [9],
      beforeId: 1,
    });
  });

  it("appends after the last tab", () => {
    expect(resolveTabDrop(requests(1), 3, "after", open)?.beforeId).toBeNull();
  });

  it("uses the strip key with no tabs", () => {
    expect(resolveTabDrop(requests(1), null, "after", [])).toEqual({
      key: "strip",
      zone: "after",
      ids: [1],
      beforeId: null,
    });
  });

  it("returns null for no-ops and groups", () => {
    expect(resolveTabDrop(requests(2), 1, "after", open)).toBeNull();
    expect(
      resolveTabDrop({ kind: "group", id: 1 }, 1, "after", open),
    ).toBeNull();
  });
});

describe("stepRequests", () => {
  const sessions = [
    { id: 1, groupId: null },
    { id: 2, groupId: null },
    { id: 3, groupId: null },
  ];
  it("moves up before the previous sibling", () => {
    expect(stepRequests(sessions, [2], -1)).toEqual({
      groupId: null,
      beforeId: 1,
    });
  });
  it("moves down past the next sibling", () => {
    expect(stepRequests(sessions, [1], 1)).toEqual({
      groupId: null,
      beforeId: 3,
    });
    expect(stepRequests(sessions, [2], 1)).toEqual({
      groupId: null,
      beforeId: null,
    });
  });
  it("stops at the edges and across groups", () => {
    expect(stepRequests(sessions, [1], -1)).toBeNull();
    expect(stepRequests(sessions, [3], 1)).toBeNull();
    expect(
      stepRequests([...sessions, { id: 4, groupId: 5 }], [3, 4], 1),
    ).toBeNull();
  });
});

describe("stepTab", () => {
  it("moves left and right", () => {
    expect(stepTab([1, 2, 3], 2, -1)).toEqual({ beforeId: 1 });
    expect(stepTab([1, 2, 3], 1, 1)).toEqual({ beforeId: 3 });
    expect(stepTab([1, 2, 3], 2, 1)).toEqual({ beforeId: null });
  });
  it("stops at the edges", () => {
    expect(stepTab([1, 2], 1, -1)).toBeNull();
    expect(stepTab([1, 2], 2, 1)).toBeNull();
  });
});
