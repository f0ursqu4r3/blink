import { describe, expect, it } from "vitest";
import {
  applyDisplayEdit,
  deleteIntoUnit,
  displayOffset,
  rawOffset,
  rawSlice,
  snapCaret,
  tokenDisplay,
} from "../token-display";

const ctx = {
  definitions: { endpoint: "users", empty: "" },
  workspaceDefinitions: { host: "api.test" },
};
const raw = "https://{{_.host}}/{{endpoint}}";
// Shown: "https://api.test/users"
const shown = tokenDisplay(raw, ctx);

describe("tokenDisplay", () => {
  it("shows defined tokens as values", () => {
    expect(shown.text).toBe("https://api.test/users");
  });

  it("keeps undefined, empty and environment references as typed", () => {
    expect(tokenDisplay("{{nope}}{{empty}}<<HOME>>", ctx).text).toBe(
      "{{nope}}{{empty}}<<HOME>>",
    );
  });
});

describe("offsets", () => {
  it("maps unit edges and plain text both ways", () => {
    expect(rawOffset(shown.segments, 8)).toBe(8);
    expect(rawOffset(shown.segments, 16)).toBe(18);
    expect(rawOffset(shown.segments, 22)).toBe(raw.length);
    expect(displayOffset(shown.segments, raw.length)).toBe(22);
    expect(displayOffset(shown.segments, 19)).toBe(17);
  });
});

describe("snapCaret", () => {
  it("moves the caret to the edge in the direction it moved", () => {
    expect(snapCaret(shown.segments, 18, 17)).toBe(22);
    expect(snapCaret(shown.segments, 21, 22)).toBe(17);
    expect(snapCaret(shown.segments, 3, 2)).toBe(3);
  });
});

describe("deleteIntoUnit", () => {
  it("removes the closing brace on Backspace after a value", () => {
    expect(deleteIntoUnit(raw, shown.segments, 22, "backward")).toEqual({
      raw: "https://{{_.host}}/{{endpoint}",
      caret: 30,
    });
  });

  it("removes the opening brace on Delete before a value", () => {
    expect(deleteIntoUnit(raw, shown.segments, 17, "forward")).toEqual({
      raw: "https://{{_.host}}/{endpoint}}",
      caret: 19,
    });
  });

  it("does nothing away from a value", () => {
    expect(deleteIntoUnit(raw, shown.segments, 3, "backward")).toBeNull();
  });
});

describe("applyDisplayEdit", () => {
  it("applies plain edits to the raw text", () => {
    expect(
      applyDisplayEdit(raw, shown, "https://api.test/users?x", 24),
    ).toEqual({ raw: `${raw}?x`, caret: raw.length + 2 });
  });

  it("replaces a whole value when an edit covers part of it", () => {
    // Select "ers" and type "X".
    expect(applyDisplayEdit(raw, shown, "https://api.test/uX", 19)).toEqual({
      raw: "https://{{_.host}}/X",
      caret: 20,
    });
  });

  it("clears the field when all text is replaced", () => {
    expect(applyDisplayEdit(raw, shown, "a", 1)).toEqual({
      raw: "a",
      caret: 1,
    });
  });
});

describe("rawSlice", () => {
  it("copies touched values as their references", () => {
    expect(rawSlice(raw, shown.segments, 10, 22)).toBe(
      "{{_.host}}/{{endpoint}}",
    );
  });
});
