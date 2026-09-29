# Context menu overhaul

Date: 2026-09-28
Status: Approved design, pending implementation plan

## Goal

Make every context menu and dropdown menu in Blink look and behave like VS
Code. Replace the hand-written menu primitives with the official shadcn-vue
components. Add the actions that users expect on each surface. All colors come
from the theme tokens.

## Problems with the current implementation

1. `src/components/ui/context-menu/` is a hand-written subset (6 files) of the
   shadcn-vue component. It has no shortcut, checkbox item, radio item, label,
   group, or destructive variant. The sub-trigger has no chevron.
2. `GroupActionsMenu.vue` and the tab-overflow menu in `RequestTabs.vue` use
   raw Reka `DropdownMenu*` parts with a second, different style.
3. Menus use the monospace font. DESIGN.md reserves monospace for data.
4. The open animation scales the menu. VS Code only fades.
5. Labels are not consistent: "Select" and "Open", "Close" and "Close tab",
   "Delete" and "Delete group".
6. Mode choices (authorization, body type, pretty, wrap) are plain items. They
   do not show the current value.
7. "Move to group" is a flat list. It ignores nesting.
8. Menus ignore the Browser multi-selection.
9. Common actions are missing: close others, close to the right, close all,
   copy URL, copy as cURL, reveal in Browser, move group.
10. The tab "Duplicate" item selects the tab first and then duplicates the
    active tab.
11. `Shift+F10` does not open a menu. macOS WebKit does not synthesize a
    `contextmenu` event for it.

## Scope

In scope:

- Official shadcn-vue `context-menu` and `dropdown-menu` components.
- Visual restyle of all menus.
- The action set for each surface in this document.
- `Shift+F10` and the Context Menu key.

Out of scope:

- A declarative menu or command registry shared with the command center.
- Renaming requests. A request has no name field; its label comes from the
  URL.
- Leading icons in menu items.
- Changes to the CodeMirror native text menu.

## Components

### Install

Run `bunx --bun shadcn-vue@latest add context-menu dropdown-menu`. This
overwrites the 6 files in `src/components/ui/context-menu/` and adds
`src/components/ui/dropdown-menu/`.

`components.json` has `style: "vega"`. The registry has no `vega` menu items
(404). Install from `new-york-v4`, which has them.

Fix the registry output:

- Change `@lucide/vue` imports to `lucide-vue-next`.
- Add the `@vueuse/core` dependency with `bun add`.
- Remove the `animate-in`, `animate-out`, `zoom-*`, and `slide-in-*` classes.
  They need `tw-animate-css`, which the project does not use. Use the fade in
  "Visual spec".
- Add `data-surface="context-menu"` to `ContextMenuContent`,
  `ContextMenuSubContent`, `DropdownMenuContent`, and
  `DropdownMenuSubContent`. `App.vue` and `ResponsePanel.vue` use this
  attribute to ignore global shortcuts while a menu is open.
- Add `--popover` and `--popover-foreground` tokens to `src/style.css` for
  both the default theme and the Ghostty theme. They equal `--secondary` and
  `--foreground`. Map them in `@theme inline`. Remove the
  `[data-surface="context-menu"] { background-color: … !important }` rule.
- Keep the `index.ts` barrel exports. Call sites import from
  `@/components/ui/context-menu` and `@/components/ui/dropdown-menu`.

Style changes go in the ui files only. Call sites use `class` for layout, not
for colors or type.

### New Blink components

- `GroupMenuTree.vue`: a recursive "Move to" submenu. Props: `groups`,
  `excludeIds` (a set), `currentId` (a group id or `null`), `rootLabel`
  ("Ungrouped" or "Top level"), and `kind` (`"context"` or `"dropdown"`). It
  emits `pick(groupId | null)`. It renders the context-menu parts or the
  dropdown-menu parts from `kind`.
- A `shortcut(keys)` helper in `src/lib/` that formats `⌘`/`⇧`/`↵` on macOS
  and `Ctrl+`/`Shift+`/`Enter` on other platforms.

## Visual spec

Surface:

- Radius 8 px. Border `border`. Background `popover`. Text
  `popover-foreground`.
- One soft shadow that uses a theme token.
- Padding 4 px. Minimum width 180 px. Maximum height: the available viewport
  height, with vertical scroll.
- System sans-serif, 12 px (`text-xs`). Group names in "Move to" also use sans.
- Motion: a 100 ms opacity fade-in. No scale. No exit animation. No motion
  when `prefers-reduced-motion: reduce`.

Item row:

- Height 24 px (`py-1`). On `pointer-coarse`, 32 px.
- Columns: check gutter (`pl-8` with the check at `left-2`), label, shortcut,
  submenu chevron. Plain items use `inset` so all labels align.
- Highlight: `accent` background with `accent-foreground` text. Menus never
  use `primary`.
- Disabled: 40% opacity. Show a disabled item only when it explains a current
  restriction.
- Destructive (`variant="destructive"`): `destructive` text, also when it is
  highlighted.
- Shortcut: `muted-foreground`, right-aligned.

Other parts:

- Separator: 1 px `border`, 4 px vertical margin, no inset.
- Label: 11 px `muted-foreground` text that you cannot select.
- Submenu: `ChevronRight` at 12 px. It opens on hover delay, `→`, or `Enter`.
- Checkbox and radio items: `Check` at 12 px in the gutter.

## Actions for each surface

Labels use sentence case. A shortcut shows only when the shortcut already
exists. Every action uses the same mutation path as its button or keyboard
command. `App.vue` and `useWorkspaceState.ts` keep ownership of state changes.

### Request tab (`RequestTabs.vue`)

- Close `⌘W`
- Close others
- Close to the right (disabled on the last tab)
- Close all
- separator
- Duplicate `⇧⌘D`. This calls `duplicate(id)` for the target tab.
- separator
- Copy URL
- Copy as cURL (disabled when the draft does not build)
- separator
- Reveal in Browser

"Reveal in Browser" shows the Browser if it is hidden, expands the ancestor
groups, selects the request, and scrolls it into view.

"Close others", "Close to the right", and "Close all" use one new
`closeTabs(ids)` action in `useWorkspaceState.ts`. It removes the ids from
`openIds` and keeps the active tab when it stays open. Closing a tab never
deletes a request. A final blank tab stays usable, as today.

### Tab strip blank space

- New request `⌘T`
- Close all

### Tab overflow dropdown

Restyle only. It uses the official `dropdown-menu` parts.

### Browser request row (`RequestBrowser.vue`)

When the target is part of the multi-selection, the menu acts on the whole
selection and the labels show the count ("Move 3 requests to", "Delete 3
requests"). Otherwise the right-click selects the target, as today.

- Open
- Duplicate `⇧⌘D`
- Close tab (only when the request is open)
- separator
- Copy URL (single request only)
- Copy as cURL (single request only)
- separator
- Authorization ▸ radio group: Inherit, No auth, Bearer, Basic. The radio
  value is the request's local mode; `undefined` is Inherit.
- Move to ▸ `GroupMenuTree`, root label "Ungrouped", current id = the
  request's group (single request only; for a selection, no current mark).
- separator
- Delete (destructive). Disabled while the request runs. A multi-selection
  delete asks for one confirmation.

### Browser group row and its `⋯` dropdown

The context menu and `GroupActionsMenu.vue` show the same items. The dropdown
uses the official `dropdown-menu` parts.

- New request
- New group
- separator
- Rename
- Settings…
- separator
- Collapse or Expand (one item; the label follows the state)
- Collapse all
- separator
- Move selection here (only when requests are selected and not all of them
  are already in this group)
- Move to ▸ `GroupMenuTree`, root label "Top level". It excludes the group
  and its descendants. It moves the group through the existing group-move
  path.
- separator
- Delete group (destructive)

### Ungrouped row

- New request
- Move selection here (same condition as for groups)

### Browser blank space

- New request
- New group
- Collapse all

### `GroupMenuTree` rules

- The first item is the root label, then a separator.
- A group with no visible children is an item.
- A group with visible children is a submenu. Its first item is "Move into
  ‹name›", then a separator, then its children.
- Children appear in the Browser order.
- `excludeIds` hides a group and all its descendants.
- The current location shows a check and is disabled.

### Request bar (`RequestWorkspace.vue`)

- Send `⌘↵` (disabled while busy or when the draft does not build)
- Focus URL `⌘L`
- separator
- Copy URL
- Copy as cURL
- Show cURL (checkbox item)

### cURL preview

- Copy cURL
- Close `Esc`

### Query and header table (`KeyValueEditor.vue`)

Table:

- Add row
- separator
- Enable all
- Disable all

Row:

- Enabled (checkbox item)
- Duplicate row
- separator
- Copy name
- Copy value
- separator
- Delete row (destructive)

All items are disabled while the request runs, as today.

### Body editor (`RequestEditor.vue`)

- Body type ▸ radio group: None, JSON, Text, GraphQL
- separator
- Format JSON or Format GraphQL (the label follows the body type)
- Clear body (destructive)

The CodeMirror text area keeps the native text menu (`@contextmenu.stop`).

### Authorization editor

- Radio group: Inherit, No auth, Bearer token, Basic auth
- separator
- Clear local override (disabled when there is no local override)

### Response toolbar (`ResponsePanel.vue`)

- Copy response
- Save response body…
- separator (body tab, text body only)
- Pretty (checkbox item, only when the body parses)
- Wrap lines (checkbox item)
- Find `⌘F`

### Response header row

- Copy name
- Copy value
- Copy name: value

### JSON tree row (`JsonTreeView.vue`)

- Copy path
- Copy value
- separator (container rows only)
- Expand or Collapse (one item; the label follows the state)
- Expand all
- Collapse all

## Keyboard and focus

- `Shift+F10` and the Context Menu key open the menu for the focused
  element. A shared handler on the trigger dispatches a `contextmenu` event
  at the center of the focused element. Reka then opens the menu there.
- Arrow keys, `Home`, `End`, type-ahead, `→` / `←` for submenus, `Enter`,
  and `Esc` come from Reka.
- When the menu closes, focus returns to the trigger.
- Global shortcuts stay off while a menu is open (`data-surface`).

## Error handling

- Clipboard actions use `useClipboard`. A failure shows the existing
  "Clipboard unavailable" message near the surface.
- "Copy as cURL" is disabled when the draft does not build. It never copies
  a partial command.
- Actions on a busy request follow the current rules: no delete, no edits.

## Testing

Unit tests (Vitest, Vue Test Utils):

- ui primitives: shortcut text, checkbox and radio state, sub-trigger
  chevron, destructive variant, `data-surface` on content and sub-content.
- `GroupMenuTree`: root item, nesting, exclusion of a group and its
  descendants, current-location check and disabled state.
- `closeTabs`: close others, close to the right, close all, active tab kept
  or moved.
- Tab "Duplicate" duplicates the target tab, not the active tab.
- Browser multi-selection labels and bulk actions.
- `shortcut()` output on macOS and on other platforms.
- `Shift+F10` opens the menu for the focused element.
- Update the existing menu tests for each surface. Keep each existing
  `data-testid` where its item stays.

Manual check: screenshots of the tab, Browser request, and Browser group
menus in the running app on port 1420, in the default theme and one Ghostty
theme.
