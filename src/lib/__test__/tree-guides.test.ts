import { describe, expect, it } from "vitest";
import { treeGuides } from "../tree-guides";

describe("treeGuides", () => {
  it("draws elbows and through lines", () => {
    // A            level 0
    //   ├ r1       1
    //   ├ B        1
    //   │  └ r2    2
    //   └ r3       1
    // C            0
    //   └ r4       1
    expect(treeGuides([0, 1, 1, 2, 1, 0, 1])).toEqual([
      null,
      { through: [], elbow: "mid" },
      { through: [], elbow: "mid" },
      { through: [1], elbow: "last" },
      { through: [], elbow: "last" },
      null,
      { through: [], elbow: "last" },
    ]);
  });
  it("ends a line when the list goes shallower", () => {
    expect(treeGuides([0, 1, 2, 0, 1])[2]).toEqual({
      through: [],
      elbow: "last",
    });
  });
});
