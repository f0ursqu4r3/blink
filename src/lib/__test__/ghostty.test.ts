import { describe, expect, it } from "vitest";
import { hexColour, parseGhostty, type Palette } from "../ghostty";

const base: Palette = {
  background: "#000000",
  foreground: "#ffffff",
  cursor: "#ffffff",
  cursorText: "#000000",
  selectionBackground: "#444444",
  selectionForeground: "#ffffff",
  ansi: Array.from({ length: 16 }, () => "#808080"),
};

describe("hexColour", () => {
  it("normalises hex forms", () => {
    expect(hexColour("#ABCDEF")).toBe("#abcdef");
    expect(hexColour("abcdef")).toBe("#abcdef");
    expect(hexColour("#fa0")).toBe("#ffaa00");
  });

  it("rejects other values", () => {
    for (const value of ["", "#abcd", "red", "rgb(0,0,0)", "#gggggg"])
      expect(hexColour(value)).toBeNull();
  });
});

describe("parseGhostty", () => {
  it("keys the text omits keep the base value", () => {
    const parsed = parseGhostty(
      "background = 101010\npalette = 1 = #ff0000\n",
      base,
    );
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.palette.background).toBe("#101010");
    expect(parsed.palette.ansi[1]).toBe("#ff0000");
    expect(parsed.palette.ansi[2]).toBe("#808080");
    expect(parsed.palette.foreground).toBe("#ffffff");
  });

  it("reads a Ghostty theme file", () => {
    const parsed = parseGhostty(
      [
        "palette = 0=#262427",
        "palette = 3=#ffc739",
        "background = #262427",
        "foreground = #fcfcfa",
        "cursor-color = #fcfcfa",
        "cursor-text = #000000",
        "selection-background = #fcfcfa",
        "selection-foreground = #262427",
      ].join("\n"),
      base,
    );
    expect(parsed.ok && parsed.palette).toMatchObject({
      background: "#262427",
      foreground: "#fcfcfa",
      cursor: "#fcfcfa",
      cursorText: "#000000",
      selectionBackground: "#fcfcfa",
      selectionForeground: "#262427",
    });
    expect(parsed.ok && parsed.palette.ansi[3]).toBe("#ffc739");
  });

  it("ignores comments, blank lines and other config keys", () => {
    const text =
      '# theme\n\nfont-family = "Berkeley Mono"\nfont-size = 13\nforeground = "#eeeeee"\n';
    const parsed = parseGhostty(text, base);
    expect(parsed.ok && parsed.palette.foreground).toBe("#eeeeee");
  });

  it("does not change the base palette", () => {
    const before = structuredClone(base);
    parseGhostty("palette = 0=#ffffff\nbackground = #ffffff\n", base);
    expect(base).toEqual(before);
  });

  it("rejects text with no colour key", () => {
    expect(parseGhostty("font-size = 12\n", base)).toEqual({
      ok: false,
      error: "no colour keys found",
    });
    expect(parseGhostty("", base).ok).toBe(false);
  });

  it("rejects bad colours and slots with the line number", () => {
    expect(
      parseGhostty("background = #000\nforeground = white\n", base),
    ).toEqual({ ok: false, error: "line 2: foreground is not a hex colour" });
    expect(parseGhostty("palette = 16=#000000\n", base)).toEqual({
      ok: false,
      error: "line 1: expected palette = 0..15=#rrggbb",
    });
    expect(parseGhostty("palette = 3=zzz\n", base)).toEqual({
      ok: false,
      error: "line 1: palette 3 is not a hex colour",
    });
  });
});
