import { describe, expect, it } from "vitest";
import {
  fuzzyMatch,
  groupPath,
  highlightRuns,
  isCommandQuery,
  matchCommands,
  matchRequests,
  type Command,
} from "../command-center";
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

describe("matchCommands", () => {
  const commands: Command[] = [
    { id: "layout", label: "View: Stack request above response" },
    { id: "browser", label: "View: Hide request browser" },
    { id: "new", label: "Request: New request" },
  ];
  it("lists every command for a bare prefix", () => {
    expect(matchCommands(commands, ">").map((c) => c.id)).toEqual([
      "layout",
      "browser",
      "new",
    ]);
  });
  it("matches every word in any order", () => {
    expect(matchCommands(commands, ">response stack").map((c) => c.id)).toEqual(
      ["layout"],
    );
    expect(matchCommands(commands, "> VIEW  request").map((c) => c.id)).toEqual(
      ["layout", "browser"],
    );
    expect(matchCommands(commands, ">nothing")).toEqual([]);
  });
  it("detects command queries", () => {
    expect(isCommandQuery(">x")).toBe(true);
    expect(isCommandQuery("x>")).toBe(false);
  });
});

describe("fuzzyMatch", () => {
  it("prefers a substring at a word start", () => {
    const match = fuzzyMatch("View: Stack request", "req");
    expect(match?.indices).toEqual([12, 13, 14]);
    expect(match!.score).toBeGreaterThan(
      fuzzyMatch("prerequest", "req")!.score,
    );
  });
  it("matches characters in order", () => {
    expect(fuzzyMatch("Request: Duplicate request", "dupr")?.indices).toEqual([
      9, 10, 11, 19,
    ]);
    expect(fuzzyMatch("abc", "acb")).toBeNull();
  });
  it("ranks commands by score, then list order", () => {
    const commands: Command[] = [
      { id: "a", label: "View: Toggle unwrapped" },
      { id: "b", label: "Response: Wrap lines" },
    ];
    expect(matchCommands(commands, ">wrap").map((c) => c.id)).toEqual([
      "b",
      "a",
    ]);
    expect(matchCommands(commands, ">tgu").map((c) => c.id)).toEqual(["a"]);
  });
  it("splits text into highlighted runs", () => {
    expect(highlightRuns("abcd", [1, 2])).toEqual([
      { text: "a", match: false },
      { text: "bc", match: true },
      { text: "d", match: false },
    ]);
  });
  it("matches requests fuzzily by label", () => {
    const users = session("https://api.example.test/v1/users/list", null);
    expect(matchRequests([users], [], "usli").map((m) => m.id)).toEqual([
      users.id,
    ]);
  });
});
