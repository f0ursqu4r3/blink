import { parseGhostty, type Palette, type ParseResult } from "./ghostty";

export const THEME_KEY = "blink.theme";

export type AccentSlot = 1 | 2 | 3 | 4 | 5 | 6;
export type ThemeSetting = { name: string; text: string; accent: AccentSlot };

export const accentSlots: { value: AccentSlot; label: string }[] = [
  { value: 1, label: "Red" },
  { value: 2, label: "Green" },
  { value: 3, label: "Yellow" },
  { value: 4, label: "Blue" },
  { value: 5, label: "Magenta" },
  { value: 6, label: "Cyan" },
];

/**
 * Hex approximation of the default amber theme. Only used for keys a
 * Ghostty text omits; with no theme the oklch tokens in style.css apply.
 */
export const BASE_PALETTE: Palette = {
  background: "#151410",
  foreground: "#e8e3d6",
  cursor: "#e8e3d6",
  cursorText: "#151410",
  selectionBackground: "#eeb93c",
  selectionForeground: "#151410",
  ansi: [
    "#151410",
    "#f48f79",
    "#92d193",
    "#eeb93c",
    "#9ec3e8",
    "#c9a3e0",
    "#8fd3d0",
    "#e8e3d6",
    "#5a574e",
    "#f7a898",
    "#aee0ae",
    "#f5cc6a",
    "#b8d4f0",
    "#d8bce9",
    "#aee2df",
    "#ffffff",
  ],
};

export function parseTheme(setting: ThemeSetting): ParseResult {
  return parseGhostty(setting.text, BASE_PALETTE);
}

/** WCAG relative luminance of a `#rrggbb` colour. */
export function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((start) => {
    const channel = parseInt(hex.slice(start, start + 2), 16) / 255;
    return channel <= 0.03928
      ? channel / 12.92
      : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Raw palette properties. style.css derives Blink tokens from them. */
export function themeCss(palette: Palette, accent: AccentSlot): string {
  const properties: [string, string][] = [
    ["--term-bg", palette.background],
    ["--term-fg", palette.foreground],
    ["--term-selection-bg", palette.selectionBackground],
    ["--term-accent", palette.ansi[accent]],
    ["--term-scheme", luminance(palette.background) > 0.5 ? "light" : "dark"],
    ...palette.ansi.map((colour, slot): [string, string] => [
      `--term-${slot}`,
      colour,
    ]),
  ];
  return `:root{${properties.map(([name, value]) => `${name}:${value};`).join("")}}`;
}

const styleId = "blink-theme";

/** Apply `palette` to `doc`, or restore the default theme with `null`. */
export function applyTheme(
  doc: Document,
  palette: Palette | null,
  accent: AccentSlot = 3,
) {
  let style = doc.getElementById(styleId);
  if (!palette) {
    style?.remove();
    delete doc.documentElement.dataset.theme;
    return;
  }
  if (!style) {
    style = doc.createElement("style");
    style.id = styleId;
    doc.head.append(style);
  }
  style.textContent = themeCss(palette, accent);
  doc.documentElement.dataset.theme = "ghostty";
}

function validSetting(value: unknown): value is ThemeSetting {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const setting = value as Record<string, unknown>;
  return (
    typeof setting.name === "string" &&
    typeof setting.text === "string" &&
    accentSlots.some((slot) => slot.value === setting.accent)
  );
}

/** The stored theme, or `null` (default) when nothing valid is stored. */
export function loadTheme(storage: Storage | null): ThemeSetting | null {
  if (!storage) return null;
  try {
    const raw = storage.getItem(THEME_KEY);
    if (raw === null) return null;
    const value: unknown = JSON.parse(raw);
    return validSetting(value) && parseTheme(value).ok ? value : null;
  } catch {
    return null;
  }
}

export function saveTheme(storage: Storage, setting: ThemeSetting | null) {
  if (setting) storage.setItem(THEME_KEY, JSON.stringify(setting));
  else storage.removeItem(THEME_KEY);
}
