# VS Code-style shell and Ghostty theme

## Goal

Give Blink an app shell modeled on the current VS Code layout (activity bar,
floating rounded panels, title bar command center, sidebar header, status bar)
and let the user color the application from a Ghostty palette.

Request, response, Browser tree, and tab behavior do not change.

## Decisions

| Topic | Decision |
| --- | --- |
| Shell parts | Activity bar, floating rounded cards, title bar command center, sidebar header, status bar. No bottom panel. |
| Title bar | Merged into the window: Tauri `titleBarStyle: "Overlay"`, hidden native title. Web and non-mac builds render the same bar as a normal header. |
| Palette source | Pick an installed Ghostty theme, or paste/edit Ghostty text. Not read from the user's Ghostty config. |
| Theme mechanism | Palette sets raw `--term-*` custom properties. `src/style.css` derives Blink tokens from them with `color-mix()`. |
| Parser | Copy term0's `ghostty.ts` into `src/lib/ghostty.ts`. Do not depend on the term0 package. |
| Storage | `localStorage` key `blink.theme`. Not part of the workspace snapshot. |
| Default | No stored theme keeps the current amber oklch tokens exactly. |

## Shell layout

Top to bottom:

1. **Title bar**, about 36 px, `bg-frame`.
   - macOS Tauri: 78 px left padding for the traffic lights.
   - Centered **command center**: rounded box, search icon, text
     `Search requests`, `⌘P` key hint.
   - Empty areas carry `data-tauri-drag-region`.
   - The BLINK wordmark and the header Duplicate button are removed.
     Duplicate stays available in the tab strip, the tab context menu, and
     `Cmd/Ctrl+Shift+D`.
   - Below 760 px, the bar also holds the Browser toggle button (current
     mobile behavior).
2. **Body**, `bg-frame`, 6 px gaps, left to right:
   - **Activity bar**, about 44 px, no card. Browser icon at the top toggles
     the sidebar (`aria-pressed`, `aria-controls="request-browser"`). Settings
     gear at the bottom opens Application Settings. Hidden below 760 px.
   - **Sidebar card**, 8 px radius, `bg-muted`. Header row: `REQUESTS` label
     and icon actions (new request, new group, collapse all groups). Below it,
     the existing Browser tree. The old in-sidebar collapse control is removed;
     the activity bar replaces it.
   - **Editor card**, 8 px radius, `bg-background`. Request tabs, close
     confirmation bar, and request workspaces, unchanged inside.
3. **Status bar**, about 24 px, `bg-frame`, no card. Left: storage status,
   request count, sending count. Right: timeout and size limits, theme name.

### Command center

- Opens on click or `Cmd/Ctrl+P`. Does not open while a dialog or context
  menu is open (same guard as the current shortcuts).
- A popover under the box with a search input and a list of all requests.
- Each row shows method, label (`sessionLabel`), and group path.
- Filter: case-insensitive substring match on label, URL, and group path.
- Arrow keys move the selection, Enter selects the request (same `select`
  path as the Browser), Esc closes and returns focus to the opener.
- Empty result shows `No matching requests`.
- ARIA: combobox input with a listbox.

## Theme

### Tokens

`src/style.css` keeps the current oklch values as the default. When a theme
is active, an adopted stylesheet sets `--term-*` on `:root` plus
`data-theme="ghostty"` on `<html>`. A `:root[data-theme="ghostty"]` rule
derives:

| Blink token | Source |
| --- | --- |
| `--background` | `--term-bg` |
| `--foreground` | `--term-fg` |
| `--muted` | fg into bg at 5 % |
| `--secondary` | fg into bg at 9 % |
| `--accent` | fg into bg at 14 % |
| `--border` | fg into bg at 18 % |
| `--input` | fg into bg at 28 % |
| `--muted-foreground` | fg into bg at 64 % |
| `--frame` | bg mixed with black at 22 % |
| `--primary` | `--term-accent` (chosen slot, default palette 3) |
| `--primary-foreground` | `--term-bg` |
| `--success` | palette 2 |
| `--destructive` | palette 1 |
| `--info` | palette 4 |
| `--selection` | `selection-background` at 35 % |

New tokens `--frame`, `--info`, and `--selection` also get default values
that match the current look. Hardcoded colors move to tokens:
`oklch(0.8 0.07 240)` in `JsonTreeView.vue`, `CodeView.vue`, and
`code-editor.ts` becomes `--info`; the selection colors in `style.css` and
`code-editor.ts` become `--selection`. Context menu shadows stay black.

`color-scheme` is `light` when the relative luminance of the background is
above 0.5, otherwise `dark`.

### Modules

- `src/lib/ghostty.ts`: `Palette`, `hexColour`, `parseGhostty(text, base)`,
  ported from term0 with its error messages (`line N: ...`).
- `src/lib/theme.ts`:
  - `type ThemeSetting = { name: string; text: string; accent: number }`
    (`accent` is 1–6).
  - `themeCss(palette, accent): string`
  - `applyTheme(doc, setting | null)`: parses, writes the adopted sheet, sets
    `data-theme` and `color-scheme`. `null` restores the default.
  - `loadTheme(storage)`, `saveTheme(storage, setting)`, `clearTheme(storage)`.
    Invalid stored JSON or text loads as the default and does not throw.
- `src/composables/useTheme.ts`: shared state (`setting`, `error`),
  `preview(setting)`, `commit()`, `revert()`, `reset()`. `main.ts` calls it
  before mount so the stored theme applies before the first paint.

### Settings UI

Application Settings gets a **Theme** section:

- **Theme** select: `Blink (default)` and the installed Ghostty theme names.
  Choosing a name reads the theme and fills the text area. Hidden when the
  Tauri API is not available.
- **Ghostty colors** text area (monospace). Invalid text shows the parser
  error below the field; the last valid palette stays in the preview.
- **Accent** select: Red (1), Green (2), Yellow (3), Blue (4), Magenta (5),
  Cyan (6). Default Yellow.
- **Reset to default** button.

Edits preview live. Save commits the theme to `localStorage` at the same time as the other settings.
Cancel, Esc, or closing the dialog restores the theme from before the dialog
opened. Save is blocked while the text is invalid; the error is visible.

### Backend

`src-tauri/src/lib.rs` adds:

- `list_ghostty_themes() -> Vec<String>`: sorted, de-duplicated file names from
  `/Applications/Ghostty.app/Contents/Resources/ghostty/themes` and
  `~/.config/ghostty/themes`. Missing directories give no entries.
- `read_ghostty_theme(name: String) -> Result<String, String>`: rejects empty
  names and names with `/`, `\`, or `..`. User themes win over bundled themes
  with the same name. Files larger than 64 KiB are rejected.

## Error handling

- Parse errors: inline under the text area, with line number.
- Theme read failure: inline under the Theme select; text area keeps its text.
- `localStorage` write failure: inline `Theme not saved: local storage is full
  or disabled.`; the preview stays applied for this session.

## Documentation

Update `DESIGN.md`: layout (activity bar, cards, command center, status bar),
shape (8 px card radius, 4 px control radius), and the theme rule
(palette-derived tokens, amber default).

## Testing

- Unit: `ghostty.ts` parser (ported term0 cases), `theme.ts` CSS output,
  luminance, load fallback for bad storage.
- Unit: command center filtering, keyboard navigation, selection.
- Unit: settings dialog preview, cancel revert, save, invalid text blocks save.
- Unit: update `App.test.ts`, `RequestBrowser.test.ts`, and other tests that
  query the removed header controls or the sidebar collapse control.
- Rust: theme name validation and user-over-bundled lookup.
- E2E: paste a palette, save, reload, colors persist; command center opens
  with `Cmd/Ctrl+P` and selects a request. Update existing specs for the
  removed header controls.

## Out of scope

- Bottom panel.
- Reading `~/.config/ghostty/config` or light/dark theme pairs.
- Theming CodeMirror syntax colors beyond the existing tokens.

## Revision 2026-09-27

1. The accent marks only markers, focus, primary actions, checkboxes, the resize handle, progress, and the close-confirm bar; everything else uses neutral or semantic tokens.
2. Semantic colors come from the palette: new `--warning` (slot 3) and `--keyword` (slot 5).
3. HTTP method colors are defined once, in CSS (`.method[data-method]`).
4. The default accent slot is Blue (4); stored themes keep their stored accent.
5. The title bar is 40 px; macOS traffic lights are vertically centered and inset; the command center is truly centered.
6. Below 761 px, a Settings button in the title bar opens Application Settings.
