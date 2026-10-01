# Blink patch to gpui-component 0.7.0

This directory is `gpui-component` 0.7.0 from crates.io (git
`0c830f4d257e69fdd17200650533ab4ca9a40cc0`, `crates/component`). The root
`Cargo.toml` uses it through `[patch.crates-io]`. The only other change is
that `.cargo-ok` is removed.

## Why

Blink's menus must match the Vue app (`src/components/ui/menu-classes.ts`):
12 px text, 24 px rows, a check gutter on every row, and a 180 px minimum
width. Upstream `PopupMenu` hardcodes 14 px text (`text_sm`), 26 px rows, an
8 px inset, a 2 px row gap, a 2 px separator, and `rems(8.)` minimum width.
Its `Size` field has no setter, and `PopupMenuItem::element` rows keep the
26 px minimum height, so no public API can set these values.

## What changed

Only `src/menu/popup_menu.rs` and `src/menu/mod.rs`.

1. New public `PopupMenuMetrics` struct (a `gpui::Global`), exported from
   `menu`. It holds the item text size, label text size, row height, check
   column position (`indicator_left`) and text gap (`indicator_gap`), right
   padding, row gap, default minimum width, separator height, and separator
   margin.
2. `PopupMenu::render` reads the global with `cx.try_global`. When it is set:
   - every row shows the check or icon column (as the Vue check gutter);
   - the items container uses `item_gap` and the `min_width` default (a
     menu's own `min_w` still wins).
3. `PopupMenu::render_item` uses the metrics when set:
   - text size, row height (items, element items, submenu triggers, and
     labels), left and right padding;
   - the icon-to-text gap (new private helper `icon_gap`);
   - the separator margin and thickness;
   - label text size.

Without the global, every value is the upstream one, so upstream behavior is
the default. Blink sets the global in `crates/blink/src/theme.rs`
(`theme::menu_metrics`, called from `theme::apply`).

## Updating

Copy the new upstream release over this directory, then apply the diff of the
two files above again (`diff -u` against the registry copy shows it).
