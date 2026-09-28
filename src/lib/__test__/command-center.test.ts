import { describe, expect, it } from "vitest";
import { groupPath, matchRequests } from "../command-center";
import { createSession } from "../session";
import type { RequestGroup } from "../groups";

const groups: RequestGroup[] = [
  { id: 1, name: "Platform", parentId: null, collapsed: false },
  { id: 2, name: "Identity", parentId: 1, collapsed: true },
];

function session(url: string, groupId: number | null) {
  const value = createSession();
  value.draft.url = url;
  value.groupId = groupId;
  return value;
}

describe("command center matching", () => {
  it("builds the group path from the root", () => {
    expect(groupPath(groups, 2)).toBe("Platform / Identity");
    expect(groupPath(groups, null)).toBe("");
    expect(groupPath(groups, 99)).toBe("");
  });

  it("stops on a group cycle", () => {
    const cyclic: RequestGroup[] = [
      { id: 1, name: "A", parentId: 2, collapsed: false },
      { id: 2, name: "B", parentId: 1, collapsed: false },
    ];
    expect(groupPath(cyclic, 1)).toBe("B / A");
  });

  it("matches label, URL, and group path without case", () => {
    const users = session("https://api.example.test/users", 2);
    const health = session("https://status.example.test/health", null);
    const all = [users, health];
    expect(matchRequests(all, groups, "").map((m) => m.id)).toEqual([
      users.id,
      health.id,
    ]);
    expect(matchRequests(all, groups, "IDENTITY").map((m) => m.id)).toEqual([
      users.id,
    ]);
    expect(
      matchRequests(all, groups, "status.example").map((m) => m.id),
    ).toEqual([health.id]);
    expect(matchRequests(all, groups, "  health ").map((m) => m.id)).toEqual([
      health.id,
    ]);
    expect(matchRequests(all, groups, "nothing")).toEqual([]);
    expect(matchRequests(all, groups, "users")[0]).toMatchObject({
      method: "GET",
      groupPath: "Platform / Identity",
    });
  });
});
