# VS Code-style Shell and Ghostty Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Blink a VS Code-style shell (title bar command center, activity bar, rounded cards, status bar) and let the user color the app from a Ghostty palette.

**Architecture:** A Ghostty palette parses into hex colors (`src/lib/ghostty.ts`). `src/lib/theme.ts` writes them as `--term-*` custom properties into one `<style id="blink-theme">` element and sets `data-theme="ghostty"` on `<html>`; `src/style.css` derives every Blink token from `--term-*` with `color-mix()` under that attribute. A singleton composable (`useTheme`) owns preview/commit/revert and `localStorage`. The shell is split into small components (`ActivityBar`, `CommandCenter`, `ThemeSettings`) wired by `App.vue`.

**Tech Stack:** Vue 3 `<script setup>`, Tailwind CSS 4, reka-ui, lucide-vue-next, Vitest + @vue/test-utils (jsdom), Playwright, Tauri 2 (Rust).

**Spec:** `docs/superpowers/specs/2026-09-27-vscode-shell-ghostty-theme-design.md`

## Global Constraints

- No new npm or Cargo dependencies.
- `localStorage` key for the theme: `blink.theme`.
- No stored theme keeps the current amber oklch tokens exactly.
- Card radius 8 px (`rounded-lg`), control radius 4 px.
- Title bar about 36 px (`h-9`). Status bar about 24 px. Activity bar about 44 px (`w-11`). Body gaps 6 px (`gap-1.5`).
- macOS Tauri: 78 px left padding (`pl-19.5`) for the traffic lights.
- Theme files larger than 64 KiB are rejected. Names with `/`, `\`, or `..` are rejected. User themes (`~/.config/ghostty/themes`) win over bundled themes (`/Applications/Ghostty.app/Contents/Resources/ghostty/themes`).
- Storage failure message, verbatim: `Theme not saved: local storage is full or disabled.`
- Empty command center result, verbatim: `No matching requests`.
- Keep the button name `Application settings` (e2e tests use it).
- Local system fonts only.
- Match surrounding code style: double quotes in `.ts` and most `.vue` files; `RequestTabs.vue` uses single quotes, keep that file's style.
- Commands: unit `bun run test`, e2e `bun run test:e2e`, types + build `bun run build`, lint `bun run lint`, Rust `cd src-tauri && cargo test`.

## Review Focus

1. **Partial Ghostty text** (for example only `background = #ffffff`): keys the text omits come from Blink's base palette; the app must not go blank. Test in Task 2.
2. **Light themes** (white background): `color-scheme` must switch to `light` so native controls and scrollbars stay readable. Test in Task 2.
3. **Corrupt `blink.theme` value** (not JSON, wrong shape, bad accent, invalid Ghostty text): load falls back to the default and does not throw at startup. Test in Task 2.
4. **Cancel after preview when the saved theme is the default**: the injected style and `data-theme` must be removed, not left at the preview. Test in Task 3 and Task 5.
5. **`Cmd/Ctrl+P` while a dialog is open**: the command center must not open behind the dialog. Test in Task 7.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/lib/ghostty.ts` (new) | Parse Ghostty `key = value` text into a `Palette`. Ported from term0. |
| `src/lib/theme.ts` (new) | Base palette, accent slots, luminance, CSS text, apply to document, load/save `blink.theme`. |
| `src/composables/useTheme.ts` (new) | Shared theme state: `draft`, `error`, `saveError`, `name`, `preview`, `commit`, `revert`, `reload`. |
| `src/lib/ghostty-themes.ts` (new) | Tauri `invoke` wrappers for listing and reading theme files. |
| `src-tauri/src/ghostty_themes.rs` (new) | Rust commands `list_ghostty_themes`, `read_ghostty_theme`. |
| `src/components/ThemeSettings.vue` (new) | Theme section of Application Settings. |
| `src/lib/command-center.ts` (new) | Request matching for the command center. |
| `src/components/CommandCenter.vue` (new) | Title bar search box and popover. |
| `src/components/ActivityBar.vue` (new) | Left icon strip: Browser toggle, Settings. |
| `src/style.css` | New tokens `--frame`, `--info`, `--selection`; `:root[data-theme="ghostty"]` derivation. |
| `src/main.ts` | Apply the stored theme before mount. |
| `src/components/ApplicationSettingsDialog.vue` | Host `ThemeSettings`, preview/commit/revert lifecycle. |
| `src/components/RequestBrowser.vue` | Card styling, new header actions, remove collapse control. |
| `src/components/RequestTabs.vue` | Add Duplicate button. |
| `src/App.vue` | New shell layout. |
| `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json` | Overlay title bar, drag permission. |
| `DESIGN.md` | Layout, shape, theme rules. |

---

### Task 1: Ghostty parser

**Files:**
- Create: `src/lib/ghostty.ts`
- Test: `src/lib/__test__/ghostty.test.ts`

**Interfaces:**
- Produces:
  - `type Palette = { background: string; foreground: string; cursor: string; cursorText: string; selectionBackground: string; selectionForeground: string; ansi: string[] }`
  - `type ParseResult = { ok: true; palette: Palette } | { ok: false; error: string }`
  - `hexColour(value: string): string | null`
  - `parseGhostty(text: string, base: Palette): ParseResult`

- [ ] **Step 1: Write the failing test**

Create `src/lib/__test__/ghostty.test.ts`:

```ts
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun run test src/lib/__test__/ghostty.test.ts`
Expected: FAIL, cannot resolve `../ghostty`.

- [ ] **Step 3: Write the implementation**

Create `src/lib/ghostty.ts`:

```ts
// Parser for Ghostty colour configuration (`key = value` lines), ported
// from term0. Only colour keys are read; other keys are ignored, so a full
// Ghostty config or a theme file both work.

export type Palette = {
  background: string;
  foreground: string;
  cursor: string;
  cursorText: string;
  selectionBackground: string;
  selectionForeground: string;
  /** ANSI colours 0 to 15, as lowercase `#rrggbb`. */
  ansi: string[];
};

export type ParseResult =
  | { ok: true; palette: Palette }
  | { ok: false; error: string };

const keys: Record<string, Exclude<keyof Palette, "ansi">> = {
  background: "background",
  foreground: "foreground",
  "cursor-color": "cursor",
  "cursor-text": "cursorText",
  "selection-background": "selectionBackground",
  "selection-foreground": "selectionForeground",
};

const ansiCount = 16;

/** `#rgb`, `#rrggbb`, `rgb` or `rrggbb` as lowercase `#rrggbb`. */
export function hexColour(value: string): string | null {
  const hex = value.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{6}$/.test(hex)) return `#${hex}`;
  if (/^[0-9a-f]{3}$/.test(hex))
    return `#${[...hex].map((digit) => digit + digit).join("")}`;
  return null;
}

function unquote(value: string): string {
  const trimmed = value.trim();
  const quoted =
    trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"');
  return quoted ? trimmed.slice(1, -1) : trimmed;
}

/**
 * Parse `text` over `base`: keys the text sets replace the base values.
 * Text with no colour key, or with any colour that is not a hex value, is
 * rejected as a whole.
 */
export function parseGhostty(text: string, base: Palette): ParseResult {
  const palette: Palette = { ...base, ansi: [...base.ansi] };
  let found = 0;
  for (const [index, raw] of text.split(/\r?\n/).entries()) {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;
    const equals = line.indexOf("=");
    if (equals < 0) continue;
    const key = line.slice(0, equals).trim();
    const value = unquote(line.slice(equals + 1));
    const where = `line ${index + 1}`;
    if (key === "palette") {
      const match = /^(\d+)\s*=\s*(.+)$/.exec(value);
      const slot = match ? Number(match[1]) : -1;
      if (!match || slot >= ansiCount)
        return { ok: false, error: `${where}: expected palette = 0..15=#rrggbb` };
      const colour = hexColour(match[2] ?? "");
      if (!colour)
        return { ok: false, error: `${where}: palette ${slot} is not a hex colour` };
      palette.ansi[slot] = colour;
      found += 1;
      continue;
    }
    const field = keys[key];
    if (!field) continue;
    const colour = hexColour(value);
    if (!colour) return { ok: false, error: `${where}: ${key} is not a hex colour` };
    palette[field] = colour;
    found += 1;
  }
  if (found === 0) return { ok: false, error: "no colour keys found" };
  return { ok: true, palette };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun run test src/lib/__test__/ghostty.test.ts`
Expected: PASS (8 tests).

- [ ] **Step 5: Format and commit**

```bash
bunx prettier --write src/lib/ghostty.ts src/lib/__test__/ghostty.test.ts
git add src/lib/ghostty.ts src/lib/__test__/ghostty.test.ts
git commit -m "feat: Ghostty palette parser"
```

---

### Task 2: Theme tokens and theme library

**Files:**
- Create: `src/lib/theme.ts`
- Modify: `src/style.css` (`:root` block, `@theme inline` block, `::selection`)
- Modify: `src/lib/code-editor.ts:99` and `:124` (hardcoded colors)
- Modify: `src/components/CodeView.vue:152`, `src/components/JsonTreeView.vue:140,142`
- Test: `src/lib/__test__/theme.test.ts`

**Interfaces:**
- Consumes: `Palette`, `ParseResult`, `parseGhostty` from Task 1.
- Produces:
  - `THEME_KEY = "blink.theme"`
  - `type AccentSlot = 1 | 2 | 3 | 4 | 5 | 6`
  - `type ThemeSetting = { name: string; text: string; accent: AccentSlot }`
  - `accentSlots: { value: AccentSlot; label: string }[]`
  - `BASE_PALETTE: Palette`
  - `parseTheme(setting: ThemeSetting): ParseResult`
  - `luminance(hex: string): number`
  - `themeCss(palette: Palette, accent: AccentSlot): string`
  - `applyTheme(doc: Document, palette: Palette | null, accent?: AccentSlot): void`
  - `loadTheme(storage: Storage | null): ThemeSetting | null`
  - `saveTheme(storage: Storage, setting: ThemeSetting | null): void`
  - CSS tokens `--frame`, `--info`, `--selection`; Tailwind colors `bg-frame`, `text-info`.

- [ ] **Step 1: Write the failing test**

Create `src/lib/__test__/theme.test.ts`:

```ts
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
    const parsed = parseTheme({ name: "x", text: "background = #ffffff", accent: 3 });
    expect(parsed.ok && parsed.palette.foreground).toBe(BASE_PALETTE.foreground);
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun run test src/lib/__test__/theme.test.ts`
Expected: FAIL, cannot resolve `../theme`.

- [ ] **Step 3: Write `src/lib/theme.ts`**

```ts
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
    "#151410", "#f48f79", "#92d193", "#eeb93c",
    "#9ec3e8", "#c9a3e0", "#8fd3d0", "#e8e3d6",
    "#5a574e", "#f7a898", "#aee0ae", "#f5cc6a",
    "#b8d4f0", "#d8bce9", "#aee2df", "#ffffff",
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun run test src/lib/__test__/theme.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 5: Add tokens to `src/style.css`**

In the `:root` block, after `--destructive: oklch(0.77 0.13 30);`, add:

```css
  --frame: oklch(0.13 0.005 100);
  --info: oklch(0.8 0.07 240);
  --selection: oklch(0.82 0.145 85 / 25%);
```

After the `:root { ... }` block, add:

```css
/* Ghostty theme: every token derives from the --term-* palette that
   src/lib/theme.ts writes. */
:root[data-theme="ghostty"] {
  color-scheme: var(--term-scheme);
  --background: var(--term-bg);
  --foreground: var(--term-fg);
  --muted: color-mix(in oklch, var(--term-fg) 5%, var(--term-bg));
  --secondary: color-mix(in oklch, var(--term-fg) 9%, var(--term-bg));
  --accent: color-mix(in oklch, var(--term-fg) 14%, var(--term-bg));
  --border: color-mix(in oklch, var(--term-fg) 18%, var(--term-bg));
  --input: color-mix(in oklch, var(--term-fg) 28%, var(--term-bg));
  --muted-foreground: color-mix(in oklch, var(--term-fg) 64%, var(--term-bg));
  --frame: color-mix(in oklch, var(--term-bg), black 22%);
  --primary: var(--term-accent);
  --primary-foreground: var(--term-bg);
  --success: var(--term-2);
  --destructive: var(--term-1);
  --info: var(--term-4);
  --selection: color-mix(in oklch, var(--term-selection-bg) 35%, transparent);
}
```

In `@theme inline`, after `--color-destructive: var(--destructive);`, add:

```css
  --color-frame: var(--frame);
  --color-info: var(--info);
```

Replace the `::selection` rule body with `background: var(--selection);`.

- [ ] **Step 6: Replace hardcoded colors with tokens**

- `src/lib/code-editor.ts` line 99: `color: "oklch(0.8 0.07 240)",` → `color: "var(--info)",`
- `src/lib/code-editor.ts` line 124: `backgroundColor: "oklch(0.82 0.145 85 / 25%)",` → `backgroundColor: "var(--selection)",`
- `src/components/CodeView.vue` line 152: `color: oklch(0.8 0.07 240);` → `color: var(--info);`
- `src/components/JsonTreeView.vue` lines 140 and 142: `'text-[oklch(0.8_0.07_240)]'` → `'text-info'`

Check: `grep -rn "oklch(0.8 0.07 240)\|oklch(0.8_0.07_240)\|0.145 85 / 25%" src --include='*.vue' --include='*.ts'` prints nothing. (`style.css` still holds the default values.)

- [ ] **Step 7: Run the full unit suite and build**

Run: `bun run test && bun run build`
Expected: all tests PASS; build succeeds.

- [ ] **Step 8: Commit**

```bash
bunx prettier --write src/lib/theme.ts src/lib/__test__/theme.test.ts src/style.css
git add src/lib/theme.ts src/lib/__test__/theme.test.ts src/style.css src/lib/code-editor.ts src/components/CodeView.vue src/components/JsonTreeView.vue
git commit -m "feat: palette-derived theme tokens"
```

---

### Task 3: Theme state composable

**Files:**
- Create: `src/composables/useTheme.ts`
- Modify: `src/main.ts`
- Test: `src/composables/__test__/useTheme.test.ts`

**Interfaces:**
- Consumes: `applyTheme`, `loadTheme`, `parseTheme`, `saveTheme`, `ThemeSetting` from Task 2.
- Produces:
  - `type ThemeState = { draft: Readonly<Ref<ThemeSetting | null>>; error: Readonly<Ref<string>>; saveError: Readonly<Ref<string>>; name: ComputedRef<string>; preview(next: ThemeSetting | null): void; commit(): string; revert(): void; reload(): void }`
  - `createThemeState(storage: Storage | null, doc: Document): ThemeState`
  - `useTheme(): ThemeState` (singleton, bound to `localStorage` and `document`)

Behavior:
- `preview(next)`: sets `draft`, clears `saveError`. `null` → default theme, `error = ""`. Invalid text → `error = parse error`, applied colors unchanged. Valid → apply, `error = ""`.
- `commit()`: returns `error` without saving if `error` is set. Otherwise sets saved = draft, writes storage, returns `""` or the storage failure message (also stored in `saveError`).
- `revert()`: `preview(saved)`.
- `reload()`: re-read storage into saved, then `revert()`.
- `name`: saved theme name, or `"Blink"` for the default.

- [ ] **Step 1: Write the failing test**

Create `src/composables/__test__/useTheme.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { createThemeState } from "../useTheme";
import { THEME_KEY, applyTheme, type ThemeSetting } from "@/lib/theme";

const paper: ThemeSetting = {
  name: "Paper",
  text: "background = #ffffff\nforeground = #111111",
  accent: 3,
};
const styleText = () => document.getElementById("blink-theme")?.textContent;

afterEach(() => {
  localStorage.clear();
  applyTheme(document, null);
});

describe("theme state", () => {
  it("applies the stored theme when created", () => {
    localStorage.setItem(THEME_KEY, JSON.stringify(paper));
    const state = createThemeState(localStorage, document);
    expect(state.name.value).toBe("Paper");
    expect(styleText()).toContain("--term-bg:#ffffff;");
  });

  it("previews, then reverts to the default", () => {
    const state = createThemeState(localStorage, document);
    state.preview(paper);
    expect(document.documentElement.dataset.theme).toBe("ghostty");
    state.revert();
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(state.draft.value).toBeNull();
    expect(localStorage.getItem(THEME_KEY)).toBeNull();
  });

  it("keeps the last valid colors while the text is invalid", () => {
    const state = createThemeState(localStorage, document);
    state.preview(paper);
    state.preview({ ...paper, text: "foreground = white" });
    expect(state.error.value).toBe("line 1: foreground is not a hex colour");
    expect(styleText()).toContain("--term-bg:#ffffff;");
    expect(state.commit()).toBe("line 1: foreground is not a hex colour");
    expect(localStorage.getItem(THEME_KEY)).toBeNull();
  });

  it("commits the draft to storage", () => {
    const state = createThemeState(localStorage, document);
    state.preview(paper);
    expect(state.commit()).toBe("");
    expect(JSON.parse(localStorage.getItem(THEME_KEY)!)).toEqual(paper);
    expect(state.name.value).toBe("Paper");
    state.preview(null);
    expect(state.commit()).toBe("");
    expect(localStorage.getItem(THEME_KEY)).toBeNull();
    expect(state.name.value).toBe("Blink");
  });

  it("reports a storage failure and keeps the theme for the session", () => {
    const full = {
      getItem: () => null,
      setItem: () => {
        throw new Error("quota");
      },
      removeItem: () => {},
    } as unknown as Storage;
    const state = createThemeState(full, document);
    state.preview(paper);
    expect(state.commit()).toBe(
      "Theme not saved: local storage is full or disabled.",
    );
    expect(state.saveError.value).toBe(
      "Theme not saved: local storage is full or disabled.",
    );
    state.revert();
    expect(styleText()).toContain("--term-bg:#ffffff;");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun run test src/composables/__test__/useTheme.test.ts`
Expected: FAIL, cannot resolve `../useTheme`.

- [ ] **Step 3: Write `src/composables/useTheme.ts`**

```ts
import { computed, readonly, ref, type ComputedRef, type Ref } from "vue";
import {
  applyTheme,
  loadTheme,
  parseTheme,
  saveTheme,
  type ThemeSetting,
} from "@/lib/theme";

export type ThemeState = {
  /** Theme shown now: the saved theme, or an unsaved preview. */
  draft: Readonly<Ref<ThemeSetting | null>>;
  /** Why the draft text does not parse, or "". */
  error: Readonly<Ref<string>>;
  /** Why the last commit did not reach storage, or "". */
  saveError: Readonly<Ref<string>>;
  /** Saved theme name for the status bar. */
  name: ComputedRef<string>;
  preview(next: ThemeSetting | null): void;
  commit(): string;
  revert(): void;
  reload(): void;
};

const storageFailure = "Theme not saved: local storage is full or disabled.";

export function createThemeState(
  storage: Storage | null,
  doc: Document,
): ThemeState {
  const saved = ref<ThemeSetting | null>(loadTheme(storage));
  const draft = ref<ThemeSetting | null>(saved.value);
  const error = ref("");
  const saveError = ref("");

  function preview(next: ThemeSetting | null) {
    draft.value = next;
    saveError.value = "";
    if (!next) {
      error.value = "";
      applyTheme(doc, null);
      return;
    }
    const parsed = parseTheme(next);
    if (!parsed.ok) {
      error.value = parsed.error;
      return;
    }
    error.value = "";
    applyTheme(doc, parsed.palette, next.accent);
  }

  function commit() {
    if (error.value) return error.value;
    saved.value = draft.value;
    try {
      if (!storage) throw new Error("no storage");
      saveTheme(storage, draft.value);
      saveError.value = "";
    } catch {
      saveError.value = storageFailure;
    }
    return saveError.value;
  }

  function revert() {
    preview(saved.value);
  }

  function reload() {
    saved.value = loadTheme(storage);
    revert();
  }

  revert();
  return {
    draft: readonly(draft) as Readonly<Ref<ThemeSetting | null>>,
    error: readonly(error),
    saveError: readonly(saveError),
    name: computed(() => saved.value?.name ?? "Blink"),
    preview,
    commit,
    revert,
    reload,
  };
}

function localStore(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    // Some webviews throw on access when storage is disabled.
    return null;
  }
}

let shared: ThemeState | undefined;

/** The app theme. Call once before mount so the stored theme paints first. */
export function useTheme(): ThemeState {
  return (shared ??= createThemeState(localStore(), document));
}
```

- [ ] **Step 4: Apply the theme at start**

Replace `src/main.ts` with:

```ts
import { createApp } from "vue";
import App from "./App.vue";
import { useTheme } from "./composables/useTheme";
import "./style.css";

useTheme();
createApp(App).mount("#app");
```

- [ ] **Step 5: Run tests and build**

Run: `bun run test src/composables/__test__/useTheme.test.ts && bun run build`
Expected: PASS (5 tests); build succeeds.

- [ ] **Step 6: Commit**

```bash
bunx prettier --write src/composables/useTheme.ts src/composables/__test__/useTheme.test.ts src/main.ts
git add src/composables/useTheme.ts src/composables/__test__/useTheme.test.ts src/main.ts
git commit -m "feat: theme preview, commit and revert state"
```

---

### Task 4: Ghostty theme files from the backend

**Files:**
- Create: `src-tauri/src/ghostty_themes.rs`
- Modify: `src-tauri/src/lib.rs` (module declaration near `#[cfg(test)] mod request_tests;`, `generate_handler!` list)
- Create: `src/lib/ghostty-themes.ts`

**Interfaces:**
- Produces (Rust): `#[tauri::command] list_ghostty_themes() -> Vec<String>`, `#[tauri::command] read_ghostty_theme(name: String) -> Result<String, String>`
- Produces (TS):
  - `canReadGhosttyThemes: boolean` (true only in Tauri)
  - `listGhosttyThemes(): Promise<string[]>`
  - `readGhosttyTheme(name: string): Promise<string>`

- [ ] **Step 1: Write the module with failing tests**

Create `src-tauri/src/ghostty_themes.rs`:

```rust
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const LIMIT: u64 = 64 * 1024;
const BUNDLED: &str = "/Applications/Ghostty.app/Contents/Resources/ghostty/themes";

/// User themes first, so they win over bundled themes with the same name.
fn theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".config/ghostty/themes"));
    }
    dirs.push(PathBuf::from(BUNDLED));
    dirs
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.contains("..")
}

fn list(dirs: &[PathBuf]) -> Vec<String> {
    let mut names = BTreeSet::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let is_file = entry.file_type().map(|kind| kind.is_file()).unwrap_or(false);
            let Ok(name) = entry.file_name().into_string() else { continue };
            if is_file && valid_name(&name) && !name.starts_with('.') {
                names.insert(name);
            }
        }
    }
    names.into_iter().collect()
}

fn read(dirs: &[PathBuf], name: &str) -> Result<String, String> {
    if !valid_name(name) {
        return Err("Invalid theme name.".into());
    }
    for dir in dirs {
        let path: &Path = &dir.join(name);
        let Ok(meta) = fs::metadata(path) else { continue };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > LIMIT {
            return Err("Theme file is larger than 64 KiB.".into());
        }
        return fs::read_to_string(path).map_err(|error| format!("Could not read theme: {error}"));
    }
    Err(format!("Theme {name} not found."))
}

#[tauri::command]
pub fn list_ghostty_themes() -> Vec<String> {
    list(&theme_dirs())
}

#[tauri::command]
pub fn read_ghostty_theme(name: String) -> Result<String, String> {
    read(&theme_dirs(), &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> (tempfile::TempDir, tempfile::TempDir, Vec<PathBuf>) {
        let user = tempfile::tempdir().unwrap();
        let bundled = tempfile::tempdir().unwrap();
        let paths = vec![user.path().to_path_buf(), bundled.path().to_path_buf()];
        (user, bundled, paths)
    }

    #[test]
    fn lists_sorted_unique_names_and_skips_missing_dirs() {
        let (user, bundled, mut paths) = dirs();
        fs::write(user.path().join("Zenburn"), "background = #3f3f3f").unwrap();
        fs::write(bundled.path().join("Zenburn"), "background = #000000").unwrap();
        fs::write(bundled.path().join("Monokai Pro"), "background = #262427").unwrap();
        fs::write(bundled.path().join(".DS_Store"), "").unwrap();
        fs::create_dir(bundled.path().join("folder")).unwrap();
        paths.push(PathBuf::from("/missing/ghostty/themes"));
        assert_eq!(list(&paths), vec!["Monokai Pro", "Zenburn"]);
    }

    #[test]
    fn user_theme_wins_over_bundled_theme() {
        let (user, bundled, paths) = dirs();
        fs::write(user.path().join("Zenburn"), "background = #3f3f3f").unwrap();
        fs::write(bundled.path().join("Zenburn"), "background = #000000").unwrap();
        assert_eq!(read(&paths, "Zenburn").unwrap(), "background = #3f3f3f");
    }

    #[test]
    fn rejects_path_names_large_files_and_missing_themes() {
        let (_user, bundled, paths) = dirs();
        for name in ["", "../secret", "a/b", "a\\b", ".."] {
            assert_eq!(read(&paths, name), Err("Invalid theme name.".into()));
        }
        fs::write(bundled.path().join("Huge"), vec![b'#'; 64 * 1024 + 1]).unwrap();
        assert_eq!(read(&paths, "Huge"), Err("Theme file is larger than 64 KiB.".into()));
        assert_eq!(read(&paths, "Nope"), Err("Theme Nope not found.".into()));
    }
}
```

In `src-tauri/src/lib.rs`, add near the other module declarations (next to `mod app_state;` — find it with `grep -n "^mod\|^pub mod" src-tauri/src/lib.rs`):

```rust
mod ghostty_themes;
```

In the `tauri::generate_handler![...]` list, after `app_state::finish_app_exit`, add:

```rust
            app_state::finish_app_exit,
            ghostty_themes::list_ghostty_themes,
            ghostty_themes::read_ghostty_theme
```

(Replace the existing last line `app_state::finish_app_exit` so the commas are correct.)

- [ ] **Step 2: Run Rust tests**

Run: `cd src-tauri && cargo test ghostty_themes`
Expected: PASS (3 tests). If a test fails, fix the implementation, not the test.

- [ ] **Step 3: Write the frontend wrapper**

Create `src/lib/ghostty-themes.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { nativeTransport } from "./transport";

/** Theme files live on disk, so only the desktop app can list them. */
export const canReadGhosttyThemes = nativeTransport;

export const listGhosttyThemes = () => invoke<string[]>("list_ghostty_themes");

export const readGhosttyTheme = (name: string) =>
  invoke<string>("read_ghostty_theme", { name });
```

- [ ] **Step 4: Verify build**

Run: `bun run build && (cd src-tauri && cargo check)`
Expected: both succeed.

- [ ] **Step 5: Commit**

```bash
bunx prettier --write src/lib/ghostty-themes.ts
git add src-tauri/src/ghostty_themes.rs src-tauri/src/lib.rs src/lib/ghostty-themes.ts
git commit -m "feat: list and read installed Ghostty themes"
```

---

### Task 5: Theme section in Application Settings

**Files:**
- Create: `src/components/ThemeSettings.vue`
- Modify: `src/components/ApplicationSettingsDialog.vue` (script: imports, watch, `save`; template: after the Workspace `<section>`)
- Test: `src/components/__test__/ThemeSettings.test.ts`
- Test: `src/components/__test__/ApplicationSettingsDialog.test.ts`

**Interfaces:**
- Consumes: `useTheme()` (Task 3), `accentSlots`, `AccentSlot` (Task 2), `canReadGhosttyThemes`, `listGhosttyThemes`, `readGhosttyTheme` (Task 4).
- Produces: `ThemeSettings.vue` with no props. DOM hooks: `#app-theme` select, `[data-theme-colors]` textarea (label `Ghostty colors`), `#app-theme-accent` select, `[data-theme-reset]` button, `[data-theme-error]`, `[data-theme-read-error]`.

- [ ] **Step 1: Write the failing component test**

Create `src/components/__test__/ThemeSettings.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import ThemeSettings from "../ThemeSettings.vue";
import { useTheme } from "@/composables/useTheme";

vi.mock("@/lib/ghostty-themes", () => ({
  canReadGhosttyThemes: true,
  listGhosttyThemes: vi.fn(async () => ["Monokai Pro"]),
  readGhosttyTheme: vi.fn(async (name: string) => {
    if (name !== "Monokai Pro") throw "Theme not found.";
    return "background = #262427\nforeground = #fcfcfa\npalette = 5=#a392e8\n";
  }),
}));

beforeEach(() => {
  localStorage.clear();
  useTheme().reload();
});
afterEach(() => useTheme().reload());

describe("ThemeSettings", () => {
  it("fills the colors from an installed theme and previews it", async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body });
    await flushPromises();
    await wrapper.get("#app-theme").setValue("Monokai Pro");
    await flushPromises();
    expect(
      wrapper.get<HTMLTextAreaElement>("[data-theme-colors]").element.value,
    ).toContain("#262427");
    expect(document.documentElement.dataset.theme).toBe("ghostty");
    expect(useTheme().draft.value?.name).toBe("Monokai Pro");
    wrapper.unmount();
  });

  it("names edited text Custom and changes the accent slot", async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body });
    await flushPromises();
    await wrapper
      .get("[data-theme-colors]")
      .setValue("background = #101010\npalette = 5=#ff00ff");
    await wrapper.get("#app-theme-accent").setValue("5");
    expect(useTheme().draft.value).toMatchObject({ name: "Custom", accent: 5 });
    expect(document.getElementById("blink-theme")?.textContent).toContain(
      "--term-accent:#ff00ff;",
    );
    wrapper.unmount();
  });

  it("shows parse errors and resets to the default", async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body });
    await flushPromises();
    await wrapper.get("[data-theme-colors]").setValue("foreground = white");
    expect(wrapper.get("[data-theme-error]").text()).toBe(
      "line 1: foreground is not a hex colour",
    );
    await wrapper.get("[data-theme-reset]").trigger("click");
    expect(wrapper.find("[data-theme-error]").exists()).toBe(false);
    expect(useTheme().draft.value).toBeNull();
    expect(document.documentElement.dataset.theme).toBeUndefined();
    wrapper.unmount();
  });

  it("clearing the text returns to the default theme", async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body });
    await flushPromises();
    await wrapper.get("[data-theme-colors]").setValue("background = #101010");
    await wrapper.get("[data-theme-colors]").setValue("");
    expect(useTheme().draft.value).toBeNull();
    expect(wrapper.find("[data-theme-error]").exists()).toBe(false);
    wrapper.unmount();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun run test src/components/__test__/ThemeSettings.test.ts`
Expected: FAIL, cannot resolve `../ThemeSettings.vue`.

- [ ] **Step 3: Write `src/components/ThemeSettings.vue`**

```vue
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useTheme } from "@/composables/useTheme";
import { accentSlots, type AccentSlot } from "@/lib/theme";
import {
  canReadGhosttyThemes,
  listGhosttyThemes,
  readGhosttyTheme,
} from "@/lib/ghostty-themes";

const { draft, error, saveError, preview } = useTheme();
const names = ref<string[]>([]);
const readError = ref("");
const accent = computed(() => draft.value?.accent ?? 3);
const message = computed(() => error.value || saveError.value);

onMounted(async () => {
  if (!canReadGhosttyThemes) return;
  try {
    names.value = await listGhosttyThemes();
  } catch {
    readError.value = "Could not list Ghostty themes.";
  }
});

async function choose(name: string) {
  readError.value = "";
  if (!name) return preview(null);
  try {
    const text = await readGhosttyTheme(name);
    preview({ name, text, accent: accent.value });
  } catch (reason) {
    readError.value = String(reason);
  }
}
function edit(text: string) {
  if (!text.trim()) return preview(null);
  preview({ name: "Custom", text, accent: accent.value });
}
function setAccent(value: AccentSlot) {
  if (draft.value) preview({ ...draft.value, accent: value });
}
</script>

<template>
  <section class="grid gap-2 border-t border-border pt-3 text-xs">
    <h3
      class="font-semibold uppercase tracking-[0.07em] text-muted-foreground"
    >
      Theme
    </h3>
    <div v-if="canReadGhosttyThemes" class="grid gap-1.5 text-muted-foreground">
      <label for="app-theme">Ghostty theme</label>
      <select
        id="app-theme"
        :value="draft?.name ?? ''"
        class="h-8 w-full min-w-0 border border-input rounded-sm px-2 bg-background text-foreground font-mono"
        @change="choose(($event.target as HTMLSelectElement).value)"
      >
        <option value="">Blink (default)</option>
        <option v-if="draft && !names.includes(draft.name)" :value="draft.name">
          {{ draft.name }}
        </option>
        <option v-for="name in names" :key="name" :value="name">
          {{ name }}
        </option>
      </select>
      <p
        v-if="readError"
        class="text-destructive font-mono"
        role="alert"
        data-theme-read-error
      >
        {{ readError }}
      </p>
    </div>
    <div class="grid gap-1.5 text-muted-foreground">
      <label for="app-theme-colors">Ghostty colors</label>
      <textarea
        id="app-theme-colors"
        :value="draft?.text ?? ''"
        rows="6"
        class="min-h-28 w-full resize-y p-2 border border-input rounded-sm text-foreground bg-background font-mono text-xs leading-relaxed"
        placeholder="background = #1e1e1e&#10;foreground = #d4d4d4&#10;palette = 3=#dcdcaa"
        data-theme-colors
        spellcheck="false"
        autocomplete="off"
        @input="edit(($event.target as HTMLTextAreaElement).value)"
      />
      <p
        v-if="message"
        class="text-destructive font-mono"
        role="alert"
        data-theme-error
      >
        {{ message }}
      </p>
    </div>
    <div class="flex items-end gap-3">
      <div class="grid flex-1 gap-1.5 text-muted-foreground">
        <label for="app-theme-accent">Accent</label>
        <select
          id="app-theme-accent"
          :value="accent"
          :disabled="!draft"
          class="h-8 w-full min-w-0 border border-input rounded-sm px-2 bg-background text-foreground font-mono disabled:opacity-50"
          @change="
            setAccent(
              Number(($event.target as HTMLSelectElement).value) as AccentSlot,
            )
          "
        >
          <option
            v-for="slot in accentSlots"
            :key="slot.value"
            :value="slot.value"
          >
            {{ slot.label }} ({{ slot.value }})
          </option>
        </select>
      </div>
      <button
        type="button"
        class="h-8 px-3 border border-border rounded-sm bg-background font-mono text-xs disabled:opacity-50"
        data-theme-reset
        :disabled="!draft"
        @click="preview(null)"
      >
        Reset to default
      </button>
    </div>
  </section>
</template>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bun run test src/components/__test__/ThemeSettings.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Write the failing dialog tests**

In `src/components/__test__/ApplicationSettingsDialog.test.ts`:

Change the imports to:

```ts
import { beforeEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import ApplicationSettingsDialog from "../ApplicationSettingsDialog.vue";
import { useTheme } from "@/composables/useTheme";
import { THEME_KEY } from "@/lib/theme";

beforeEach(() => {
  localStorage.clear();
  useTheme().reload();
});

function renderOpen() {
  return mount(ApplicationSettingsDialog, {
    props: { definitions: {}, open: true },
    attachTo: document.body,
    global: {
      stubs: {
        DialogPortal: { template: "<slot />" },
        TooltipPortal: { template: "<slot />" },
      },
    },
  });
}
const paper = "background = #ffffff\nforeground = #111111";
```

Append inside the `describe("ApplicationSettingsDialog", ...)` block:

```ts
  it("restores the saved theme when closed without saving", async () => {
    const wrapper = renderOpen();
    await nextTick();
    await wrapper.get("[data-theme-colors]").setValue(paper);
    expect(document.documentElement.dataset.theme).toBe("ghostty");

    const cancel = wrapper.findAll("button").find((b) => b.text() === "Cancel")!;
    await cancel.trigger("click");
    expect(wrapper.emitted("update:open")?.at(-1)).toEqual([false]);
    await wrapper.setProps({ open: false });

    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(localStorage.getItem(THEME_KEY)).toBeNull();
    wrapper.unmount();
  });

  it("stores the theme on save", async () => {
    const wrapper = renderOpen();
    await nextTick();
    await wrapper.get("[data-theme-colors]").setValue(paper);
    await wrapper.get("[data-save-application-settings]").trigger("click");
    await wrapper.setProps({ open: false });

    expect(JSON.parse(localStorage.getItem(THEME_KEY)!)).toMatchObject({
      name: "Custom",
      text: paper,
      accent: 3,
    });
    expect(document.documentElement.dataset.theme).toBe("ghostty");
    wrapper.unmount();
  });

  it("does not save while the Ghostty text is invalid", async () => {
    const wrapper = renderOpen();
    await nextTick();
    await wrapper.get("[data-theme-colors]").setValue("foreground = white");
    await wrapper.get("[data-save-application-settings]").trigger("click");

    expect(wrapper.emitted("save")).toBeUndefined();
    expect(wrapper.get("[data-theme-error]").text()).toContain("line 1");
    wrapper.unmount();
  });
```

- [ ] **Step 6: Run to verify the new dialog tests fail**

Run: `bun run test src/components/__test__/ApplicationSettingsDialog.test.ts`
Expected: the 3 new tests FAIL (`[data-theme-colors]` not found); existing tests PASS.

- [ ] **Step 7: Integrate into the dialog**

In `src/components/ApplicationSettingsDialog.vue` script:

Add imports after `import HelpTooltip from "./HelpTooltip.vue";`:

```ts
import ThemeSettings from "./ThemeSettings.vue";
import { useTheme } from "../composables/useTheme";
```

After `const preferences = ref(defaultPreferences());`, add:

```ts
const theme = useTheme();
// Theme edits preview live. Closing without a successful save restores the
// saved theme.
let committed = false;
watch(
  () => props.open,
  (open, wasOpen) => {
    if (open && !wasOpen) {
      committed = false;
      theme.revert();
    } else if (!open && wasOpen && !committed) theme.revert();
  },
);
```

Replace `save()` with:

```ts
function save() {
  const definitions = parseDefinitions();
  if (!definitions || theme.error.value) return;
  emit("save", definitions, { ...preferences.value });
  if (theme.commit()) return;
  committed = true;
  emit("update:open", false);
}
```

In the template, directly after the Workspace `</section>` (the section that contains "Confirm before closing drafts"), add:

```vue
            <ThemeSettings />
```

- [ ] **Step 8: Run tests**

Run: `bun run test src/components/__test__/ApplicationSettingsDialog.test.ts src/components/__test__/ThemeSettings.test.ts`
Expected: all PASS.

- [ ] **Step 9: Commit**

```bash
bunx prettier --write src/components/ThemeSettings.vue src/components/ApplicationSettingsDialog.vue src/components/__test__/ThemeSettings.test.ts src/components/__test__/ApplicationSettingsDialog.test.ts
git add src/components/ThemeSettings.vue src/components/ApplicationSettingsDialog.vue src/components/__test__/ThemeSettings.test.ts src/components/__test__/ApplicationSettingsDialog.test.ts
git commit -m "feat: Ghostty theme settings with live preview"
```

---

### Task 6: Command center

**Files:**
- Create: `src/lib/command-center.ts`
- Create: `src/components/CommandCenter.vue`
- Test: `src/lib/__test__/command-center.test.ts`
- Test: `src/components/__test__/CommandCenter.test.ts`

**Interfaces:**
- Consumes: `RequestSession`, `sessionLabel` from `src/lib/session.ts`; `RequestGroup` from `src/lib/groups.ts` (`{ id, name, parentId, collapsed, ... }`).
- Produces:
  - `type RequestMatch = { id: number; method: string; label: string; url: string; groupPath: string }`
  - `groupPath(groups: RequestGroup[], groupId: number | null): string` (names joined with `" / "`, root first)
  - `matchRequests(sessions: RequestSession[], groups: RequestGroup[], query: string): RequestMatch[]`
  - `CommandCenter.vue`: props `{ sessions: RequestSession[]; groups: RequestGroup[] }`, emits `select: [id: number]`, exposes `show(): Promise<void>`. Open popover root has `data-surface="command-center"`.

- [ ] **Step 1: Write the failing library test**

Create `src/lib/__test__/command-center.test.ts`:

```ts
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
    expect(matchRequests(all, groups, "status.example").map((m) => m.id)).toEqual(
      [health.id],
    );
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `bun run test src/lib/__test__/command-center.test.ts`
Expected: FAIL, cannot resolve `../command-center`.

- [ ] **Step 3: Write `src/lib/command-center.ts`**

```ts
import type { RequestGroup } from "./groups";
import { sessionLabel, type RequestSession } from "./session";

export type RequestMatch = {
  id: number;
  method: string;
  label: string;
  url: string;
  groupPath: string;
};

/** Group names from the root to `groupId`, joined with " / ". */
export function groupPath(groups: RequestGroup[], groupId: number | null) {
  const byId = new Map(groups.map((group) => [group.id, group]));
  const names: string[] = [];
  const seen = new Set<number>();
  let group = groupId === null ? undefined : byId.get(groupId);
  while (group && !seen.has(group.id)) {
    seen.add(group.id);
    names.unshift(group.name);
    group = group.parentId === null ? undefined : byId.get(group.parentId);
  }
  return names.join(" / ");
}

export function matchRequests(
  sessions: RequestSession[],
  groups: RequestGroup[],
  query: string,
): RequestMatch[] {
  const needle = query.trim().toLowerCase();
  return sessions
    .map((session) => ({
      id: session.id,
      method: session.draft.method,
      label: sessionLabel(session),
      url: session.draft.url,
      groupPath: groupPath(groups, session.groupId),
    }))
    .filter(
      (match) =>
        !needle ||
        [match.label, match.url, match.groupPath].some((text) =>
          text.toLowerCase().includes(needle),
        ),
    );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `bun run test src/lib/__test__/command-center.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing component test**

Create `src/components/__test__/CommandCenter.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import CommandCenter from "../CommandCenter.vue";
import { createSession } from "@/lib/session";
import type { RequestGroup } from "@/lib/groups";

const groups: RequestGroup[] = [
  { id: 1, name: "Platform", parentId: null, collapsed: false },
];
function sessions() {
  const users = createSession();
  users.draft.url = "https://api.example.test/users";
  users.groupId = 1;
  const health = createSession();
  health.draft.url = "https://api.example.test/health";
  return [users, health];
}

function render() {
  const list = sessions();
  const wrapper = mount(CommandCenter, {
    props: { sessions: list, groups },
    attachTo: document.body,
  });
  return { wrapper, list };
}

describe("CommandCenter", () => {
  it("opens from the trigger and focuses the search input", async () => {
    const { wrapper } = render();
    await wrapper.get("[data-command-center-trigger]").trigger("click");
    const input = wrapper.get('[role="combobox"]');
    expect(document.activeElement).toBe(input.element);
    expect(wrapper.findAll('[role="option"]')).toHaveLength(2);
    wrapper.unmount();
  });

  it("moves with arrow keys and selects with Enter", async () => {
    const { wrapper, list } = render();
    await (wrapper.vm as unknown as { show(): Promise<void> }).show();
    const input = wrapper.get('[role="combobox"]');
    await input.trigger("keydown", { key: "ArrowDown" });
    expect(input.attributes("aria-activedescendant")).toBe(
      `command-center-option-${list[1].id}`,
    );
    await input.trigger("keydown", { key: "ArrowDown" });
    expect(input.attributes("aria-activedescendant")).toBe(
      `command-center-option-${list[0].id}`,
    );
    await input.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("select")).toEqual([[list[0].id]]);
    expect(wrapper.find('[role="combobox"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("filters by group, shows an empty result, and closes on Escape", async () => {
    const { wrapper, list } = render();
    await wrapper.get("[data-command-center-trigger]").trigger("click");
    const input = wrapper.get('[role="combobox"]');
    await input.setValue("platform");
    expect(wrapper.findAll('[role="option"]')).toHaveLength(1);
    expect(wrapper.get('[role="option"]').text()).toContain("Platform");
    await input.setValue("zzz");
    expect(wrapper.text()).toContain("No matching requests");
    await input.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("select")).toBeUndefined();
    await input.trigger("keydown", { key: "Escape" });
    expect(wrapper.find('[role="combobox"]').exists()).toBe(false);
    await wrapper.vm.$nextTick();
    expect(document.activeElement).toBe(
      wrapper.get("[data-command-center-trigger]").element,
    );
    expect(list).toHaveLength(2);
    wrapper.unmount();
  });
});
```

- [ ] **Step 6: Run to verify it fails**

Run: `bun run test src/components/__test__/CommandCenter.test.ts`
Expected: FAIL, cannot resolve `../CommandCenter.vue`.

- [ ] **Step 7: Write `src/components/CommandCenter.vue`**

Focus rule: on Escape or outside click, focus returns to the element that was focused before opening; if that was `body` or is gone, focus the trigger.

```vue
<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { Search } from "lucide-vue-next";
import type { RequestGroup } from "@/lib/groups";
import type { RequestSession } from "@/lib/session";
import { matchRequests } from "@/lib/command-center";

const props = defineProps<{
  sessions: RequestSession[];
  groups: RequestGroup[];
}>();
const emit = defineEmits<{ select: [id: number] }>();

const open = ref(false);
const query = ref("");
const index = ref(0);
const input = ref<HTMLInputElement>();
const trigger = ref<HTMLButtonElement>();
let opener: HTMLElement | null = null;
const matches = computed(() =>
  matchRequests(props.sessions, props.groups, query.value),
);
watch(query, () => (index.value = 0));

async function show() {
  opener = document.activeElement as HTMLElement | null;
  query.value = "";
  index.value = 0;
  open.value = true;
  await nextTick();
  input.value?.focus();
}
async function hide() {
  open.value = false;
  await nextTick();
  const target =
    opener && opener !== document.body && opener.isConnected
      ? opener
      : trigger.value;
  target?.focus();
}
function choose(id: number) {
  open.value = false;
  emit("select", id);
}
function onKey(event: KeyboardEvent) {
  const count = matches.value.length;
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    void hide();
  } else if (event.key === "ArrowDown" && count) {
    event.preventDefault();
    index.value = (index.value + 1) % count;
  } else if (event.key === "ArrowUp" && count) {
    event.preventDefault();
    index.value = (index.value - 1 + count) % count;
  } else if (event.key === "Enter" && count) {
    event.preventDefault();
    choose(matches.value[index.value].id);
  }
}
defineExpose({ show });
</script>

<template>
  <div class="relative w-[min(420px,50vw)]" data-command-center>
    <button
      v-show="!open"
      ref="trigger"
      type="button"
      class="flex h-6.5 w-full items-center gap-2 rounded-md border border-border bg-muted px-2.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
      data-command-center-trigger
      aria-label="Search requests"
      title="Search requests · Cmd/Ctrl+P"
      @click="show"
    >
      <Search :size="13" aria-hidden="true" />
      <span class="flex-1 text-left">Search requests</span>
      <kbd class="text-[10px]">⌘P</kbd>
    </button>
    <template v-if="open">
      <button
        type="button"
        tabindex="-1"
        class="fixed inset-0 z-40 cursor-default"
        aria-label="Close request search"
        @click="hide"
      />
      <div
        class="absolute inset-x-0 top-0 z-50 overflow-hidden rounded-md border border-border bg-secondary shadow-[0_8px_24px_oklch(0_0_0/0.4)]"
        data-surface="command-center"
      >
        <input
          ref="input"
          v-model="query"
          role="combobox"
          aria-label="Search requests"
          aria-autocomplete="list"
          aria-expanded="true"
          aria-controls="command-center-list"
          :aria-activedescendant="
            matches.length
              ? `command-center-option-${matches[index].id}`
              : undefined
          "
          class="h-7 w-full rounded-none border-0 border-b border-border bg-transparent px-2.5 text-xs outline-none focus-visible:outline-none"
          placeholder="Search requests by name, URL, or group"
          spellcheck="false"
          autocomplete="off"
          @keydown="onKey"
        />
        <ul
          id="command-center-list"
          role="listbox"
          aria-label="Requests"
          class="max-h-72 overflow-auto py-1"
        >
          <li
            v-for="(match, i) in matches"
            :id="`command-center-option-${match.id}`"
            :key="match.id"
            role="option"
            :aria-selected="i === index"
            class="flex cursor-pointer items-center gap-2 px-2.5 py-1 text-xs aria-selected:bg-accent"
            @mousedown.prevent="choose(match.id)"
            @mousemove="index = i"
          >
            <span class="w-14 shrink-0 font-mono text-[10px] text-primary">{{
              match.method
            }}</span>
            <span class="min-w-0 flex-1 truncate">{{ match.label }}</span>
            <span
              v-if="match.groupPath"
              class="max-w-[40%] truncate text-muted-foreground"
              >{{ match.groupPath }}</span
            >
          </li>
        </ul>
        <p
          v-if="!matches.length"
          class="px-2.5 pb-2 text-xs text-muted-foreground"
          role="status"
        >
          No matching requests
        </p>
      </div>
    </template>
  </div>
</template>
```

- [ ] **Step 8: Run to verify it passes**

Run: `bun run test src/components/__test__/CommandCenter.test.ts src/lib/__test__/command-center.test.ts`
Expected: PASS. If the focus assertion after Escape fails because `document.activeElement` was `body` before opening via `show()`, check the fallback-to-trigger rule in `hide()`.

- [ ] **Step 9: Commit**

```bash
bunx prettier --write src/lib/command-center.ts src/components/CommandCenter.vue src/lib/__test__/command-center.test.ts src/components/__test__/CommandCenter.test.ts
git add src/lib/command-center.ts src/components/CommandCenter.vue src/lib/__test__/command-center.test.ts src/components/__test__/CommandCenter.test.ts
git commit -m "feat: command center request search"
```

---

### Task 7: VS Code-style shell

**Files:**
- Create: `src/components/ActivityBar.vue`
- Modify: `src/components/RequestBrowser.vue` (props/emits/imports at lines 1-66; template `<aside>` and `<header>` at lines 292-344; every `!collapsed` condition)
- Modify: `src/components/RequestTabs.vue` (imports line 3; add a button after the `data-new-request` button near line 188)
- Modify: `src/App.vue` (script imports and `onKey`; whole template)
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`
- Modify tests: `src/__test__/App.test.ts`, `src/components/__test__/RequestBrowserContextMenu.test.ts:33` (only if `collapsed` there is a component prop)

**Interfaces:**
- Consumes: `CommandCenter` (Task 6: props `sessions`, `groups`; emit `select`; exposed `show()`), `useTheme().name` (Task 3), `nativeTransport` from `src/lib/transport.ts`.
- Produces:
  - `ActivityBar.vue`: props `{ browserOpen: boolean }`; emits `toggleBrowser: []`, `openSettings: [event: MouseEvent]`. Buttons `[data-activity="browser"]` (name `Request browser`, `aria-pressed`) and `[data-activity="settings"]` (name `Application settings`).
  - `RequestBrowser.vue`: removes prop `collapsed` and emit `toggleSidebar`; adds emit `collapseAllGroups: []`. Header buttons: `Add request` (`data-browser-new-request`; not named `New request`, because e2e tests match the tab strip button by that exact name), `Add top-level group`, `Group selected requests`, `Collapse all groups`.
  - `RequestTabs.vue`: button `[data-duplicate-request]`, name `Duplicate request`.
  - `App.vue`: `[data-title-bar]` (36 px), `[data-status-bar]`, `[data-theme-name]`.

- [ ] **Step 1: Write the failing App tests**

In `src/__test__/App.test.ts`, change the import line to include `nextTick`:

```ts
import { mount, flushPromises } from "@vue/test-utils";
import { nextTick } from "vue";
```

Replace the test `"collapses and restores the request browser"` with:

```ts
  it("hides and restores the request browser from the activity bar", async () => {
    const app = render();
    const toggle = app.get('[data-activity="browser"]');

    expect(toggle.attributes("aria-pressed")).toBe("true");
    await toggle.trigger("click");
    expect(toggle.attributes("aria-pressed")).toBe("false");
    expect(app.get("[data-request-browser]").isVisible()).toBe(false);
    await toggle.trigger("click");
    expect(app.get("[data-request-browser]").isVisible()).toBe(true);
  });

  it("opens the command center with Cmd+P and selects a request", async () => {
    const app = render();
    await app
      .get("[data-request-url]")
      .setValue("https://example.test/users");
    await app.get("[data-new-request]").trigger("click");

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "p", metaKey: true }),
    );
    await flushPromises();
    const input = app.get('[role="combobox"]');
    await input.setValue("users");
    await input.trigger("keydown", { key: "Enter" });
    await flushPromises();

    expect(app.get('[role="tab"][aria-selected="true"]').text()).toContain(
      "/users",
    );
  });

  it("does not open the command center while a dialog is open", async () => {
    const app = render();
    await app.get('[data-activity="settings"]').trigger("click");
    await flushPromises();
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "p", metaKey: true }),
    );
    await nextTick();
    expect(document.querySelector('[data-surface="command-center"]')).toBeNull();
  });

  it("collapses every group from the browser header", async () => {
    const app = render();
    await app.get('[aria-label="Add top-level group"]').trigger("click");
    await app.get('[aria-label="Top-level group name"]').setValue("Platform");
    await app.get(".top-level-form").trigger("submit");
    expect(app.get('[aria-label="Collapse Platform"]')).toBeTruthy();
    await app.get('[aria-label="Collapse all groups"]').trigger("click");
    expect(app.get('[aria-label="Expand Platform"]')).toBeTruthy();
  });

  it("shows the theme name in the status bar", () => {
    const app = render();
    expect(app.get("[data-theme-name]").text()).toBe("Blink");
  });
```

Before writing the implementation, confirm the group toggle label: `grep -n "Expand' : 'Collapse'" src/components/RequestBrowser.vue` shows `` `${row.group.collapsed ? 'Expand' : 'Collapse'} ${row.group.name}` ``. `sessionLabel` returns the URL path (`/users`) for this URL, so the tab text contains `/users`.

- [ ] **Step 2: Run to verify the new tests fail**

Run: `bun run test src/__test__/App.test.ts`
Expected: the 5 new tests FAIL (missing `[data-activity]`, combobox, `Collapse all groups`, `[data-theme-name]`).

- [ ] **Step 3: Write `src/components/ActivityBar.vue`**

```vue
<script setup lang="ts">
import { Files, Settings } from "lucide-vue-next";

defineProps<{ browserOpen: boolean }>();
const emit = defineEmits<{
  toggleBrowser: [];
  openSettings: [event: MouseEvent];
}>();
</script>

<template>
  <nav
    class="flex w-11 shrink-0 flex-col items-center gap-1 py-1"
    aria-label="Activity bar"
    data-activity-bar
  >
    <button
      type="button"
      class="activity-button"
      data-activity="browser"
      aria-label="Request browser"
      title="Request browser"
      aria-controls="request-browser"
      :aria-pressed="browserOpen"
      @click="emit('toggleBrowser')"
    >
      <Files :size="18" aria-hidden="true" />
    </button>
    <button
      type="button"
      class="activity-button mt-auto"
      data-activity="settings"
      aria-label="Application settings"
      title="Application settings · Cmd/Ctrl+,"
      @click="emit('openSettings', $event)"
    >
      <Settings :size="18" aria-hidden="true" />
    </button>
  </nav>
</template>

<style scoped>
@reference "../style.css";
.activity-button {
  @apply relative flex size-9 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground;
}
.activity-button[aria-pressed="true"] {
  @apply text-foreground;
}
/* Active marker on the outer edge, as in VS Code. */
.activity-button[aria-pressed="true"]::before {
  position: absolute;
  inset: 8px auto 8px -4px;
  width: 2px;
  content: "";
  background: var(--primary);
}
</style>
```

- [ ] **Step 4: Update `src/components/RequestBrowser.vue`**

Script:
- In the lucide import, remove `PanelLeftClose`, `PanelLeftOpen`, `Plus`; add `FilePlus`, `FolderInput`, `ListCollapse` (keep `FolderPlus` and the rest). Before removing `Plus`, run `grep -n "<Plus" src/components/RequestBrowser.vue`; keep `Plus` if other rows still use it.
- In `defineProps`, remove `collapsed?: boolean;`.
- In `defineEmits`, replace `toggleSidebar: [];` with `collapseAllGroups: [];`.

Template: replace the `<aside ...>` opening tag and the whole `<header>...</header>` (lines 292-344) with:

```vue
  <aside
    id="request-browser"
    class="flex flex-col overflow-hidden rounded-lg border border-border bg-muted w-61 min-w-47 max-[760px]:absolute max-[760px]:inset-y-0 max-[760px]:left-0 max-[760px]:z-40 max-[760px]:rounded-none"
    :class="{ 'max-[760px]:hidden': !mobileOpen }"
    data-request-browser
    aria-label="Request browser"
  >
    <header
      class="flex h-9 shrink-0 items-center gap-0.5 border-b border-border pl-3 pr-1.5"
    >
      <strong
        class="mr-auto font-mono text-[11px] font-bold tracking-[0.08em]"
        >REQUESTS</strong
      >
      <button
        type="button"
        class="browser-action"
        data-browser-new-request
        aria-label="Add request"
        title="Add request · Cmd/Ctrl+T"
        @click="emit('createRequest', active?.groupId ?? null)"
      >
        <FilePlus :size="14" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="browser-action"
        aria-label="Add top-level group"
        title="Add group"
        @click="startCreating(null)"
      >
        <FolderPlus :size="14" aria-hidden="true" />
      </button>
      <button
        v-if="selectedIds?.length"
        type="button"
        class="browser-action"
        aria-label="Group selected requests"
        title="Group selected requests"
        @click="startCreating(null, selectedIds)"
      >
        <FolderInput :size="14" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="browser-action"
        aria-label="Collapse all groups"
        title="Collapse all groups"
        :disabled="!groups.length"
        @click="emit('collapseAllGroups')"
      >
        <ListCollapse :size="14" aria-hidden="true" />
      </button>
    </header>
```

Remove every remaining `collapsed` prop condition:
- `v-if="!collapsed && creatingParent === null"` → `v-if="creatingParent === null"`
- `<div v-if="!collapsed" class="min-h-0 flex-1 overflow-auto py-2">` → `<div class="min-h-0 flex-1 overflow-auto py-2">`
- Check: `grep -n "[^.]collapsed\b" src/components/RequestBrowser.vue | grep -v "group.collapsed"` prints nothing.

Add at the end of the file (or into the existing `<style scoped>` if one exists; check with `grep -n "<style" src/components/RequestBrowser.vue`):

```vue
<style scoped>
@reference "../style.css";
.browser-action {
  @apply inline-flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30 pointer-coarse:size-8;
}
</style>
```

If `src/components/__test__/RequestBrowserContextMenu.test.ts:33` passes `collapsed: false` as a component prop (not inside a group object), delete that line.

- [ ] **Step 5: Add the Duplicate button to `src/components/RequestTabs.vue`**

Change line 3 to `import { CopyPlus, Plus, X } from 'lucide-vue-next';`.

Directly after the closing `</button>` of the `data-new-request` button, add:

```vue
        <button
          type="button"
          class="flex items-center justify-center shrink-0 w-9.5 text-muted-foreground border-r border-border cursor-pointer hover:bg-accent hover:text-primary pointer-coarse:w-11"
          data-duplicate-request
          aria-label="Duplicate request"
          title="Duplicate request · Cmd/Ctrl+Shift+D"
          @click="emit('duplicate')"
        >
          <CopyPlus :size="14" aria-hidden="true" />
        </button>
```

- [ ] **Step 6: Rebuild the shell in `src/App.vue`**

Script changes:

Replace the lucide import with:

```ts
import { HardDrive, PanelLeft } from "lucide-vue-next";
```

Add imports after `import HelpTooltip from "@/components/HelpTooltip.vue";`:

```ts
import ActivityBar from "@/components/ActivityBar.vue";
import CommandCenter from "@/components/CommandCenter.vue";
import { useTheme } from "@/composables/useTheme";
import { nativeTransport } from "@/lib/transport";
```

After `const sidebarCollapsed = ref(false);`, add:

```ts
const commandCenter = ref<InstanceType<typeof CommandCenter>>();
const { name: themeName } = useTheme();
// Tauri draws the macOS traffic lights over the title bar.
const macOverlay = nativeTransport && /Mac/.test(navigator.userAgent);
```

After `function select(id: number) { ... }`, add:

```ts
function selectFromSearch(id: number) {
  select(id);
  void nextTick(() => document.getElementById(`request-tab-${id}`)?.focus());
}
function collapseAllGroups() {
  for (const group of groups.value) group.collapsed = true;
}
```

In `onKey`, change the context-menu guard to also cover the command center:

```ts
    document.querySelector(
      '[data-surface="context-menu"], [data-surface="command-center"]',
    )
```

In the `else if (event.metaKey || event.ctrlKey)` block, after the `","` case, add:

```ts
    if (key === "p" && !event.shiftKey) {
      event.preventDefault();
      void commandCenter.value?.show();
    }
```

Template: replace the whole `<template>...</template>` with:

```vue
<template>
  <main
    class="flex flex-col h-full min-h-100 bg-frame max-[760px]:h-auto max-[760px]:min-h-dvh"
    :inert="closing || undefined"
  >
    <header
      class="relative flex h-9 shrink-0 items-center justify-center px-2"
      :class="{ 'pl-19.5': macOverlay }"
      data-title-bar
      data-tauri-drag-region
    >
      <Button
        variant="ghost"
        class="absolute left-2 min-[761px]:hidden"
        :aria-label="
          mobileBrowserOpen ? 'Hide request browser' : 'Show request browser'
        "
        :aria-expanded="mobileBrowserOpen"
        aria-controls="request-browser"
        @click="
          mobileBrowserOpen = !mobileBrowserOpen;
          sidebarCollapsed = false;
        "
      >
        <PanelLeft :size="14" aria-hidden="true" />
      </Button>
      <CommandCenter
        v-if="ready"
        ref="commandCenter"
        :sessions="sessions"
        :groups="groups"
        @select="selectFromSearch"
      />
    </header>
    <WorkspaceStorageNotice
      :error="storageError"
      :ready="ready"
      :exit-blocked="exitBlocked"
      @retry="ready ? flush() : restore()"
      @reset="reset"
      @quit="quitWithoutSaving"
    />
    <div
      v-if="ready"
      class="relative flex min-w-0 min-h-0 flex-1 gap-1.5 pr-1.5 max-[760px]:gap-0 max-[760px]:pr-0"
    >
      <ActivityBar
        class="max-[760px]:hidden"
        :browser-open="!sidebarCollapsed"
        @toggle-browser="sidebarCollapsed = !sidebarCollapsed"
        @open-settings="openApplicationSettings($event)"
      />
      <button
        v-if="mobileBrowserOpen"
        type="button"
        class="absolute inset-0 z-30 bg-black/50 min-[761px]:hidden"
        aria-label="Close request browser"
        @click="mobileBrowserOpen = false"
      />
      <RequestBrowser
        v-show="!sidebarCollapsed || mobileBrowserOpen"
        :mobile-open="mobileBrowserOpen"
        :sessions="sessions"
        :active-id="activeId"
        :groups="groups"
        :selected-ids="selectedRequestIds"
        :selection-anchor-id="selectionAnchorId"
        @select="select"
        @update-selection="updateSelection"
        @create-group="createGroup"
        @rename-group="renameGroup"
        @toggle-group="toggleGroup"
        @move-request="moveRequest"
        @move-requests="moveRequests"
        @reorder-group="reorderGroup"
        @delete-group="deleteGroup"
        @collapse-all-groups="collapseAllGroups"
        @open-group-settings="openGroupSettings"
        @create-request="(groupId) => create(false, groupId)"
        @duplicate-request="(id) => duplicate(id)"
        @close-request="(id) => close(id)"
        @set-request-local-auth="(id, auth) => setRequestLocalAuth(id, auth)"
      />
      <div
        class="relative flex flex-col min-w-0 min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-background max-[760px]:rounded-none max-[760px]:border-x-0"
      >
        <RequestTabs
          :sessions="sessions"
          :active-id="activeId"
          @select="select"
          @create="create()"
          @close="close"
          @duplicate="duplicate()"
        />
        <div
          v-if="closeTarget"
          class="flex items-center gap-2 px-3.5 py-1.5 bg-secondary border-b border-primary"
          role="group"
          aria-label="Confirm close request"
        >
          <p class="flex gap-1.25 min-w-0 mr-auto text-xs">
            Discard
            <strong
              class="overflow-hidden text-ellipsis whitespace-nowrap text-primary font-medium"
            >
              {{ sessionLabel(closeTarget) }}
            </strong>
            ?
          </p>
          <Button variant="ghost" data-cancel-close @click="cancelClose">
            Keep open
          </Button>
          <Button
            variant="secondary"
            data-confirm-close
            :disabled="closeTarget.busy"
            @click="close(closeTarget.id, true)"
          >
            Discard tab
          </Button>
        </div>
        <RequestWorkspace
          v-for="session in sessions"
          :key="session.id"
          :session="session"
          :active="session.id === activeId"
          :groups="groups"
          :global-definitions="globalDefinitions"
        />
      </div>
    </div>
    <footer
      class="flex items-center gap-4.5 min-h-6 shrink-0 px-3.5 font-mono text-[0.5625rem] tracking-[0.07em] text-muted-foreground bg-frame max-[760px]:gap-3 max-[760px]:flex-wrap max-[760px]:px-3 max-[760px]:py-2"
      data-status-bar
    >
      <HelpTooltip
        text="Saved on this device, including credentials and response content. Not encrypted."
      >
        <button
          type="button"
          aria-label="Local storage information"
          class="flex items-center gap-1.5 hover:text-foreground"
        >
          <HardDrive :size="11" aria-hidden="true" /><span role="status">{{
            storageStatus
          }}</span>
        </button>
      </HelpTooltip>
      <span>
        {{ sessions.length }}
        {{ sessions.length === 1 ? "REQUEST" : "REQUESTS" }}
      </span>
      <span v-if="sending" class="text-primary" role="status">
        {{ sending }} SENDING
      </span>
      <span class="ml-auto max-[760px]:hidden">30 s TIMEOUT · 4 MiB LIMIT</span>
      <span data-theme-name>{{ themeName }}</span>
    </footer>
    <GroupSettingsDialog
      :group="groupSettingsGroup"
      :preferences="preferences"
      :groups="groups"
      :sessions="sessions"
      :open="groupSettingsOpen"
      @update:open="groupSettingsOpen = $event"
      @save="handleSaveGroupSettings"
    />
    <ApplicationSettingsDialog
      :definitions="globalDefinitions"
      :preferences="preferences"
      :open="applicationSettingsOpen"
      @update:open="applicationSettingsOpen = $event"
      @save="
        (definitions, next: WorkspacePreferences) => {
          setGlobalDefinitions(definitions);
          setPreferences(next);
        }
      "
    />
  </main>
</template>
```

Note: the old footer hid the limits while sending (`v-else`). The new footer shows them on the right at all times; this is intended.

- [ ] **Step 7: Configure the Tauri window**

In `src-tauri/tauri.conf.json`, in `app.windows[0]`, after `"minHeight": 620`, add:

```json
        "minHeight": 620,
        "titleBarStyle": "Overlay",
        "hiddenTitle": true
```

In `src-tauri/capabilities/default.json`, change `permissions` to:

```json
  "permissions": [
    "core:default",
    "core:window:allow-start-dragging",
    "opener:default"
  ]
```

- [ ] **Step 8: Run the full unit suite, build, and lint**

Run: `bun run test && bun run build && bun run lint && (cd src-tauri && cargo check)`
Expected: all PASS. Fix any other unit test that still queries `Collapse request browser`, `Expand request browser`, `toggleSidebar`, or the removed header: `grep -rn "request browser\"\|toggleSidebar\|toggle-sidebar" src`.

- [ ] **Step 9: Commit**

```bash
bunx prettier --write src/App.vue src/components/ActivityBar.vue src/components/RequestBrowser.vue src/components/RequestTabs.vue src/__test__/App.test.ts
git add src/App.vue src/components/ActivityBar.vue src/components/RequestBrowser.vue src/components/RequestTabs.vue src/__test__/App.test.ts src/components/__test__ src-tauri/tauri.conf.json src-tauri/capabilities/default.json
git commit -m "feat: VS Code-style shell with activity bar and command center"
```

---

### Task 8: End-to-end tests and design doc

**Files:**
- Modify: `e2e/console.spec.ts:31-38`
- Create: `e2e/shell-theme.spec.ts`
- Modify: `DESIGN.md` (Visual system: Color strategy, Layout, Shape)

**Interfaces:**
- Consumes: DOM hooks from Tasks 5 and 7: label `Ghostty colors`, button `Application settings`, button `Save`, combobox `Search requests`, `[data-title-bar]`, `[data-theme-name]`.

- [ ] **Step 1: Update `e2e/console.spec.ts`**

Replace:

```ts
  await expect(
    page.getByRole("heading", { name: "BLINK", exact: true }),
  ).toBeVisible();
  await expect(
    page.locator("header").filter({
      has: page.getByRole("heading", { name: "BLINK", exact: true }),
    }),
  ).toHaveCSS("height", "42px");
```

with:

```ts
  await expect(page.locator("[data-title-bar]")).toHaveCSS("height", "36px");
```

- [ ] **Step 2: Write `e2e/shell-theme.spec.ts`**

```ts
import { expect, test } from "@playwright/test";

test("a pasted Ghostty palette applies and persists", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.removeItem("blink.theme"));
  await page.reload();

  await page.getByRole("button", { name: "Application settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Application Settings" });
  await dialog
    .getByLabel("Ghostty colors")
    .fill("background = #ffffff\nforeground = #111111\npalette = 3=#0055ff");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "ghostty");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();

  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
  await expect(page.locator("[data-theme-name]")).toHaveText("Custom");

  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "ghostty");
  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
});

test("cancel restores the previous theme", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.removeItem("blink.theme"));
  await page.reload();

  await page.getByRole("button", { name: "Application settings" }).click();
  const dialog = page.getByRole("dialog", { name: "Application Settings" });
  await dialog.getByLabel("Ghostty colors").fill("background = #ffffff");
  await dialog.getByRole("button", { name: "Cancel" }).click();

  await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.+/);
  await expect(page.locator("[data-theme-name]")).toHaveText("Blink");
});

test("the command center jumps to a request", async ({ page }) => {
  await page.goto("/");
  await page
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/v1/users");
  await page.getByRole("button", { name: "New request", exact: true }).click();

  await page.keyboard.press("ControlOrMeta+p");
  const search = page.getByRole("combobox", { name: "Search requests" });
  await expect(search).toBeFocused();
  await search.fill("users");
  await page.keyboard.press("Enter");

  await expect(
    page.getByRole("tab", { selected: true }),
  ).toContainText("users");
});
```

Note: the URL is filled before the second request opens, so `getByLabel("Request URL")` matches one field.

- [ ] **Step 3: Run e2e tests**

Run: `bun run test:e2e`
Expected: all specs PASS. If a spec in `e2e/` fails because it used the removed header controls (`Collapse request browser`, heading `BLINK`, header Duplicate button), update it to the new controls: `[data-activity="browser"]`, `[data-duplicate-request]` in the tab strip.

- [ ] **Step 4: Update `DESIGN.md`**

Replace the **Color strategy**, **Layout**, and **Shape** bullets in "Visual system" with:

```markdown
- **Color strategy:** Default: near-black warm surfaces, thin graphite
  dividers, off-white text, and industrial amber for primary actions and
  active tabs. Reserve muted green for successful responses and coral for
  errors. A Ghostty palette can replace the default: surfaces mix foreground
  into background, the accent comes from a chosen palette slot (default 3),
  success from slot 2, errors from slot 1, and info from slot 4. Components
  use tokens only; never hardcode colors.
- **Layout:** A 36-pixel title bar holds the centered command center
  (`Cmd/Ctrl+P`) and doubles as the window drag area; on macOS the traffic
  lights sit on its left. Below it, a 44-pixel activity bar toggles the
  244-pixel Browser card and opens Settings. The editor card holds the
  request tab strip and the resizable request and response split. Cards sit
  on a darker frame with 6-pixel gaps. A 24-pixel status bar closes the
  window. Below 900 pixels, panels stack without hiding core request
  controls. Below 760 pixels, the activity bar hides and the Browser opens
  from the title bar as an overlay.
- **Shape:** 8-pixel radius for cards and popovers, 4-pixel radius for
  controls.
```

In "Interaction rules", replace `Keep the Browser sidebar visible.` with `The activity bar can hide the Browser; keep it visible by default.`

- [ ] **Step 5: Commit**

```bash
git add e2e/console.spec.ts e2e/shell-theme.spec.ts DESIGN.md
git commit -m "test: e2e for theme and command center; update design rules"
```

- [ ] **Step 6: Manual desktop check**

Run: `bun run desktop`
Check:
1. The traffic lights do not overlap the command center.
2. Dragging the empty title bar moves the window.
3. Settings → Ghostty theme lists installed themes; choosing `Monokai Pro` recolors the app.
4. Save, quit, and restart: the theme stays; the status bar shows `Monokai Pro`.

Report any failure with a screenshot instead of claiming completion.
