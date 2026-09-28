import { describe, expect, it } from "vitest";
import {
  groupsAfterMove,
  sessionsAfterMove,
  type RequestGroup,
} from "@/lib/groups";

const group = (id: number, parentId: number | null): RequestGroup => ({
  id,
  name: `G${id}`,
  parentId,
  collapsed: false,
});
const ids = (items: { id: number }[]) => items.map((item) => item.id);

describe("sessionsAfterMove", () => {
  const sessions = [
    { id: 1, groupId: null },
    { id: 2, groupId: 10 },
    { id: 3, groupId: null },
    { id: 4, groupId: 10 },
  ];

  it("inserts before the named session", () => {
    expect(ids(sessionsAfterMove(sessions, [4], 10, 2))).toEqual([1, 4, 2, 3]);
  });

  it("appends after the last session of the group", () => {
    expect(ids(sessionsAfterMove(sessions, [1], 10, null))).toEqual([
      2, 3, 4, 1,
    ]);
  });

  it("appends to the end for an empty group", () => {
    expect(ids(sessionsAfterMove(sessions, [1], 20, null))).toEqual([
      2, 3, 4, 1,
    ]);
  });

  it("ignores a beforeId from another group and appends", () => {
    expect(ids(sessionsAfterMove(sessions, [3], 10, 1))).toEqual([1, 2, 4, 3]);
  });

  it("keeps the source order of several moved sessions", () => {
    expect(ids(sessionsAfterMove(sessions, [4, 1], null, 3))).toEqual([
      2, 1, 4, 3,
    ]);
  });

  it("returns the same objects", () => {
    expect(sessionsAfterMove(sessions, [1], null, null)[0]).toBe(sessions[1]);
  });
});

describe("groupsAfterMove", () => {
  const groups = [group(1, null), group(2, null), group(3, 1), group(4, 1)];

  it("reorders among siblings", () => {
    expect(ids(groupsAfterMove(groups, 2, null, 1) ?? [])).toEqual([
      2, 1, 3, 4,
    ]);
  });

  it("nests at the end of the new parent's children", () => {
    expect(ids(groupsAfterMove(groups, 2, 1, null) ?? [])).toEqual([
      1, 3, 4, 2,
    ]);
  });

  it("un-nests before a root group", () => {
    expect(ids(groupsAfterMove(groups, 4, null, 1) ?? [])).toEqual([
      4, 1, 2, 3,
    ]);
  });

  it("rejects a move into a descendant", () => {
    expect(groupsAfterMove(groups, 1, 3, null)).toBeNull();
  });

  it("rejects unknown ids", () => {
    expect(groupsAfterMove(groups, 99, null, null)).toBeNull();
    expect(groupsAfterMove(groups, 2, 99, null)).toBeNull();
  });

  it("does not assign parentId", () => {
    groupsAfterMove(groups, 2, 1, null);
    expect(groups[1].parentId).toBeNull();
  });
});
