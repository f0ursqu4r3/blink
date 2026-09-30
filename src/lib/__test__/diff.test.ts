import { describe, expect, it } from "vitest";
import { diffLines, foldDiff, type DiffLine } from "../diff";

const apply = (lines: DiffLine[]) => ({
  before: lines.filter((l) => l.kind !== "add").map((l) => l.text),
  after: lines.filter((l) => l.kind !== "remove").map((l) => l.text),
});

describe("diffLines", () => {
  it("finds a minimal edit", () => {
    const lines = diffLines(["a", "b", "c", "d"], ["a", "x", "c", "d", "e"])!;
    expect(lines.map((l) => `${l.kind[0]}${l.text}`)).toEqual([
      "sa",
      "rb",
      "ax",
      "sc",
      "sd",
      "ae",
    ]);
    expect(lines[2]).toMatchObject({ after: 2 });
    expect(lines[1]).toMatchObject({ before: 2 });
  });
  it("handles empty sides", () => {
    expect(diffLines([], [])).toEqual([]);
    expect(diffLines([], ["a"])).toEqual([
      { kind: "add", text: "a", after: 1 },
    ]);
    expect(diffLines(["a"], [])).toEqual([
      { kind: "remove", text: "a", before: 1 },
    ]);
  });
  it("round-trips random edits", () => {
    let seed = 7;
    const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
    for (let run = 0; run < 50; run++) {
      const a = Array.from({ length: Math.floor(random() * 30) }, () =>
        String(Math.floor(random() * 5)),
      );
      const b = Array.from({ length: Math.floor(random() * 30) }, () =>
        String(Math.floor(random() * 5)),
      );
      const result = apply(diffLines(a, b)!);
      expect(result.before).toEqual(a);
      expect(result.after).toEqual(b);
    }
  });
  it("gives up past the edit limit", () => {
    expect(diffLines(["a", "b"], ["c", "d"], 3)).toBeNull();
  });
});

describe("foldDiff", () => {
  it("folds unchanged runs outside the context", () => {
    const before = Array.from({ length: 20 }, (_, i) => String(i));
    const after = [...before];
    after[10] = "x";
    const hunks = foldDiff(diffLines(before, after)!, 2);
    expect(hunks[0]).toEqual({ hidden: 8 });
    expect("lines" in hunks[1] && hunks[1].lines.length).toBe(6);
    expect(hunks[2]).toEqual({ hidden: 7 });
  });
});
