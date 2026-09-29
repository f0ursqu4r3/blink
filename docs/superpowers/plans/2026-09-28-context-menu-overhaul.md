# Context Menu Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Blink's hand-written menu primitives with the official shadcn-vue `context-menu` and `dropdown-menu`, restyle them like VS Code, and give every surface its full action set.

**Architecture:** The shadcn-vue files live in `src/components/ui/context-menu/` and `src/components/ui/dropdown-menu/`. Both sets use class constants from one file, `src/components/ui/menu-classes.ts`. Pure helpers go in `src/lib/`. Two Blink components, `GroupMenuTree.vue` and `GroupMenuItems.vue`, render the parts of a menu that more than one surface uses. `App.vue` and `useWorkspaceState.ts` keep ownership of all state changes.

**Tech Stack:** Vue 3.5, TypeScript, Reka UI 2, shadcn-vue (registry `new-york-v4`), Tailwind v4, lucide-vue-next, Vitest + Vue Test Utils (jsdom), Bun.

**Spec:** `docs/superpowers/specs/2026-09-28-context-menu-overhaul-design.md`

## Global Constraints

- Colors come from theme tokens only (`--popover`, `--accent`, `--foreground`, `--muted-foreground`, `--destructive`, `--border`, `--frame`). Never hardcode a color. Menus never use `primary`.
- Menu text: system sans-serif, 12 px (`font-sans text-xs`). Row height 24 px (`h-6`), 32 px on `pointer-coarse` (`pointer-coarse:h-8`).
- Every item has the 32 px check gutter (`pl-8`). It is the default in `menu-classes.ts`, so call sites never need `inset`.
- Motion: a 100 ms opacity fade-in only. No scale, no slide, no exit animation. None with `prefers-reduced-motion: reduce`.
- Icons: `lucide-vue-next`. Never `@lucide/vue`. No leading icons in menu items.
- Every menu content and sub-content element has `data-surface="context-menu"`.
- Labels are in sentence case. A shortcut hint shows only for a shortcut that already exists.
- Call sites use `class` for layout only, not for color or type.
- Existing `data-testid` values stay on items that stay.
- A menu action uses the same mutation path as its button or keyboard command.
- In the Browser request menu, **Move to**, **Delete**, and **Authorization** act on the multi-selection when the target is in it. **Open**, **Duplicate**, **Close tab**, **Copy URL**, and **Copy as cURL** act on the target only.
- Commands: `bun run test`, `bun run lint`, `bun run build`. The dev server is already running on port 1420; do not start a second one.
- Commit after each task. End each commit message with `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.

## Review Focus

1. **Choosing the current authorization mode again.** When you select "Bearer" on a request that already has a bearer token, the token must stay. The current menu clears it. Test in Task 6.
2. **Multi-selection with one busy request.** "Delete 3 requests" must be disabled when any selected request is running. It must never delete some of them silently. Test in Task 6.
3. **Close to the right on the last tab, and Close others with one tab.** These must be disabled, not no-ops that move the focus. Test in Task 5.
4. **Moving a group into itself or into a descendant.** The group's own subtree must not show in its "Move to" tree. Test in Task 4 (the tree) and Task 7 (the call site).
5. **`Shift+F10` inside a text input.** It must not open a Blink menu over a field that keeps the native text menu, and it must not throw. Test in Task 1.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/components/ui/menu-classes.ts` (new) | Class strings used by both menu sets |
| `src/components/ui/context-menu/*` (replaced) | Official shadcn-vue context menu, adapted |
| `src/components/ui/dropdown-menu/*` (new) | Official shadcn-vue dropdown menu, adapted |
| `src/lib/menu-keyboard.ts` (new) | `Shift+F10` / Context Menu key detection and dispatch |
| `src/lib/shortcut.ts` (new) | Platform shortcut labels |
| `src/lib/session-curl.ts` (new) | cURL text for a session, or `""` |
| `src/components/GroupMenuTree.vue` (new) | Recursive "Move to" submenu |
| `src/components/GroupMenuItems.vue` (new) | Group menu items for both the context menu and the `⋯` dropdown |
| `src/composables/useWorkspaceState.ts` | New `closeTabs(ids)` |
| `src/App.vue` | Wiring: close many, reveal, copy, status bar copy message |
| `src/components/RequestTabs.vue` | Tab and strip menus, overflow dropdown |
| `src/components/RequestBrowser.vue` | Request, group, Ungrouped, and blank-space menus |
| `src/components/GroupActionsMenu.vue` | `⋯` dropdown that uses `GroupMenuItems` |
| `src/components/RequestWorkspace.vue` | Request bar and cURL preview menus |
| `src/components/KeyValueEditor.vue` | Table and row menus |
| `src/components/RequestEditor.vue` | Body and authorization menus |
| `src/components/ResponsePanel.vue` | Toolbar menu |
| `src/components/JsonTreeView.vue` | Row menu |
| `src/style.css` | Popover tokens, menu shadow, fade |
| `DESIGN.md` | Menu typography rule |

---

### Task 1: Official menu primitives

**Files:**
- Replace: `src/components/ui/context-menu/*` (all files)
- Create: `src/components/ui/dropdown-menu/*`, `src/components/ui/menu-classes.ts`, `src/lib/menu-keyboard.ts`
- Modify: `src/style.css`, `package.json`, `bun.lock`
- Test: `src/components/ui/context-menu/__test__/ContextMenuContent.test.ts`, `src/components/ui/__test__/ContextMenu.test.ts`, `src/components/ui/__test__/DropdownMenu.test.ts` (new), `src/lib/__test__/menu-keyboard.test.ts` (new)

**Interfaces:**
- Produces: barrel exports from `@/components/ui/context-menu`: `ContextMenu`, `ContextMenuTrigger`, `ContextMenuContent`, `ContextMenuItem` (prop `variant?: "default" | "destructive"`), `ContextMenuCheckboxItem` (`v-model:model-value` boolean), `ContextMenuRadioGroup` (`v-model:model-value` string), `ContextMenuRadioItem` (`value` string), `ContextMenuLabel`, `ContextMenuGroup`, `ContextMenuSeparator`, `ContextMenuShortcut`, `ContextMenuSub`, `ContextMenuSubTrigger`, `ContextMenuSubContent`, `ContextMenuPortal`. The same set with the `DropdownMenu` prefix from `@/components/ui/dropdown-menu`.
- Produces: `isContextMenuKey(event: KeyboardEvent): boolean`, `openContextMenuAt(target: Element): void`.

- [ ] **Step 1: Install the registry components**

The configured style (`vega`) has no menu items in the registry, so install from the `new-york-v4` URLs. `--overwrite` replaces the 6 hand-written files; the user approved this.

```bash
bunx --bun shadcn-vue@latest add https://shadcn-vue.com/r/styles/new-york-v4/context-menu.json https://shadcn-vue.com/r/styles/new-york-v4/dropdown-menu.json --overwrite --yes
```

Then:

```bash
bun remove @lucide/vue 2>/dev/null; bun add @vueuse/core
grep -rl "@lucide/vue" src/components/ui | xargs sed -i '' 's#"@lucide/vue"#"lucide-vue-next"#'
git status --short
```

Expected: 15 files in `context-menu/`, the matching files in `dropdown-menu/`, no `@lucide/vue` import. Keep the existing `__test__` folder.

- [ ] **Step 2: Add the shared classes**

Create `src/components/ui/menu-classes.ts`:

```ts
// Class strings shared by the context menu and the dropdown menu, so every
// menu in Blink looks the same. The layout follows VS Code: a 24px row, a
// check gutter on every item, the shortcut at the right.

export const menuContent =
  "z-50 min-w-45 overflow-x-hidden overflow-y-auto rounded-lg border border-border bg-popover p-1 font-sans text-xs text-popover-foreground shadow-menu";

export const menuItem =
  "relative flex h-6 cursor-default select-none items-center gap-2 rounded-sm pr-2 pl-8 outline-hidden data-highlighted:bg-accent data-highlighted:text-foreground data-disabled:pointer-events-none data-disabled:opacity-40 data-[variant=destructive]:text-destructive pointer-coarse:h-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3";

export const menuSubTrigger = `${menuItem} data-[state=open]:bg-accent`;

export const menuIndicator =
  "pointer-events-none absolute left-2.5 flex size-3 items-center justify-center";

export const menuSeparator = "-mx-1 my-1 h-px bg-border";

export const menuLabel =
  "pr-2 pl-8 py-1 text-[11px] text-muted-foreground select-none";

export const menuShortcut = "ml-auto pl-4 text-muted-foreground";
```

- [ ] **Step 3: Apply the classes in every menu file**

In each file of `context-menu/` and `dropdown-menu/`, replace the first argument of `cn(...)` with the constant and keep `props.class` as the second argument:

| File suffix | Constant | Extra |
| --- | --- | --- |
| `Content.vue` | `menuContent` | Add `max-h-(--reka-context-menu-content-available-height)` (context) or `max-h-(--reka-dropdown-menu-content-available-height)` (dropdown) after the constant. Add `data-surface="context-menu"`. |
| `SubContent.vue` | `menuContent` | Add `data-surface="context-menu"`. |
| `Item.vue` | `menuItem` | Keep `variant` and `data-variant`. Remove the `inset` prop and `data-inset`. |
| `CheckboxItem.vue`, `RadioItem.vue` | `menuItem` | The indicator `<span>` uses `menuIndicator`. The icon is `<Check class="size-3" />` (radio also uses `Check`, not `Circle`, like VS Code). |
| `SubTrigger.vue` | `menuSubTrigger` | Remove `inset`. The chevron is `<ChevronRight class="ml-auto size-3" />`. |
| `Separator.vue` | `menuSeparator` | |
| `Label.vue` | `menuLabel` | Remove `inset`. |
| `Shortcut.vue` | `menuShortcut` | |

Import with `import { menuItem } from "../menu-classes";`. After this step no file in either folder contains `animate-`, `zoom-`, `slide-in`, `accent-foreground`, `destructive-foreground`, `text-sm`, or `inset`:

```bash
grep -rnE "animate-|zoom-|slide-in|accent-foreground|destructive-foreground|text-sm|inset" src/components/ui/context-menu src/components/ui/dropdown-menu
```

Expected: no output.

- [ ] **Step 4: Add tokens, shadow, and fade to `src/style.css`**

In `:root`, after `--secondary`:

```css
  --popover: var(--secondary);
  --popover-foreground: var(--foreground);
```

In `:root[data-theme="ghostty"]`, after `--secondary`, the same two lines. In `@theme inline`, after `--color-secondary`:

```css
  --color-popover: var(--popover);
  --color-popover-foreground: var(--popover-foreground);
  --shadow-menu: 0 4px 12px color-mix(in oklch, var(--frame) 60%, transparent);
```

Replace the rule `[data-surface="context-menu"] { background-color: var(--secondary) !important; }` with:

```css
/* Menus fade in like VS Code: no scale, no slide, no exit animation. */
@media (prefers-reduced-motion: no-preference) {
  [data-surface="context-menu"][data-state="open"] {
    animation: menu-fade-in 100ms ease-out;
  }
}
@keyframes menu-fade-in {
  from {
    opacity: 0;
  }
}
```

- [ ] **Step 5: Write the failing keyboard helper tests**

Create `src/lib/__test__/menu-keyboard.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { isContextMenuKey, openContextMenuAt } from "../menu-keyboard";

describe("isContextMenuKey", () => {
  it("accepts Shift+F10 and the Context Menu key", () => {
    expect(isContextMenuKey(new KeyboardEvent("keydown", { key: "F10", shiftKey: true }))).toBe(true);
    expect(isContextMenuKey(new KeyboardEvent("keydown", { key: "ContextMenu" }))).toBe(true);
  });
  it("rejects F10 alone and F10 with other modifiers", () => {
    expect(isContextMenuKey(new KeyboardEvent("keydown", { key: "F10" }))).toBe(false);
    expect(isContextMenuKey(new KeyboardEvent("keydown", { key: "F10", shiftKey: true, metaKey: true }))).toBe(false);
  });
});

describe("openContextMenuAt", () => {
  it("dispatches a bubbling contextmenu event at the element center", () => {
    const el = document.createElement("button");
    el.getBoundingClientRect = () => ({ left: 10, top: 20, width: 100, height: 40 }) as DOMRect;
    const listener = vi.fn();
    document.body.append(el);
    document.body.addEventListener("contextmenu", listener);
    openContextMenuAt(el);
    const event = listener.mock.calls[0][0] as MouseEvent;
    expect(event.clientX).toBe(60);
    expect(event.clientY).toBe(40);
    document.body.removeEventListener("contextmenu", listener);
    el.remove();
  });
});
```

Run: `bun run test src/lib/__test__/menu-keyboard.test.ts`
Expected: FAIL, module not found.

- [ ] **Step 6: Implement `src/lib/menu-keyboard.ts`**

```ts
/** True for the keys that open a context menu: Shift+F10 and the Context Menu key. */
export function isContextMenuKey(event: KeyboardEvent) {
  if (event.key === "ContextMenu") return true;
  return (
    event.key === "F10" &&
    event.shiftKey &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.altKey
  );
}

/**
 * Open the context menu for `target` at its center. macOS WebKit does not
 * send a contextmenu event for Shift+F10, so Blink sends one.
 */
export function openContextMenuAt(target: Element) {
  const rect = target.getBoundingClientRect();
  target.dispatchEvent(
    new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      clientX: rect.left + rect.width / 2,
      clientY: rect.top + rect.height / 2,
    }),
  );
}
```

Run: `bun run test src/lib/__test__/menu-keyboard.test.ts`
Expected: PASS.

- [ ] **Step 7: Handle the keys in `ContextMenuTrigger.vue`**

Add to the registry trigger's script:

```ts
import { isContextMenuKey, openContextMenuAt } from "@/lib/menu-keyboard";

function onKeydown(event: KeyboardEvent) {
  if (!isContextMenuKey(event) || !(event.target instanceof Element)) return;
  // A text field keeps the native text menu; right-click does nothing there
  // either, because those fields stop the contextmenu event.
  if (event.target.closest("input, textarea, [contenteditable='true'], .cm-editor")) return;
  event.preventDefault();
  // A nested trigger handles the key first; the outer one must not open too.
  event.stopPropagation();
  openContextMenuAt(event.target);
}
```

Bind it on the root element: `<ContextMenuTrigger data-slot="context-menu-trigger" v-bind="forwardedProps" @keydown="onKeydown">`.

- [ ] **Step 8: Write the primitive tests**

Create `src/components/__test__/menu-test-utils.ts`. Later tasks use it to read item labels without the shortcut text or the check indicator:

```ts
/** The label of each open menu item, without its shortcut hint. */
export function menuLabels(root: ParentNode = document.body) {
  return [...root.querySelectorAll('[role^="menuitem"]')].map((el) => {
    const copy = el.cloneNode(true) as HTMLElement;
    copy.querySelectorAll('[data-slot$="shortcut"]').forEach((hint) => hint.remove());
    return copy.textContent?.trim() ?? "";
  });
}
```

Keep the existing tests in `src/components/ui/__test__/ContextMenu.test.ts` and `context-menu/__test__/ContextMenuContent.test.ts`. Append to `ContextMenu.test.ts`:

```ts
import {
  ContextMenuCheckboxItem,
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "../context-menu";

function mountTemplate(template: string, setup = () => ({})) {
  const wrapper = mount(
    defineComponent({
      components: {
        ContextMenu, ContextMenuTrigger, ContextMenuContent, ContextMenuItem,
        ContextMenuCheckboxItem, ContextMenuRadioGroup, ContextMenuRadioItem,
        ContextMenuShortcut, ContextMenuSub, ContextMenuSubTrigger, ContextMenuSubContent,
      },
      setup,
      template,
    }),
    { attachTo: document.body },
  );
  wrappers.push(wrapper);
  return wrapper;
}
const open = (wrapper: ReturnType<typeof mount>) =>
  wrapper.get("[data-trigger]").trigger("contextmenu");

describe("ContextMenu parts", () => {
  it("renders a shortcut at the end of an item", async () => {
    const w = mountTemplate(`<ContextMenu><ContextMenuTrigger as-child><button data-trigger>t</button></ContextMenuTrigger>
      <ContextMenuContent><ContextMenuItem>Close<ContextMenuShortcut>⌘W</ContextMenuShortcut></ContextMenuItem></ContextMenuContent></ContextMenu>`);
    await open(w);
    expect(document.body.querySelector('[data-slot="context-menu-shortcut"]')?.textContent).toBe("⌘W");
  });

  it("marks a destructive item", async () => {
    const w = mountTemplate(`<ContextMenu><ContextMenuTrigger as-child><button data-trigger>t</button></ContextMenuTrigger>
      <ContextMenuContent><ContextMenuItem variant="destructive">Delete</ContextMenuItem></ContextMenuContent></ContextMenu>`);
    await open(w);
    expect(document.body.querySelector('[role="menuitem"]')?.getAttribute("data-variant")).toBe("destructive");
  });

  it("shows the checked state of checkbox and radio items", async () => {
    const w = mountTemplate(
      `<ContextMenu><ContextMenuTrigger as-child><button data-trigger>t</button></ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuCheckboxItem :model-value="true">Wrap lines</ContextMenuCheckboxItem>
          <ContextMenuRadioGroup model-value="json">
            <ContextMenuRadioItem value="none">None</ContextMenuRadioItem>
            <ContextMenuRadioItem value="json">JSON</ContextMenuRadioItem>
          </ContextMenuRadioGroup>
        </ContextMenuContent></ContextMenu>`,
    );
    await open(w);
    expect(document.body.querySelector('[role="menuitemcheckbox"]')?.getAttribute("aria-checked")).toBe("true");
    const radios = [...document.body.querySelectorAll('[role="menuitemradio"]')];
    expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual(["false", "true"]);
  });

  it("gives a sub-trigger a chevron and a sub-content the menu surface", async () => {
    const w = mountTemplate(`<ContextMenu><ContextMenuTrigger as-child><button data-trigger>t</button></ContextMenuTrigger>
      <ContextMenuContent><ContextMenuSub><ContextMenuSubTrigger data-sub>Move to</ContextMenuSubTrigger>
      <ContextMenuSubContent><ContextMenuItem>Inside</ContextMenuItem></ContextMenuSubContent></ContextMenuSub></ContextMenuContent></ContextMenu>`);
    await open(w);
    const sub = document.body.querySelector("[data-sub]") as HTMLElement;
    expect(sub.querySelector("svg")).not.toBeNull();
    sub.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.querySelectorAll('[data-surface="context-menu"]').length).toBe(2);
  });

  it("opens with Shift+F10 on the focused element", async () => {
    const w = mountTemplate(`<ContextMenu><ContextMenuTrigger as-child><button data-trigger>t</button></ContextMenuTrigger>
      <ContextMenuContent><ContextMenuItem data-testid="kb">Item</ContextMenuItem></ContextMenuContent></ContextMenu>`);
    await w.get("[data-trigger]").trigger("keydown", { key: "F10", shiftKey: true });
    expect(document.body.querySelector('[data-testid="kb"]')).not.toBeNull();
  });

  it("does not open with Shift+F10 inside a text input", async () => {
    const w = mountTemplate(`<ContextMenu><ContextMenuTrigger as-child><div data-trigger><input data-field /></div></ContextMenuTrigger>
      <ContextMenuContent><ContextMenuItem data-testid="kb">Item</ContextMenuItem></ContextMenuContent></ContextMenu>`);
    await w.get("[data-field]").trigger("keydown", { key: "F10", shiftKey: true });
    expect(document.body.querySelector('[data-testid="kb"]')).toBeNull();
  });
});
```

Create `src/components/ui/__test__/DropdownMenu.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { defineComponent } from "vue";
import { DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem } from "../dropdown-menu";

afterEach(() => { document.body.innerHTML = ""; });

describe("DropdownMenu", () => {
  it("uses the shared menu surface", async () => {
    const w = mount(defineComponent({
      components: { DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem },
      template: `<DropdownMenu><DropdownMenuTrigger data-trigger>Open</DropdownMenuTrigger>
        <DropdownMenuContent><DropdownMenuItem>One</DropdownMenuItem></DropdownMenuContent></DropdownMenu>`,
    }), { attachTo: document.body });
    await w.get("[data-trigger]").trigger("keydown", { key: "Enter" });
    const surface = document.body.querySelector('[data-surface="context-menu"]');
    expect(surface?.className).toContain("bg-popover");
    expect(surface?.className).toContain("font-sans");
    w.unmount();
  });
});
```

Run: `bun run test src/components/ui`
Expected: PASS. If a test in the existing files checks an old class name (for example `bg-secondary` or `font-mono`), update it to the new constant.

- [ ] **Step 9: Run the whole suite and the build**

Run: `bun run test && bun run build && bun run lint`
Expected: PASS. The call sites still use `ContextMenuItem`, `@select`, `:disabled`, and `as-child`, and the official components support all of these. If a call site imported `ContextMenuSub`/`ContextMenuSubTrigger` from `"@/components/ui/context-menu/index"`, the barrel still exports them.

- [ ] **Step 10: Commit**

```bash
git add -A src/components/ui src/components/__test__/menu-test-utils.ts src/lib/menu-keyboard.ts src/lib/__test__/menu-keyboard.test.ts src/style.css package.json bun.lock
git commit -m "feat: official shadcn-vue menus with VS Code styling"
```

---

### Task 2: Shortcut labels, session cURL, and dropdown migration

**Files:**
- Create: `src/lib/shortcut.ts`, `src/lib/session-curl.ts`
- Modify: `src/components/GroupActionsMenu.vue` (temporary migration; Task 8 replaces the items), `src/components/RequestTabs.vue` (overflow dropdown)
- Test: `src/lib/__test__/shortcut.test.ts`, `src/lib/__test__/session-curl.test.ts`, existing `RequestTabsOverflow.test.ts` and `RequestBrowser.test.ts`

**Interfaces:**
- Produces: `shortcutLabel(keys: string[], mac?: boolean): string`. Keys: `"mod"`, `"shift"`, `"alt"`, `"enter"`, `"esc"`, or one character.
- Produces: `sessionCurl(session: RequestSession, groups: RequestGroup[], globals: Record<string, string>): string` (returns `""` when the draft does not build).

- [ ] **Step 1: Write the failing tests**

`src/lib/__test__/shortcut.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { shortcutLabel } from "../shortcut";

describe("shortcutLabel", () => {
  it("uses symbols in VS Code order on macOS", () => {
    expect(shortcutLabel(["mod", "shift", "d"], true)).toBe("⇧⌘D");
    expect(shortcutLabel(["mod", "enter"], true)).toBe("⌘↵");
    expect(shortcutLabel(["esc"], true)).toBe("Esc");
  });
  it("uses words joined by + on other platforms", () => {
    expect(shortcutLabel(["mod", "shift", "d"], false)).toBe("Ctrl+Shift+D");
    expect(shortcutLabel(["mod", "enter"], false)).toBe("Ctrl+Enter");
    expect(shortcutLabel(["mod", "w"], false)).toBe("Ctrl+W");
  });
});
```

`src/lib/__test__/session-curl.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { createSession } from "../session";
import { sessionCurl } from "../session-curl";

describe("sessionCurl", () => {
  it("builds a cURL command with inherited authorization", () => {
    const session = createSession();
    session.draft.url = "https://api.example.test/users";
    session.groupId = 1;
    const groups = [{ id: 1, name: "API", parentId: null, collapsed: false, localAuth: { type: "bearer" as const, token: "abc" } }];
    const command = sessionCurl(session, groups, {});
    expect(command.startsWith("curl")).toBe(true);
    expect(command).toContain("https://api.example.test/users");
    expect(command).toContain("Bearer abc");
  });
  it("returns an empty string when the draft does not build", () => {
    const session = createSession();
    session.draft.url = "not a url";
    expect(sessionCurl(session, [], {})).toBe("");
  });
});
```

Run: `bun run test src/lib/__test__/shortcut.test.ts src/lib/__test__/session-curl.test.ts`
Expected: FAIL, modules not found. If the `RequestGroup` literal fails type checks, copy the required fields from `src/lib/groups.ts`.

- [ ] **Step 2: Implement**

`src/lib/shortcut.ts`:

```ts
const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/i.test(navigator.platform);

const order = ["alt", "shift", "mod"];
const macSymbols: Record<string, string> = { alt: "⌥", shift: "⇧", mod: "⌘", enter: "↵", esc: "Esc" };
const words: Record<string, string> = { alt: "Alt", shift: "Shift", mod: "Ctrl", enter: "Enter", esc: "Esc" };

/**
 * A shortcut hint for menus, such as "⇧⌘D" on macOS and "Ctrl+Shift+D"
 * elsewhere. Modifiers use a fixed order whatever order the caller gives.
 */
export function shortcutLabel(keys: string[], mac = isMac) {
  const modifiers = keys.filter((key) => order.includes(key));
  const rest = keys.filter((key) => !order.includes(key));
  if (mac) {
    modifiers.sort((a, b) => order.indexOf(a) - order.indexOf(b));
    return [...modifiers, ...rest].map((key) => macSymbols[key] ?? key.toUpperCase()).join("");
  }
  const wordOrder = ["mod", "alt", "shift"];
  modifiers.sort((a, b) => wordOrder.indexOf(a) - wordOrder.indexOf(b));
  return [...modifiers, ...rest].map((key) => words[key] ?? key.toUpperCase()).join("+");
}
```

`src/lib/session-curl.ts`:

```ts
import { buildResolvedRequestContext } from "./authorization";
import type { RequestGroup } from "./groups";
import { buildRequest, toCurl } from "./request";
import type { RequestSession } from "./session";

/** The cURL command for a session, resolved like a send. Empty when the draft does not build. */
export function sessionCurl(
  session: RequestSession,
  groups: RequestGroup[],
  globals: Record<string, string>,
) {
  try {
    const ctx = buildResolvedRequestContext(session.draft, session.groupId ?? null, groups, globals);
    return toCurl(buildRequest(session.draft, ctx));
  } catch {
    return "";
  }
}
```

Run the two tests. Expected: PASS.

- [ ] **Step 3: Migrate the two dropdowns**

In `RequestTabs.vue`, replace the raw Reka `DropdownMenuRoot`/`DropdownMenuTrigger`/`DropdownMenuPortal`/`DropdownMenuContent`/`DropdownMenuItem` imports with `DropdownMenu`, `DropdownMenuTrigger`, `DropdownMenuContent`, `DropdownMenuItem` from `@/components/ui/dropdown-menu`. The overflow menu becomes:

```vue
<DropdownMenu v-if="overflow.hidden > 0">
  <DropdownMenuTrigger
    class="flex items-center justify-center gap-0.5 shrink-0 px-2 font-mono text-[0.625rem] text-muted-foreground border-x border-border cursor-pointer hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground pointer-coarse:min-w-11"
    data-tab-overflow
    :aria-label="`${overflow.hidden} more ${overflow.hidden === 1 ? 'tab' : 'tabs'}`"
    title="Show all open tabs"
  >
    <ChevronDown :size="13" aria-hidden="true" />
    {{ overflow.hidden }}
  </DropdownMenuTrigger>
  <DropdownMenuContent align="end" :side-offset="4" class="max-h-[60dvh] min-w-56 max-w-80">
    <DropdownMenuItem
      v-for="session in sessions"
      :key="session.id"
      data-tab-overflow-item
      :class="session.id === activeId ? '' : 'text-muted-foreground'"
      @select="emit('select', session.id)"
    >
      <span class="method w-11 shrink-0 font-mono text-[0.5625rem] font-bold tracking-[0.04em]" :data-method="session.draft.method">{{ session.draft.method }}</span>
      <span class="truncate">{{ sessionLabel(session) }}</span>
    </DropdownMenuItem>
  </DropdownMenuContent>
</DropdownMenu>
```

The muted text for inactive tabs is a state, not a restyle, so it stays at the call site.

In `GroupActionsMenu.vue`, make the same import change, use `DropdownMenuContent align="end" :side-offset="4"` with no class, give the delete item `variant="destructive"`, and remove the `.menu-item` class and the scoped `<style>` block. Keep the aria-labels.

Run: `bun run test src/components/__test__/RequestTabsOverflow.test.ts src/components/__test__/RequestBrowser.test.ts`
Expected: PASS. The tests stub `DropdownMenuPortal`; the official content renders Reka's `DropdownMenuPortal`, so the stub still applies.

- [ ] **Step 4: Commit**

```bash
git add src/lib/shortcut.ts src/lib/session-curl.ts src/lib/__test__/shortcut.test.ts src/lib/__test__/session-curl.test.ts src/components/RequestTabs.vue src/components/GroupActionsMenu.vue
git commit -m "feat: shortcut labels, session cURL, and shared dropdown menus"
```

---

### Task 3: `closeTabs` in workspace state

**Files:**
- Modify: `src/composables/useWorkspaceState.ts` (after `closeTab`, and the returned object)
- Test: `src/composables/__test__/useWorkspaceStateSetters.test.ts`

**Interfaces:**
- Produces: `closeTabs(ids: number[]): void`, returned from `useWorkspaceState()`.

- [ ] **Step 1: Write the failing tests**

Append to `useWorkspaceStateSetters.test.ts`:

```ts
describe("closeTabs", () => {
  function openFour() {
    const ids = [0, 1, 2, 3].map(() => {
      const session = createSession();
      state.sessions.value.push(session);
      state.openRequest(session.id);
      return session.id;
    });
    state.openIds.value = ids;
    return ids;
  }

  it("keeps the active tab when it stays open", () => {
    const [a, b, c, d] = openFour();
    state.activeId.value = b;
    state.closeTabs([a, c]);
    expect(state.openIds.value).toEqual([b, d]);
    expect(state.activeId.value).toBe(b);
  });

  it("moves the active tab to the next open tab at the same position", () => {
    const [a, b, c, d] = openFour();
    state.activeId.value = b;
    state.closeTabs([b, c]);
    expect(state.openIds.value).toEqual([a, d]);
    expect(state.activeId.value).toBe(d);
  });

  it("clears the active tab when all tabs close, and keeps the requests", () => {
    const ids = openFour();
    const count = state.sessions.value.length;
    state.closeTabs(ids);
    expect(state.openIds.value).toEqual([]);
    expect(state.activeId.value).toBeNull();
    expect(state.sessions.value.length).toBe(count);
  });
});
```

Add `import { createSession } from "@/lib/session";` at the top if it is not there.

Run: `bun run test src/composables/__test__/useWorkspaceStateSetters.test.ts`
Expected: FAIL, `state.closeTabs is not a function`.

- [ ] **Step 2: Implement**

After `closeTab` in `useWorkspaceState.ts`:

```ts
  /**
   * Close several tabs. The active tab stays active when it stays open;
   * otherwise the next open tab at its position becomes active.
   */
  function closeTabs(ids: number[]) {
    const closing = new Set(ids);
    const before = openIds.value;
    const remaining = before.filter((id) => !closing.has(id));
    if (remaining.length === before.length) return;
    openIds.value = remaining;
    if (activeId.value === null || !closing.has(activeId.value)) return;
    const position = before
      .slice(0, before.indexOf(activeId.value))
      .filter((id) => !closing.has(id)).length;
    activeId.value = remaining[Math.min(position, remaining.length - 1)] ?? null;
  }
```

Add `closeTabs` to the returned object next to `closeTab`.

Run the test. Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/composables/useWorkspaceState.ts src/composables/__test__/useWorkspaceStateSetters.test.ts
git commit -m "feat: close several tabs at once"
```

---

### Task 4: `GroupMenuTree`

**Files:**
- Create: `src/components/GroupMenuTree.vue`
- Test: `src/components/__test__/GroupMenuTree.test.ts`

**Interfaces:**
- Consumes: the ui parts from Task 1.
- Produces: `<GroupMenuTree :groups :root-label :current-id? :exclude-id? :kind? @pick="(groupId: number | null) => …" />`. `currentId` `undefined` means no current mark; `null` means the root is current. `excludeId` hides that group and its subtree. `kind` is `"context"` (default) or `"dropdown"`.

- [ ] **Step 1: Write the failing tests**

```ts
import { afterEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import GroupMenuTree from "../GroupMenuTree.vue";
import type { RequestGroup } from "@/lib/groups";

const groups: RequestGroup[] = [
  { id: 1, name: "Platform", parentId: null, collapsed: false },
  { id: 2, name: "Identity", parentId: 1, collapsed: false },
  { id: 3, name: "Billing", parentId: null, collapsed: false },
];

afterEach(() => { document.body.innerHTML = ""; });

async function openTree(props: Record<string, unknown>) {
  const picks: (number | null)[] = [];
  const wrapper = mount(defineComponent({
    setup: () => () =>
      h(ContextMenu, null, { default: () => [
        h(ContextMenuTrigger, { asChild: true }, { default: () => h("button", { "data-trigger": "" }, "t") }),
        h(ContextMenuContent, null, { default: () =>
          h(GroupMenuTree, { groups, rootLabel: "Ungrouped", ...props, onPick: (id: number | null) => picks.push(id) }) }),
      ] }),
  }), { attachTo: document.body });
  await wrapper.get("[data-trigger]").trigger("contextmenu");
  return { wrapper, picks };
}
const items = () => [...document.body.querySelectorAll('[role^="menuitem"]')].map((el) => el.textContent?.trim());

describe("GroupMenuTree", () => {
  it("lists the root, then top-level groups; a group with children is a submenu", async () => {
    await openTree({});
    expect(items()).toEqual(["Ungrouped", "Platform", "Billing"]);
    expect(document.body.querySelector('[data-move-target="1"]')?.getAttribute("aria-haspopup")).toBe("menu");
    expect(document.body.querySelector('[data-move-target="3"]')?.getAttribute("aria-haspopup")).toBeNull();
  });

  it("opens a submenu with 'Move into' first, then the children", async () => {
    const { picks } = await openTree({});
    (document.body.querySelector('[data-move-target="1"]') as HTMLElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(items()).toEqual(["Ungrouped", "Platform", "Billing", "Move into Platform", "Identity"]);
    (document.body.querySelector('[data-move-into="1"]') as HTMLElement).click();
    expect(picks).toEqual([1]);
  });

  it("marks the current location as checked and disabled", async () => {
    await openTree({ currentId: 3 });
    const current = document.body.querySelector('[data-move-target="3"]')!;
    expect(current.getAttribute("aria-checked")).toBe("true");
    expect(current.hasAttribute("data-disabled")).toBe(true);
  });

  it("marks the root as current when currentId is null", async () => {
    await openTree({ currentId: null });
    expect(document.body.querySelector('[data-move-target="root"]')?.getAttribute("aria-checked")).toBe("true");
  });

  it("hides the excluded group and its subtree", async () => {
    await openTree({ excludeId: 1, rootLabel: "Top level" });
    expect(items()).toEqual(["Top level", "Billing"]);
  });

  it("emits pick with null for the root", async () => {
    const { picks } = await openTree({});
    (document.body.querySelector('[data-move-target="root"]') as HTMLElement).click();
    expect(picks).toEqual([null]);
  });
});
```

Run: `bun run test src/components/__test__/GroupMenuTree.test.ts`
Expected: FAIL, component not found.

- [ ] **Step 2: Implement `src/components/GroupMenuTree.vue`**

```vue
<script setup lang="ts">
import { computed } from "vue";
import type { RequestGroup } from "@/lib/groups";
import {
  ContextMenuCheckboxItem,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenuCheckboxItem,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";

defineOptions({ name: "GroupMenuTree" });
const props = withDefaults(
  defineProps<{
    groups: RequestGroup[];
    rootLabel: string;
    /** The current location. Undefined: none. Null: the root. */
    currentId?: number | null;
    /** A group to hide with its subtree, so a group cannot move into itself. */
    excludeId?: number;
    kind?: "context" | "dropdown";
    /** Internal: the parent whose children this level lists. */
    parentId?: number | null;
  }>(),
  { currentId: undefined, excludeId: undefined, kind: "context", parentId: undefined },
);
const emit = defineEmits<{ pick: [groupId: number | null] }>();

const ui = computed(() =>
  props.kind === "dropdown"
    ? { Item: DropdownMenuItem, Check: DropdownMenuCheckboxItem, Separator: DropdownMenuSeparator, Sub: DropdownMenuSub, SubTrigger: DropdownMenuSubTrigger, SubContent: DropdownMenuSubContent }
    : { Item: ContextMenuItem, Check: ContextMenuCheckboxItem, Separator: ContextMenuSeparator, Sub: ContextMenuSub, SubTrigger: ContextMenuSubTrigger, SubContent: ContextMenuSubContent },
);
const nested = computed(() => props.parentId !== undefined);
function children(parentId: number | null) {
  return props.groups.filter((group) => group.parentId === parentId && group.id !== props.excludeId);
}
/** The current location is a checked, disabled item; others are plain items. */
function target(id: number | null) {
  return props.currentId === id
    ? { is: ui.value.Check, modelValue: true, disabled: true }
    : { is: ui.value.Item };
}
</script>

<template>
  <template v-if="!nested">
    <component :is="target(null).is" v-bind="target(null)" data-move-target="root" @select="emit('pick', null)">
      {{ rootLabel }}
    </component>
    <component :is="ui.Separator" v-if="children(null).length" />
  </template>
  <template v-for="group in children(parentId ?? null)" :key="group.id">
    <component :is="ui.Sub" v-if="children(group.id).length">
      <component :is="ui.SubTrigger" :data-move-target="group.id">{{ group.name }}</component>
      <component :is="ui.SubContent">
        <component :is="target(group.id).is" v-bind="target(group.id)" :data-move-into="group.id" @select="emit('pick', group.id)">
          Move into {{ group.name }}
        </component>
        <component :is="ui.Separator" />
        <GroupMenuTree
          :groups="groups" :root-label="rootLabel" :current-id="currentId" :exclude-id="excludeId"
          :kind="kind" :parent-id="group.id" @pick="emit('pick', $event)"
        />
      </component>
    </component>
    <component :is="target(group.id).is" v-else v-bind="target(group.id)" :data-move-target="group.id" @select="emit('pick', group.id)">
      {{ group.name }}
    </component>
  </template>
</template>
```

`v-bind="target(x)"` also passes `is`; Vue ignores it as a prop on the resolved component. If vue-tsc rejects it, split the object: `const { is, ...rest } = target(x)` in a helper that returns `rest` only.

In the "current location" test for a group with children, the submenu trigger has no check; the "Move into" item carries it.

Run the test. Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/components/GroupMenuTree.vue src/components/__test__/GroupMenuTree.test.ts
git commit -m "feat: nested group tree for Move to menus"
```

---

### Task 5: Request tab and tab strip menus

**Files:**
- Modify: `src/components/RequestTabs.vue`, `src/App.vue`
- Test: `src/components/__test__/RequestTabsContextMenu.test.ts`

**Interfaces:**
- Consumes: `closeTabs` (Task 3), `shortcutLabel`, `sessionCurl` (Task 2).
- Produces: `RequestTabs` prop `curlFor?: (id: number) => string`; emits `closeMany: [ids: number[]]`, `duplicate: [id?: number]` (was `[]`), `copy: [text: string]`, `reveal: [id: number]`.
- Produces in `App.vue`: `closeMany(ids)`, `reveal(id)`, `expandAncestors(groupId)`, `copyText(text)`.

- [ ] **Step 1: Write the failing tests**

Add `import { menuLabels } from "./menu-test-utils";`. Replace the tab-menu tests in `RequestTabsContextMenu.test.ts` (keep the file's `render` helper; add `curlFor` to its props: `curlFor: (id: number) => (id === sessions[0].id ? "curl https://a" : "")`):

```ts
async function openTabMenu(wrapper: ReturnType<typeof mount>, id: number) {
  await wrapper.get(`[data-tab-id="${id}"]`).trigger("contextmenu");
}
const item = (testid: string) => document.body.querySelector(`[data-testid="${testid}"]`) as HTMLElement;

describe("RequestTabs tab context menu", () => {
  it("lists the VS Code tab actions in order", async () => {
    const { wrapper, sessions } = render(3);
    await openTabMenu(wrapper, sessions[1].id);
    expect(menuLabels()).toEqual(["Close", "Close others", "Close to the right", "Close all", "Duplicate", "Copy URL", "Copy as cURL", "Reveal in Browser"]);
  });

  it("Close others emits closeMany with every other tab", async () => {
    const { wrapper, sessions } = render(3);
    await openTabMenu(wrapper, sessions[1].id);
    item(`tab-ctx-close-others-${sessions[1].id}`).click();
    expect(wrapper.emitted("closeMany")![0]).toEqual([[sessions[0].id, sessions[2].id]]);
  });

  it("Close to the right emits the tabs after the target", async () => {
    const { wrapper, sessions } = render(3);
    await openTabMenu(wrapper, sessions[0].id);
    item(`tab-ctx-close-right-${sessions[0].id}`).click();
    expect(wrapper.emitted("closeMany")![0]).toEqual([[sessions[1].id, sessions[2].id]]);
  });

  it("disables Close to the right on the last tab and Close others with one tab", async () => {
    const { wrapper, sessions } = render(1);
    await openTabMenu(wrapper, sessions[0].id);
    expect(item(`tab-ctx-close-right-${sessions[0].id}`).hasAttribute("data-disabled")).toBe(true);
    expect(item(`tab-ctx-close-others-${sessions[0].id}`).hasAttribute("data-disabled")).toBe(true);
  });

  it("Duplicate duplicates the target tab, not the active tab", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[1].id);
    item(`tab-ctx-duplicate-${sessions[1].id}`).click();
    expect(wrapper.emitted("duplicate")![0]).toEqual([sessions[1].id]);
    expect(wrapper.emitted("select")).toBeFalsy();
  });

  it("Copy URL emits copy with the draft URL", async () => {
    const { wrapper, sessions } = render(2);
    sessions[0].draft.url = "https://a";
    await openTabMenu(wrapper, sessions[0].id);
    item(`tab-ctx-copy-url-${sessions[0].id}`).click();
    expect(wrapper.emitted("copy")![0]).toEqual(["https://a"]);
  });

  it("disables Copy as cURL when the draft does not build", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[1].id);
    expect(item(`tab-ctx-copy-curl-${sessions[1].id}`).hasAttribute("data-disabled")).toBe(true);
  });

  it("Reveal in Browser emits reveal", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[0].id);
    item(`tab-ctx-reveal-${sessions[0].id}`).click();
    expect(wrapper.emitted("reveal")![0]).toEqual([sessions[0].id]);
  });

  it("shows the existing shortcuts", async () => {
    const { wrapper, sessions } = render(2);
    await openTabMenu(wrapper, sessions[0].id);
    const shortcuts = [...document.body.querySelectorAll('[data-slot="context-menu-shortcut"]')].map((el) => el.textContent);
    expect(shortcuts.length).toBe(2); // Close, Duplicate
  });
});

describe("RequestTabs tab-strip context menu", () => {
  it("New request emits create", async () => {
    const { wrapper } = render(2);
    await wrapper.get('[data-testid="tab-strip-ctx-trigger"]').trigger("contextmenu");
    item("tab-strip-ctx-new").click();
    expect(wrapper.emitted("create")).toBeTruthy();
  });

  it("Close all emits closeMany with every tab", async () => {
    const { wrapper, sessions } = render(2);
    await wrapper.get('[data-testid="tab-strip-ctx-trigger"]').trigger("contextmenu");
    item("tab-strip-ctx-close-all").click();
    expect(wrapper.emitted("closeMany")![0]).toEqual([sessions.map((s) => s.id)]);
  });
});
```

Delete the old "'Duplicate active' emits duplicate" test and the "Select" tests; those items are removed. If the tab cell's `data-tab-id` attribute is on a different element than the `ContextMenuTrigger` child, use the trigger's existing `data-testid` in `openTabMenu` instead.

Run: `bun run test src/components/__test__/RequestTabsContextMenu.test.ts`
Expected: FAIL.

- [ ] **Step 2: Implement the menus in `RequestTabs.vue`**

Script additions:

```ts
import { ContextMenuShortcut } from "@/components/ui/context-menu";
import { shortcutLabel } from "@/lib/shortcut";

const props = defineProps<{
  sessions: RequestSession[];
  activeId: number | null;
  /** cURL text for a request; empty when its draft does not build. */
  curlFor?: (id: number) => string;
}>();
const emit = defineEmits<{
  select: [id: number];
  close: [id: number];
  closeMany: [ids: number[]];
  create: [];
  duplicate: [id?: number];
  copy: [text: string];
  reveal: [id: number];
  openRequests: [ids: number[], beforeId: number | null];
}>();
const allIds = computed(() => props.sessions.map((session) => session.id));
function othersOf(id: number) {
  return allIds.value.filter((other) => other !== id);
}
function rightOf(id: number) {
  return allIds.value.slice(allIds.value.indexOf(id) + 1);
}
```

The toolbar duplicate button keeps `@click="emit('duplicate')"`.

Tab menu content (replaces the current `<ContextMenuContent>` for a tab):

```vue
<ContextMenuContent>
  <ContextMenuItem :data-testid="`tab-ctx-close-${session.id}`" @select="emit('close', session.id)">
    Close<ContextMenuShortcut>{{ shortcutLabel(["mod", "w"]) }}</ContextMenuShortcut>
  </ContextMenuItem>
  <ContextMenuItem :data-testid="`tab-ctx-close-others-${session.id}`" :disabled="!othersOf(session.id).length" @select="emit('closeMany', othersOf(session.id))">
    Close others
  </ContextMenuItem>
  <ContextMenuItem :data-testid="`tab-ctx-close-right-${session.id}`" :disabled="!rightOf(session.id).length" @select="emit('closeMany', rightOf(session.id))">
    Close to the right
  </ContextMenuItem>
  <ContextMenuItem :data-testid="`tab-ctx-close-all-${session.id}`" @select="emit('closeMany', allIds)">
    Close all
  </ContextMenuItem>
  <ContextMenuSeparator />
  <ContextMenuItem :data-testid="`tab-ctx-duplicate-${session.id}`" @select="emit('duplicate', session.id)">
    Duplicate<ContextMenuShortcut>{{ shortcutLabel(["mod", "shift", "d"]) }}</ContextMenuShortcut>
  </ContextMenuItem>
  <ContextMenuSeparator />
  <ContextMenuItem :data-testid="`tab-ctx-copy-url-${session.id}`" :disabled="!session.draft.url" @select="emit('copy', session.draft.url)">
    Copy URL
  </ContextMenuItem>
  <ContextMenuItem :data-testid="`tab-ctx-copy-curl-${session.id}`" :disabled="!curlFor?.(session.id)" @select="emit('copy', curlFor?.(session.id) ?? '')">
    Copy as cURL
  </ContextMenuItem>
  <ContextMenuSeparator />
  <ContextMenuItem :data-testid="`tab-ctx-reveal-${session.id}`" @select="emit('reveal', session.id)">
    Reveal in Browser
  </ContextMenuItem>
</ContextMenuContent>
```

Strip menu content:

```vue
<ContextMenuContent>
  <ContextMenuItem data-testid="tab-strip-ctx-new" @select="emit('create')">
    New request<ContextMenuShortcut>{{ shortcutLabel(["mod", "t"]) }}</ContextMenuShortcut>
  </ContextMenuItem>
  <ContextMenuItem data-testid="tab-strip-ctx-close-all" :disabled="!allIds.length" @select="emit('closeMany', allIds)">
    Close all
  </ContextMenuItem>
</ContextMenuContent>
```

Run the test. Expected: PASS.

- [ ] **Step 3: Wire `App.vue`**

Add `closeTabs` to the destructured `useWorkspaceState()` result. Add:

```ts
import { useClipboard } from "@/composables/useClipboard";
import { sessionCurl } from "@/lib/session-curl";

const { copied, copyError, copy: copyText } = useClipboard();
function curlFor(id: number) {
  const session = sessions.value.find((candidate) => candidate.id === id);
  return session ? sessionCurl(session, groups.value, globalDefinitions.value) : "";
}
/** Expand every group from `groupId` up to the root. */
function expandAncestors(groupId: number | null) {
  const byId = new Map(groups.value.map((group) => [group.id, group]));
  let cursor = groupId;
  while (cursor !== null) {
    const group = byId.get(cursor);
    if (!group) break;
    group.collapsed = false;
    cursor = group.parentId;
  }
}
async function closeMany(ids: number[]) {
  closeTabs(ids);
  await nextTick();
  document.getElementById(`request-tab-${activeId.value}`)?.focus();
}
/** Show a request in the Browser: open the Browser, expand its groups, select and scroll to it. */
async function reveal(id: number) {
  const session = sessions.value.find((candidate) => candidate.id === id);
  if (!session) return;
  if (narrow.value) mobileBrowserOpen.value = true;
  else sidebarCollapsed.value = false;
  expandAncestors(session.groupId);
  updateSelection([id], id);
  await nextTick();
  const row = document.querySelector<HTMLElement>(`[data-request-id="${id}"]`);
  row?.scrollIntoView({ block: "nearest" });
  row?.focus();
}
```

Replace the two ancestor loops in `create()` and `duplicate()` with `expandAncestors(groupId)` and `expandAncestors(session.groupId)`.

Update the `<RequestTabs>` element:

```vue
<RequestTabs
  :sessions="openSessions"
  :active-id="activeId"
  :curl-for="curlFor"
  @select="select"
  @create="create()"
  @close="close"
  @close-many="closeMany"
  @duplicate="(id) => duplicate(id)"
  @copy="copyText"
  @reveal="reveal"
  @open-requests="placeTabs"
/>
```

In the status bar, after the `SENDING` span:

```vue
<span v-if="copyError" class="text-destructive" role="alert">{{ copyError }}</span>
<span v-else-if="copied" role="status">COPIED</span>
```

- [ ] **Step 4: Run tests, build, and commit**

Run: `bun run test && bun run build`
Expected: PASS.

```bash
git add src/components/RequestTabs.vue src/App.vue src/components/__test__/RequestTabsContextMenu.test.ts
git commit -m "feat: VS Code tab menu actions"
```

---

### Task 6: Browser request row menu

**Files:**
- Modify: `src/components/RequestBrowser.vue`, `src/App.vue`
- Test: `src/components/__test__/RequestBrowserContextMenu.test.ts`

**Interfaces:**
- Consumes: `GroupMenuTree` (Task 4), `shortcutLabel`, App `copyText` and `curlFor` (Task 5).
- Produces: `RequestBrowser` prop `curlFor?: (id: number) => string`; emit `copy: [text: string]`.

- [ ] **Step 1: Write the failing tests**

Add to `RequestBrowserContextMenu.test.ts` (reuse its mount helper; pass `selectedIds` where needed):

```ts
describe("request row menu", () => {
  it("keeps a bearer token when Bearer is chosen again", async () => {
    const s = createSession();
    s.draft.localAuth = { type: "bearer", token: "keep" };
    const onSetRequestLocalAuth = vi.fn();
    const w = mountBrowser({ sessions: [s], activeId: s.id, groups: [], onSetRequestLocalAuth });
    await w.get(`[data-request-id="${s.id}"]`).trigger("contextmenu");
    (document.body.querySelector("[data-auth-menu]") as HTMLElement).click();
    await new Promise((r) => setTimeout(r, 0));
    (document.body.querySelector('[data-auth-mode="bearer"]') as HTMLElement).click();
    expect(onSetRequestLocalAuth).not.toHaveBeenCalled();
  });

  it("checks the current authorization mode", async () => {
    const s = createSession();
    s.draft.localAuth = { type: "none" };
    const w = mountBrowser({ sessions: [s], activeId: s.id, groups: [] });
    await w.get(`[data-request-id="${s.id}"]`).trigger("contextmenu");
    (document.body.querySelector("[data-auth-menu]") as HTMLElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.querySelector('[data-auth-mode="none"]')?.getAttribute("aria-checked")).toBe("true");
  });

  it("labels and deletes a multi-selection after one confirmation", async () => {
    const [a, b, c] = [createSession(), createSession(), createSession()];
    const onDeleteRequest = vi.fn();
    const w = mountBrowser({ sessions: [a, b, c], activeId: a.id, groups: [], selectedIds: [a.id, b.id], onDeleteRequest });
    await w.get(`[data-request-id="${a.id}"]`).trigger("contextmenu");
    const del = document.body.querySelector("[data-request-delete]") as HTMLElement;
    expect(del.textContent?.trim()).toBe("Delete 2 requests");
    del.click();
    await w.vm.$nextTick();
    expect(onDeleteRequest).not.toHaveBeenCalled();
    await w.get("[data-confirm-delete-request]").trigger("click");
    expect(onDeleteRequest.mock.calls.map((call) => call[0])).toEqual([a.id, b.id]);
  });

  it("disables a multi-selection delete when one request is running", async () => {
    const [a, b] = [createSession(), createSession()];
    b.busy = true;
    const w = mountBrowser({ sessions: [a, b], activeId: a.id, groups: [], selectedIds: [a.id, b.id] });
    await w.get(`[data-request-id="${a.id}"]`).trigger("contextmenu");
    expect(document.body.querySelector("[data-request-delete]")?.hasAttribute("data-disabled")).toBe(true);
  });

  it("moves the selection through the nested tree", async () => {
    const [a, b] = [createSession(), createSession()];
    const onMoveRequests = vi.fn();
    const groups = [{ id: 1, name: "API", parentId: null, collapsed: false }];
    const w = mountBrowser({ sessions: [a, b], activeId: a.id, groups, selectedIds: [a.id, b.id], onMoveRequests });
    await w.get(`[data-request-id="${a.id}"]`).trigger("contextmenu");
    const moveMenu = document.body.querySelector("[data-move-menu]") as HTMLElement;
    expect(moveMenu.textContent?.trim()).toBe("Move 2 requests to");
    moveMenu.click();
    await new Promise((r) => setTimeout(r, 0));
    (document.body.querySelector('[data-move-target="1"]') as HTMLElement).click();
    expect(onMoveRequests).toHaveBeenCalledWith([a.id, b.id], 1, null);
  });

  it("copies the target URL", async () => {
    const s = createSession();
    s.draft.url = "https://api.example.test/x";
    const onCopy = vi.fn();
    const w = mountBrowser({ sessions: [s], activeId: s.id, groups: [], onCopy });
    await w.get(`[data-request-id="${s.id}"]`).trigger("contextmenu");
    (document.body.querySelector("[data-request-copy-url]") as HTMLElement).click();
    expect(onCopy).toHaveBeenCalledWith("https://api.example.test/x");
  });
});
```

If the file has no `mountBrowser` helper, add one that mounts `RequestBrowser` with `attachTo: document.body` and pushes the wrapper to the cleanup list.

Run: `bun run test src/components/__test__/RequestBrowserContextMenu.test.ts`
Expected: FAIL.

- [ ] **Step 2: Implement the script changes in `RequestBrowser.vue`**

Add the prop `curlFor?: (id: number) => string;` and the emit `copy: [text: string];`. Import `GroupMenuTree`, `ContextMenuRadioGroup`, `ContextMenuRadioItem`, `ContextMenuShortcut`, and `shortcutLabel`. Add:

```ts
type AuthMode = "inherit" | "none" | "bearer" | "basic";
const sessionById = computed(() => new Map(props.sessions.map((session) => [session.id, session])));
/** The requests a request-row menu acts on: the selection when the target is in it. */
function menuTargets(sessionId: number) {
  return selected.value.has(sessionId) && (props.selectedIds?.length ?? 0) > 1
    ? [...props.selectedIds!]
    : [sessionId];
}
function countLabel(verb: string, ids: number[], suffix = "") {
  return ids.length > 1 ? `${verb} ${ids.length} requests${suffix}` : `${verb}${suffix}`;
}
function authModeOf(id: number): AuthMode {
  return sessionById.value.get(id)?.draft.localAuth?.type ?? "inherit";
}
/** The shared mode of the targets, or undefined when they differ. */
function authMode(ids: number[]) {
  const modes = new Set(ids.map(authModeOf));
  return modes.size === 1 ? [...modes][0] : undefined;
}
function authConfig(mode: AuthMode): AuthorizationConfig | undefined {
  if (mode === "inherit") return undefined;
  if (mode === "none") return { type: "none" };
  if (mode === "bearer") return { type: "bearer", token: "" };
  return { type: "basic", username: "", password: "" };
}
/** Set the mode on each target. A target that already has the mode keeps its credentials. */
function setAuthMode(ids: number[], mode: AuthMode) {
  for (const id of ids)
    if (authModeOf(id) !== mode) emit("setRequestLocalAuth", id, authConfig(mode));
}
function moveTargets(ids: number[], groupId: number | null) {
  if (ids.length > 1) emit("moveRequests", ids, groupId, null);
  else emit("moveRequest", ids[0], groupId);
}
```

Replace `requestDelete` and the confirm state:

```ts
const deletingRequestIds = ref<number[]>([]);
function requestDelete(session: RequestSession) {
  const ids = menuTargets(session.id);
  if (ids.some((id) => sessionById.value.get(id)?.busy)) return;
  if (ids.length > 1 || (props.confirmDelete && hasDraft(session))) {
    deletingRequestId.value = session.id;
    deletingRequestIds.value = ids;
  } else emit("deleteRequest", session.id);
}
function confirmDeleteRequests() {
  for (const id of deletingRequestIds.value) emit("deleteRequest", id);
  deletingRequestId.value = null;
  deletingRequestIds.value = [];
}
```

In the confirm row, the text becomes:

```vue
<p class="w-full leading-[1.45]">
  <template v-if="deletingRequestIds.length > 1">Delete {{ deletingRequestIds.length }} requests? Their drafts and responses are lost.</template>
  <template v-else>Delete {{ sessionLabel(row.session) }}? Its draft and response are lost.</template>
</p>
```

and the confirm button uses `@click="confirmDeleteRequests"` with `:disabled="deletingRequestIds.some((id) => sessionById.get(id)?.busy)"`. The cancel button also clears `deletingRequestIds`.

- [ ] **Step 3: Replace the request row menu content**

```vue
<ContextMenuContent>
  <ContextMenuItem @select="emit('select', row.session.id)">Open</ContextMenuItem>
  <ContextMenuItem @select="emit('duplicateRequest', row.session.id)">
    Duplicate<ContextMenuShortcut>{{ shortcutLabel(["mod", "shift", "d"]) }}</ContextMenuShortcut>
  </ContextMenuItem>
  <ContextMenuItem v-if="openIds?.includes(row.session.id)" @select="emit('closeRequest', row.session.id)">
    Close tab
  </ContextMenuItem>
  <template v-if="menuTargets(row.session.id).length === 1">
    <ContextMenuSeparator />
    <ContextMenuItem data-request-copy-url :disabled="!row.session.draft.url" @select="emit('copy', row.session.draft.url)">Copy URL</ContextMenuItem>
    <ContextMenuItem data-request-copy-curl :disabled="!curlFor?.(row.session.id)" @select="emit('copy', curlFor?.(row.session.id) ?? '')">Copy as cURL</ContextMenuItem>
  </template>
  <ContextMenuSeparator />
  <ContextMenuSub>
    <ContextMenuSubTrigger data-auth-menu>Authorization</ContextMenuSubTrigger>
    <ContextMenuSubContent>
      <ContextMenuRadioGroup
        :model-value="authMode(menuTargets(row.session.id))"
        @update:model-value="(mode) => setAuthMode(menuTargets(row.session.id), mode as AuthMode)"
      >
        <ContextMenuRadioItem value="inherit" data-auth-mode="inherit">Inherit</ContextMenuRadioItem>
        <ContextMenuRadioItem value="none" data-auth-mode="none">No auth</ContextMenuRadioItem>
        <ContextMenuRadioItem value="bearer" data-auth-mode="bearer">Bearer</ContextMenuRadioItem>
        <ContextMenuRadioItem value="basic" data-auth-mode="basic">Basic</ContextMenuRadioItem>
      </ContextMenuRadioGroup>
    </ContextMenuSubContent>
  </ContextMenuSub>
  <ContextMenuSub>
    <ContextMenuSubTrigger data-move-menu>{{ countLabel("Move", menuTargets(row.session.id), " to") }}</ContextMenuSubTrigger>
    <ContextMenuSubContent>
      <GroupMenuTree
        :groups="groups"
        root-label="Ungrouped"
        :current-id="menuTargets(row.session.id).length === 1 ? row.session.groupId : undefined"
        @pick="(groupId) => moveTargets(menuTargets(row.session.id), groupId)"
      />
    </ContextMenuSubContent>
  </ContextMenuSub>
  <ContextMenuSeparator />
  <ContextMenuItem
    data-request-delete
    variant="destructive"
    :disabled="menuTargets(row.session.id).some((id) => sessionById.get(id)?.busy)"
    @select="requestDelete(row.session)"
  >
    {{ countLabel("Delete", menuTargets(row.session.id)) }}
  </ContextMenuItem>
</ContextMenuContent>
```

The "Move" label for one request reads "Move to". Reka emits `update:modelValue` only when the value changes, and `setAuthMode` also skips an unchanged target, so a repeated choice keeps credentials in both cases.

- [ ] **Step 4: Wire `App.vue`**

On `<RequestBrowser>`, add `:curl-for="curlFor"` and `@copy="copyText"`.

- [ ] **Step 5: Run tests and commit**

Run: `bun run test src/components/__test__/RequestBrowser*.test.ts && bun run build`
Expected: PASS.

```bash
git add src/components/RequestBrowser.vue src/App.vue src/components/__test__/RequestBrowserContextMenu.test.ts
git commit -m "feat: Browser request menu with selection, auth state, and nested move"
```

---

### Task 7: Group, Ungrouped, and blank-space menus

**Files:**
- Create: `src/components/GroupMenuItems.vue`
- Modify: `src/components/GroupActionsMenu.vue`, `src/components/RequestBrowser.vue`
- Test: `src/components/__test__/GroupMenuItems.test.ts` (new), `src/components/__test__/RequestBrowser.test.ts`, `src/components/__test__/RequestBrowserContextMenu.test.ts`

**Interfaces:**
- Consumes: `GroupMenuTree` (Task 4).
- Produces: `type GroupAction = "createRequest" | "createGroup" | "rename" | "settings" | "toggle" | "collapseAll" | "moveSelection" | "delete"` exported from `GroupMenuItems.vue` (in a `<script lang="ts">` block). `<GroupMenuItems :group :groups :can-move-selection :kind? @action="(a: GroupAction) => …" @move-to="(parentId: number | null) => …" />`.
- Produces: `<GroupActionsMenu :group :groups :can-move-selection @action @move-to />` (props `name` and `canMove` are removed).

- [ ] **Step 1: Write the failing tests**

`src/components/__test__/GroupMenuItems.test.ts`:

```ts
import { afterEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import GroupMenuItems from "../GroupMenuItems.vue";
import { menuLabels } from "./menu-test-utils";

const groups = [
  { id: 1, name: "Platform", parentId: null, collapsed: false },
  { id: 2, name: "Identity", parentId: 1, collapsed: true },
  { id: 3, name: "Billing", parentId: null, collapsed: false },
];
afterEach(() => { document.body.innerHTML = ""; });

async function open(groupIndex: number, canMoveSelection = false) {
  const actions: string[] = [];
  const moves: (number | null)[] = [];
  const w = mount(defineComponent({ setup: () => () =>
    h(ContextMenu, null, { default: () => [
      h(ContextMenuTrigger, { asChild: true }, { default: () => h("button", { "data-trigger": "" }, "t") }),
      h(ContextMenuContent, null, { default: () => h(GroupMenuItems, {
        group: groups[groupIndex], groups, canMoveSelection,
        onAction: (a: string) => actions.push(a), onMoveTo: (id: number | null) => moves.push(id),
      }) }),
    ] }) }), { attachTo: document.body });
  await w.get("[data-trigger]").trigger("contextmenu");
  return { actions, moves };
}
const labels = () => menuLabels();

describe("GroupMenuItems", () => {
  it("lists the group actions in order", async () => {
    await open(0);
    expect(labels()).toEqual(["New request", "New group", "Rename", "Settings…", "Collapse", "Collapse all", "Move to", "Delete group"]);
  });

  it("offers Move selection here only with a movable selection", async () => {
    await open(0, true);
    expect(labels()).toContain("Move selection here");
  });

  it("labels a collapsed group Expand", async () => {
    await open(1);
    expect(labels()).toContain("Expand");
  });

  it("marks Delete group destructive and emits delete", async () => {
    const { actions } = await open(0);
    const del = document.body.querySelector("[data-group-delete]") as HTMLElement;
    expect(del.getAttribute("data-variant")).toBe("destructive");
    del.click();
    expect(actions).toEqual(["delete"]);
  });

  it("does not offer the group or its subtree as a Move to target", async () => {
    const { moves } = await open(0);
    (document.body.querySelector("[data-group-move-menu]") as HTMLElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.querySelector('[data-move-target="1"]')).toBeNull();
    expect(document.body.querySelector('[data-move-target="2"]')).toBeNull();
    (document.body.querySelector('[data-move-target="3"]') as HTMLElement).click();
    expect(moves).toEqual([3]);
  });

  it("marks Top level as current for a top-level group", async () => {
    await open(0);
    (document.body.querySelector("[data-group-move-menu]") as HTMLElement).click();
    await new Promise((r) => setTimeout(r, 0));
    expect(document.body.querySelector('[data-move-target="root"]')?.getAttribute("aria-checked")).toBe("true");
  });
});
```

In `RequestBrowser.test.ts`, the second test clicks `[aria-label="Add group inside Platform"]`; keep that aria-label on the "New group" item. The first test finds "Move selection here" by text; keep that label.

Run: `bun run test src/components/__test__/GroupMenuItems.test.ts`
Expected: FAIL.

- [ ] **Step 2: Implement `src/components/GroupMenuItems.vue`**

```vue
<script lang="ts">
export type GroupAction =
  | "createRequest"
  | "createGroup"
  | "rename"
  | "settings"
  | "toggle"
  | "collapseAll"
  | "moveSelection"
  | "delete";
</script>

<script setup lang="ts">
import { computed } from "vue";
import type { RequestGroup } from "@/lib/groups";
import GroupMenuTree from "./GroupMenuTree.vue";
import { ContextMenuItem, ContextMenuSeparator, ContextMenuSub, ContextMenuSubContent, ContextMenuSubTrigger } from "@/components/ui/context-menu";
import { DropdownMenuItem, DropdownMenuSeparator, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger } from "@/components/ui/dropdown-menu";

// The group menu, shared by the row's context menu and its ⋯ dropdown.
const props = withDefaults(
  defineProps<{
    group: RequestGroup;
    groups: RequestGroup[];
    canMoveSelection: boolean;
    kind?: "context" | "dropdown";
  }>(),
  { kind: "context" },
);
const emit = defineEmits<{ action: [action: GroupAction]; moveTo: [parentId: number | null] }>();
const ui = computed(() =>
  props.kind === "dropdown"
    ? { Item: DropdownMenuItem, Separator: DropdownMenuSeparator, Sub: DropdownMenuSub, SubTrigger: DropdownMenuSubTrigger, SubContent: DropdownMenuSubContent }
    : { Item: ContextMenuItem, Separator: ContextMenuSeparator, Sub: ContextMenuSub, SubTrigger: ContextMenuSubTrigger, SubContent: ContextMenuSubContent },
);
</script>

<template>
  <component :is="ui.Item" @select="emit('action', 'createRequest')">New request</component>
  <component :is="ui.Item" :aria-label="`Add group inside ${group.name}`" @select="emit('action', 'createGroup')">New group</component>
  <component :is="ui.Separator" />
  <component :is="ui.Item" @select="emit('action', 'rename')">Rename</component>
  <component :is="ui.Item" @select="emit('action', 'settings')">Settings…</component>
  <component :is="ui.Separator" />
  <component :is="ui.Item" @select="emit('action', 'toggle')">{{ group.collapsed ? "Expand" : "Collapse" }}</component>
  <component :is="ui.Item" @select="emit('action', 'collapseAll')">Collapse all</component>
  <component :is="ui.Separator" />
  <component :is="ui.Item" v-if="canMoveSelection" @select="emit('action', 'moveSelection')">Move selection here</component>
  <component :is="ui.Sub">
    <component :is="ui.SubTrigger" data-group-move-menu>Move to</component>
    <component :is="ui.SubContent">
      <GroupMenuTree :groups="groups" root-label="Top level" :current-id="group.parentId" :exclude-id="group.id" :kind="kind" @pick="emit('moveTo', $event)" />
    </component>
  </component>
  <component :is="ui.Separator" />
  <component :is="ui.Item" data-group-delete variant="destructive" @select="emit('action', 'delete')">Delete group</component>
</template>
```

Run the test. Expected: PASS.

- [ ] **Step 3: Rewrite `GroupActionsMenu.vue`**

```vue
<script setup lang="ts">
import { Ellipsis } from "lucide-vue-next";
import type { RequestGroup } from "@/lib/groups";
import GroupMenuItems, { type GroupAction } from "./GroupMenuItems.vue";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

defineProps<{ group: RequestGroup; groups: RequestGroup[]; canMoveSelection: boolean }>();
const emit = defineEmits<{ action: [action: GroupAction]; moveTo: [parentId: number | null] }>();
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger
      class="inline-flex size-5.5 shrink-0 items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground pointer-coarse:size-8"
      :aria-label="`More actions for ${group.name}`"
    >
      <Ellipsis :size="14" aria-hidden="true" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" :side-offset="4">
      <GroupMenuItems
        kind="dropdown"
        :group="group"
        :groups="groups"
        :can-move-selection="canMoveSelection"
        @action="emit('action', $event)"
        @move-to="emit('moveTo', $event)"
      />
    </DropdownMenuContent>
  </DropdownMenu>
</template>
```

- [ ] **Step 4: Use `GroupMenuItems` in `RequestBrowser.vue`**

Add one handler:

```ts
import GroupMenuItems, { type GroupAction } from "./GroupMenuItems.vue";

function onGroupAction(group: RequestGroup, action: GroupAction) {
  if (action === "createRequest") emit("createRequest", group.id);
  else if (action === "createGroup") startCreating(group.id);
  else if (action === "rename") startRename(group);
  else if (action === "settings") emit("openGroupSettings", group.id);
  else if (action === "toggle") emit("toggleGroup", group.id);
  else if (action === "collapseAll") emit("collapseAllGroups");
  else if (action === "moveSelection") moveSelection(group.id);
  else deletingId.value = group.id;
}
const hasMovableSelection = (groupId: number | null) =>
  (props.selectedIds?.length ?? 0) > 0 && !selectionAlreadyIn(groupId);
```

`hasMovableSelection` uses only `selectedIds`, not the active request: "Move selection here" shows only when requests are selected.

Replace the `<GroupActionsMenu … />` element with:

```vue
<GroupActionsMenu
  :group="row.group"
  :groups="groups"
  :can-move-selection="hasMovableSelection(row.group.id)"
  @action="(action) => onGroupAction(row.group, action)"
  @move-to="(parentId) => emit('moveGroup', row.group.id, parentId, null)"
/>
```

Replace the group row `<ContextMenuContent>` body with:

```vue
<ContextMenuContent>
  <GroupMenuItems
    :group="row.group"
    :groups="groups"
    :can-move-selection="hasMovableSelection(row.group.id)"
    @action="(action) => onGroupAction(row.group, action)"
    @move-to="(parentId) => emit('moveGroup', row.group.id, parentId, null)"
  />
</ContextMenuContent>
```

Ungrouped row menu:

```vue
<ContextMenuContent>
  <ContextMenuItem @select="emit('createRequest', null)">New request</ContextMenuItem>
  <ContextMenuItem v-if="hasMovableSelection(null)" @select="moveSelection(null)">Move selection here</ContextMenuItem>
</ContextMenuContent>
```

Blank-space menu:

```vue
<ContextMenuContent>
  <ContextMenuItem @select="emit('createRequest', null)">New request</ContextMenuItem>
  <ContextMenuItem @select="startCreating(null)">New group</ContextMenuItem>
  <ContextMenuItem :disabled="!groups.length" @select="emit('collapseAllGroups')">Collapse all</ContextMenuItem>
</ContextMenuContent>
```

The first test in `RequestBrowser.test.ts` moves the *active* request with "Move selection here" and passes no `selectedIds`. Update that test to pass `selectedIds: [active.id]`, because the item now requires a selection.

- [ ] **Step 5: Add a call-site test for group moves**

Append to `RequestBrowserContextMenu.test.ts`:

```ts
it("moves a group through its Move to menu, never into its own subtree", async () => {
  const onMoveGroup = vi.fn();
  const groups = [
    { id: 1, name: "Platform", parentId: null, collapsed: false },
    { id: 2, name: "Identity", parentId: 1, collapsed: false },
    { id: 3, name: "Billing", parentId: null, collapsed: false },
  ];
  const s = createSession();
  const w = mountBrowser({ sessions: [s], activeId: s.id, groups, onMoveGroup });
  await w.get('[data-group-id="1"]').trigger("contextmenu");
  (document.body.querySelector("[data-group-move-menu]") as HTMLElement).click();
  await new Promise((r) => setTimeout(r, 0));
  expect(document.body.querySelector('[data-move-target="2"]')).toBeNull();
  (document.body.querySelector('[data-move-target="3"]') as HTMLElement).click();
  expect(onMoveGroup).toHaveBeenCalledWith(1, 3, null);
});
```

- [ ] **Step 6: Run tests and commit**

Run: `bun run test src/components && bun run build`
Expected: PASS.

```bash
git add src/components/GroupMenuItems.vue src/components/GroupActionsMenu.vue src/components/RequestBrowser.vue src/components/__test__/GroupMenuItems.test.ts src/components/__test__/RequestBrowser.test.ts src/components/__test__/RequestBrowserContextMenu.test.ts
git commit -m "feat: shared group menu with Move to, Ungrouped and Browser menus"
```

---

### Task 8: Request bar and cURL preview menus

**Files:**
- Modify: `src/components/RequestWorkspace.vue`
- Test: `src/components/__test__/RequestWorkspaceContextMenu.test.ts`

**Interfaces:**
- Consumes: `shortcutLabel`, `ContextMenuCheckboxItem`, `ContextMenuShortcut`.

- [ ] **Step 1: Write the failing tests**

Append (reuse the file's mount helper and its way of opening the request-bar menu; import `menuLabels` from `./menu-test-utils`):

```ts
it("request bar menu lists Send, Focus URL, Copy URL, Copy as cURL, Show cURL", async () => {
  const w = mountWorkspace();
  await openRequestBarMenu(w);
  expect(menuLabels()).toEqual(["Send", "Focus URL", "Copy URL", "Copy as cURL", "Show cURL"]);
  expect(document.body.querySelector('[data-testid="ctx-show-curl"]')?.getAttribute("role")).toBe("menuitemcheckbox");
});

it("request bar shows the Send and Focus URL shortcuts", async () => {
  const w = mountWorkspace();
  await openRequestBarMenu(w);
  expect(document.body.querySelectorAll('[data-slot="context-menu-shortcut"]').length).toBe(2);
});
```

If the helpers have other names in the file, use those. Keep the existing `ctx-send`, `ctx-focus-url`, `ctx-show-curl`, `ctx-copy-curl`, `ctx-close-curl` tests.

Run: `bun run test src/components/__test__/RequestWorkspaceContextMenu.test.ts`
Expected: FAIL.

- [ ] **Step 2: Implement**

Rename the local `const shortcut = /Mac/i.test(...) ? "⌘" : "Ctrl";` only if it conflicts; `shortcutLabel` has a different name, so both can stay. Request bar menu:

```vue
<ContextMenuContent>
  <ContextMenuItem data-testid="ctx-send" :disabled="session.busy || !prepared.request" @select="send()">
    Send<ContextMenuShortcut>{{ shortcutLabel(["mod", "enter"]) }}</ContextMenuShortcut>
  </ContextMenuItem>
  <ContextMenuItem data-testid="ctx-focus-url" @select="focusUrl()">
    Focus URL<ContextMenuShortcut>{{ shortcutLabel(["mod", "l"]) }}</ContextMenuShortcut>
  </ContextMenuItem>
  <ContextMenuSeparator />
  <ContextMenuItem data-testid="ctx-copy-url" :disabled="!session.draft.url" @select="copy(session.draft.url)">Copy URL</ContextMenuItem>
  <ContextMenuItem data-testid="ctx-copy-curl-bar" :disabled="!curl" @select="copy(curl)">Copy as cURL</ContextMenuItem>
  <ContextMenuCheckboxItem data-testid="ctx-show-curl" v-model:model-value="showCurl">Show cURL</ContextMenuCheckboxItem>
</ContextMenuContent>
```

cURL preview menu:

```vue
<ContextMenuContent>
  <ContextMenuItem data-testid="ctx-copy-curl" @select="copy(curl)">Copy cURL</ContextMenuItem>
  <ContextMenuSeparator />
  <ContextMenuItem data-testid="ctx-close-curl" @select="showCurl = false">
    Close<ContextMenuShortcut>{{ shortcutLabel(["esc"]) }}</ContextMenuShortcut>
  </ContextMenuItem>
</ContextMenuContent>
```

If an existing test clicks `ctx-show-curl` and expects a toggle, it still passes: a checkbox item toggles its model on select.

- [ ] **Step 3: Run tests and commit**

Run: `bun run test src/components/__test__/RequestWorkspace*.test.ts`
Expected: PASS.

```bash
git add src/components/RequestWorkspace.vue src/components/__test__/RequestWorkspaceContextMenu.test.ts
git commit -m "feat: request bar menu with copy actions and shortcuts"
```

---

### Task 9: Query and header table menus

**Files:**
- Modify: `src/components/KeyValueEditor.vue`
- Test: `src/components/__test__/KeyValueEditorContextMenu.test.ts`

- [ ] **Step 1: Write the failing tests**

Append (reuse the file's mount and open helpers):

```ts
it("row menu has Enabled as a checkbox, copy items, and a destructive Delete row", async () => {
  const { wrapper, rows } = mountEditor([{ key: "a", value: "1", enabled: true }]);
  await openRowMenu(wrapper, rows[0].id);
  const toggle = document.body.querySelector('[data-testid="kv-row-ctx-toggle"]')!;
  expect(toggle.getAttribute("role")).toBe("menuitemcheckbox");
  expect(toggle.getAttribute("aria-checked")).toBe("true");
  expect(document.body.querySelector('[data-testid="kv-row-ctx-remove"]')?.getAttribute("data-variant")).toBe("destructive");
  expect(document.body.querySelector('[data-testid="kv-row-ctx-remove"]')?.textContent?.trim()).toBe("Delete row");
});

it("Copy name copies the row key", async () => {
  const writeText = vi.fn(() => Promise.resolve());
  Object.assign(navigator, { clipboard: { writeText } });
  const { wrapper, rows } = mountEditor([{ key: "X-Id", value: "7", enabled: true }]);
  await openRowMenu(wrapper, rows[0].id);
  (document.body.querySelector('[data-testid="kv-row-ctx-copy-name"]') as HTMLElement).click();
  expect(writeText).toHaveBeenCalledWith("X-Id");
});
```

Use the helper names that the file already has; add small helpers if it has none.

Run: `bun run test src/components/__test__/KeyValueEditorContextMenu.test.ts`
Expected: FAIL.

- [ ] **Step 2: Implement**

Script: `import { useClipboard } from "@/composables/useClipboard"; const { copyError, copy } = useClipboard();` and import `ContextMenuCheckboxItem`.

Row menu:

```vue
<ContextMenuContent>
  <ContextMenuCheckboxItem data-testid="kv-row-ctx-toggle" :model-value="row.enabled" :disabled="disabled" @select="toggleRow(row.id)">
    Enabled
  </ContextMenuCheckboxItem>
  <ContextMenuItem data-testid="kv-row-ctx-duplicate" :disabled="disabled" @select="duplicateRow(row.id)">Duplicate row</ContextMenuItem>
  <ContextMenuSeparator />
  <ContextMenuItem data-testid="kv-row-ctx-copy-name" :disabled="!row.key" @select="copy(row.key)">Copy name</ContextMenuItem>
  <ContextMenuItem data-testid="kv-row-ctx-copy-value" :disabled="!row.value" @select="copy(row.value)">Copy value</ContextMenuItem>
  <ContextMenuSeparator />
  <ContextMenuItem data-testid="kv-row-ctx-remove" variant="destructive" :disabled="disabled" @select="removeRow(row.id)">Delete row</ContextMenuItem>
</ContextMenuContent>
```

The checkbox uses `:model-value` with `@select`, not `v-model`, so the toggle goes through `toggleRow` and its `canEdit()` guard.

Under the table, add the clipboard error:

```vue
<p v-if="copyError" class="px-3 py-1 text-xs text-destructive" role="alert">{{ copyError }}</p>
```

The table menu keeps its items.

- [ ] **Step 3: Run tests and commit**

Run: `bun run test src/components/__test__/KeyValueEditorContextMenu.test.ts`
Expected: PASS. If an older test expects the label "Disable"/"Enable" or "Remove", update it to "Enabled"/"Delete row".

```bash
git add src/components/KeyValueEditor.vue src/components/__test__/KeyValueEditorContextMenu.test.ts
git commit -m "feat: key-value row menu with Enabled check and copy items"
```

---

### Task 10: Body and authorization editor menus

**Files:**
- Modify: `src/components/RequestEditor.vue`
- Test: `src/components/__test__/RequestEditorInheritedAuth.test.ts`, a new `describe` in the same file for the body menu

- [ ] **Step 1: Write the failing tests**

Append to `RequestEditorInheritedAuth.test.ts` (reuse its mount helper, here called `mountEditor`, and its way of opening the auth menu):

```ts
it("auth menu shows the current mode as a checked radio item", async () => {
  const w = mountEditor({ localAuth: { type: "basic", username: "", password: "" } });
  await openAuthMenu(w);
  const checked = [...document.body.querySelectorAll('[role="menuitemradio"][aria-checked="true"]')].map((el) => el.textContent?.trim());
  expect(checked).toEqual(["Basic auth"]);
});

it("body menu shows the body type as radio items in a submenu and Clear body as destructive", async () => {
  const w = mountEditor({ bodyMode: "json", body: "{}" });
  await openBodyMenu(w);
  expect(document.body.querySelector('[data-testid="body-menu-clear"]')?.getAttribute("data-variant")).toBe("destructive");
  (document.body.querySelector('[data-testid="body-menu-type"]') as HTMLElement).click();
  await new Promise((r) => setTimeout(r, 0));
  const checked = document.body.querySelector('[role="menuitemradio"][aria-checked="true"]');
  expect(checked?.textContent?.trim()).toBe("JSON");
});
```

Run: `bun run test src/components/__test__/RequestEditorInheritedAuth.test.ts`
Expected: FAIL.

- [ ] **Step 2: Implement**

Body menu:

```vue
<ContextMenuContent>
  <ContextMenuSub>
    <ContextMenuSubTrigger data-testid="body-menu-type" :disabled="busy">Body type</ContextMenuSubTrigger>
    <ContextMenuSubContent>
      <ContextMenuRadioGroup :model-value="draft.bodyMode" @update:model-value="(mode) => !busy && (draft.bodyMode = mode as typeof draft.bodyMode)">
        <ContextMenuRadioItem value="none">None</ContextMenuRadioItem>
        <ContextMenuRadioItem value="json">JSON</ContextMenuRadioItem>
        <ContextMenuRadioItem value="text">Text</ContextMenuRadioItem>
        <ContextMenuRadioItem value="graphql">GraphQL</ContextMenuRadioItem>
      </ContextMenuRadioGroup>
    </ContextMenuSubContent>
  </ContextMenuSub>
  <ContextMenuSeparator />
  <ContextMenuItem data-testid="body-menu-format" :disabled="busy || !formattable || !draft.body" @select="formatBody">
    {{ draft.bodyMode === "graphql" ? "Format GraphQL" : "Format JSON" }}
  </ContextMenuItem>
  <ContextMenuItem data-testid="body-menu-clear" variant="destructive" :disabled="busy || !draft.body" @select="clearBody">
    Clear body
  </ContextMenuItem>
</ContextMenuContent>
```

Authorization menu:

```vue
<ContextMenuContent>
  <ContextMenuRadioGroup :model-value="authType" @update:model-value="(value) => setAuthType(value as string)">
    <ContextMenuRadioItem value="inherit" :disabled="busy">Inherit</ContextMenuRadioItem>
    <ContextMenuRadioItem value="none" :disabled="busy">No auth</ContextMenuRadioItem>
    <ContextMenuRadioItem value="bearer" :disabled="busy">Bearer token</ContextMenuRadioItem>
    <ContextMenuRadioItem value="basic" :disabled="busy">Basic auth</ContextMenuRadioItem>
  </ContextMenuRadioGroup>
  <ContextMenuSeparator />
  <ContextMenuItem data-testid="auth-menu-clear-local" :disabled="draft.localAuth === undefined" @select="setAuthType('inherit')">
    Clear local override
  </ContextMenuItem>
</ContextMenuContent>
```

`authType` is the value that the auth `<select>` already shows. If the component has no computed for it, add:

```ts
const authType = computed(() => props.draft.localAuth?.type ?? "inherit");
```

(use `draft` from the component's existing prop or model name).

- [ ] **Step 3: Run tests and commit**

Run: `bun run test src/components/__test__/RequestEditor*.test.ts`
Expected: PASS.

```bash
git add src/components/RequestEditor.vue src/components/__test__/RequestEditorInheritedAuth.test.ts
git commit -m "feat: body type and authorization menus show the current mode"
```

---

### Task 11: Response toolbar and JSON tree menus

**Files:**
- Modify: `src/components/ResponsePanel.vue`, `src/components/JsonTreeView.vue`
- Test: `src/components/__test__/ResponsePanelContextMenu.test.ts`, `src/components/__test__/JsonTreeViewContextMenu.test.ts`

- [ ] **Step 1: Write the failing tests**

`ResponsePanelContextMenu.test.ts` (reuse its helpers):

```ts
it("Pretty and Wrap lines are checkbox items that show their state; Find shows its shortcut", async () => {
  const w = mountPanel({ json: true });
  await openToolbarMenu(w);
  const wrap = document.body.querySelector('[data-testid="ctx-toolbar-wrap"]')!;
  expect(wrap.getAttribute("role")).toBe("menuitemcheckbox");
  expect(wrap.textContent?.trim()).toBe("Wrap lines");
  expect(document.body.querySelector('[data-testid="ctx-toolbar-pretty"]')?.getAttribute("role")).toBe("menuitemcheckbox");
  expect(document.body.querySelector('[data-testid="ctx-toolbar-find"] [data-slot="context-menu-shortcut"]')).not.toBeNull();
});
```

`JsonTreeViewContextMenu.test.ts`:

```ts
it("Collapse all collapses every container below the root; Expand all restores them", async () => {
  const w = mountTree('{"a":{"b":{"c":1}},"d":[1]}');
  await openRowMenu(w, 0);
  (document.body.querySelector('[data-testid="ctx-collapse-all"]') as HTMLElement).click();
  await w.vm.$nextTick();
  expect(visibleLabels(w)).toEqual(["root {2}", "a {1}", "d [1]"]);
  await openRowMenu(w, 0);
  (document.body.querySelector('[data-testid="ctx-expand-all"]') as HTMLElement).click();
  await w.vm.$nextTick();
  expect(visibleLabels(w)).toContain("c 1");
});
```

Use the file's existing mount and label helpers; add them if they do not exist. The labels follow `JsonTreeView`'s `label` field (`key valueLabel`).

Run both test files. Expected: FAIL.

- [ ] **Step 2: Implement the response toolbar**

```vue
<ContextMenuContent>
  <ContextMenuItem data-testid="ctx-copy-response" @select="copyResult">Copy response</ContextMenuItem>
  <ContextMenuItem data-testid="ctx-save-response" :disabled="!savable" @select="saveBody">Save response body…</ContextMenuItem>
  <template v-if="tab === 'body' && !response.binary">
    <ContextMenuSeparator />
    <ContextMenuCheckboxItem v-if="parsed" data-testid="ctx-toolbar-pretty" v-model:model-value="pretty">Pretty</ContextMenuCheckboxItem>
    <ContextMenuCheckboxItem data-testid="ctx-toolbar-wrap" v-model:model-value="wrap">Wrap lines</ContextMenuCheckboxItem>
    <ContextMenuItem data-testid="ctx-toolbar-find" @select="toggleInspector">
      Find<ContextMenuShortcut>{{ shortcutLabel(["mod", "f"]) }}</ContextMenuShortcut>
    </ContextMenuItem>
  </template>
</ContextMenuContent>
```

`pretty` and `wrap` are writable computeds, so `v-model` works.

- [ ] **Step 3: Implement the JSON tree menu**

Script:

```ts
/** Ids of every container below the root. */
function containerIds(value: unknown, id: string): string[] {
  if (!isContainer(value)) return [];
  return childEntries(value).flatMap(([key, child]) => {
    const childId = `${id}/${key}`;
    return isContainer(child) ? [childId, ...containerIds(child, childId)] : [];
  });
}
function collapseAll() {
  collapsed.value = new Set(containerIds(value.value, "$"));
}
function expandAll() {
  collapsed.value = new Set();
}
```

Menu:

```vue
<ContextMenuContent>
  <ContextMenuItem data-testid="ctx-copy-path" @select="copyPath">Copy path</ContextMenuItem>
  <ContextMenuItem data-testid="ctx-copy-value" @select="copyValue">Copy value</ContextMenuItem>
  <ContextMenuSeparator data-testid="ctx-separator" />
  <ContextMenuItem v-if="contextRow?.container" data-testid="ctx-toggle" @select="contextRow && toggle(contextRow)">
    {{ contextRow && collapsed.has(contextRow.id) ? "Expand" : "Collapse" }}
  </ContextMenuItem>
  <ContextMenuItem data-testid="ctx-expand-all" :disabled="!collapsed.size" @select="expandAll">Expand all</ContextMenuItem>
  <ContextMenuItem data-testid="ctx-collapse-all" @select="collapseAll">Collapse all</ContextMenuItem>
</ContextMenuContent>
```

If an existing test expects `ctx-separator` to be absent for a leaf row, update it: the separator now always shows because "Expand all" and "Collapse all" always show.

- [ ] **Step 4: Run tests and commit**

Run: `bun run test src/components/__test__/ResponsePanel*.test.ts src/components/__test__/JsonTreeView*.test.ts`
Expected: PASS.

```bash
git add src/components/ResponsePanel.vue src/components/JsonTreeView.vue src/components/__test__/ResponsePanelContextMenu.test.ts src/components/__test__/JsonTreeViewContextMenu.test.ts
git commit -m "feat: response and JSON tree menus with checks and expand all"
```

---

### Task 12: Documentation and final verification

**Files:**
- Modify: `DESIGN.md`

- [ ] **Step 1: Update `DESIGN.md`**

In "Typography", after the monospace sentence, add: `Menus use the system sans-serif at 12 px; only data inside a menu item, such as an HTTP method, uses monospace.` In "Interaction rules", add: `Context menus follow VS Code: a check gutter on every item, shortcut hints at the right, submenus for nested groups, and destructive items in the error color. Shift+F10 and the Context Menu key open the menu for the focused element.`

- [ ] **Step 2: Run the full verification**

Run: `bun run test && bun run lint && bun run build && bun run test:e2e`
Expected: all PASS. The e2e suite reuses the dev server on port 1420.

- [ ] **Step 3: Check the look in the running app**

Open http://127.0.0.1:1420. Right-click a tab, a Browser request (with 2 selected), and a group, and open the `⋯` group menu. Take screenshots in the default theme and in one Ghostty theme. Check: sans 12 px text, 24 px rows, aligned labels, right-aligned shortcuts, chevrons, checks, red Delete items, fade with no scale, no `primary` color.

- [ ] **Step 4: Commit**

```bash
git add DESIGN.md
git commit -m "docs: context menu rules"
```
