import { afterEach, describe, expect, it } from "vitest";
import {
  BASE_PALETTE,
  THEME_KEY,
  applyTheme,
  loadTheme,
  luminance,
  parseTheme,
  saveTheme,
  themeCss,
  type ThemeSetting,
} from "../theme";

const light: ThemeSetting = {
  name: "Paper",
  text: "background = #ffffff\nforeground = #111111\npalette = 4=#0055ff\n",
  accent: 4,
};

afterEach(() => {
  localStorage.clear();
  applyTheme(document, null);
});

describe("theme", () => {
  it("fills keys the text omits from the base palette", () => {
    const parsed = parseTheme({
      name: "x",
      text: "background = #ffffff",
      accent: 3,
    });
    expect(parsed.ok && parsed.palette.foreground).toBe(
      BASE_PALETTE.foreground,
    );
    expect(parsed.ok && parsed.palette.ansi[3]).toBe(BASE_PALETTE.ansi[3]);
  });

  it("measures relative luminance", () => {
    expect(luminance("#ffffff")).toBeCloseTo(1);
    expect(luminance("#000000")).toBeCloseTo(0);
  });

  it("writes term properties, the accent slot, and the colour scheme", () => {
    const parsed = parseTheme(light);
    if (!parsed.ok) throw new Error(parsed.error);
    const css = themeCss(parsed.palette, 4);
    expect(css).toContain("--term-bg:#ffffff;");
    expect(css).toContain("--term-fg:#111111;");
    expect(css).toContain("--term-accent:#0055ff;");
    expect(css).toContain("--term-4:#0055ff;");
    expect(css).toContain("--term-scheme:light;");
    expect(themeCss(BASE_PALETTE, 3)).toContain("--term-scheme:dark;");
  });

  it("applies and removes the theme on the document", () => {
    const parsed = parseTheme(light);
    if (!parsed.ok) throw new Error(parsed.error);
    applyTheme(document, parsed.palette, 4);
    expect(document.documentElement.dataset.theme).toBe("ghostty");
    expect(document.getElementById("blink-theme")?.textContent).toContain(
      "--term-bg:#ffffff;",
    );
    applyTheme(document, parsed.palette, 4);
    expect(document.querySelectorAll("#blink-theme")).toHaveLength(1);
    applyTheme(document, null);
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(document.getElementById("blink-theme")).toBeNull();
  });

  it("round-trips a setting through storage", () => {
    saveTheme(localStorage, light);
    expect(loadTheme(localStorage)).toEqual(light);
    saveTheme(localStorage, null);
    expect(localStorage.getItem(THEME_KEY)).toBeNull();
    expect(loadTheme(localStorage)).toBeNull();
  });

  it("loads the default for corrupt stored values", () => {
    for (const raw of [
      "not json",
      "[]",
      JSON.stringify({ name: "x", text: light.text, accent: 9 }),
      JSON.stringify({ name: "x", text: "foreground = white", accent: 3 }),
      JSON.stringify({ name: 1, text: light.text, accent: 3 }),
    ]) {
      localStorage.setItem(THEME_KEY, raw);
      expect(loadTheme(localStorage)).toBeNull();
    }
    expect(loadTheme(null)).toBeNull();
  });
});
