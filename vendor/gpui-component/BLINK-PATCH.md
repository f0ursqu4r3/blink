# Blink patch to gpui-component 0.7.0

This directory is `gpui-component` 0.7.0 from crates.io (git
`0c830f4d257e69fdd17200650533ab4ca9a40cc0`, `crates/component`). The root
`Cargo.toml` uses it through `[patch.crates-io]`. It has two patches (menu
metrics and dialog placement, below). The only other change is that
`.cargo-ok` is removed.

## Menu metrics

### Why

Blink's menus must match the Vue app (`src/components/ui/menu-classes.ts`):
12 px text, 24 px rows, a check gutter on every row, and a 180 px minimum
width. Upstream `PopupMenu` hardcodes 14 px text (`text_sm`), 26 px rows, an
8 px inset, a 2 px row gap, a 2 px separator, and `rems(8.)` minimum width.
Its `Size` field has no setter, and `PopupMenuItem::element` rows keep the
26 px minimum height, so no public API can set these values.

### What changed

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

## Dialog placement

### Why

The Vue dialogs (`ApplicationSettingsDialog.vue`, `GroupSettingsDialog.vue`,
`CookiesDialog.vue`) are `top-1/2 -translate-y-1/2` with `max-h-[90dvh]`
(`80dvh` for Cookies): centered, at most that fraction of the window high,
with a scrolling body between a visible header and footer. Upstream `Dialog`
always anchors its top edge at `margin_top` (a tenth of the viewport by
default), and it sets `max_h` after the app style, so an app `max_h` has no
effect. A tall dialog therefore snaps to the top margin.

### What changed

Only `src/dialog/dialog.rs`.

1. New `DialogProps::centered` (default `false`) and public
   `Dialog::centered(bool)`. When it is set, the popup anchors its
   `LeftCenter` corner at the viewport middle (plus the stacked-layer
   offset), so the positioner centers it at any height. `margin_top` is
   ignored. The entrance animation slides it down its last 16 px instead of
   from the top of the window.
2. In `render`, a definite `max_h` from the app style (for example
   `relative(0.9)`, a fraction of the viewport height) now also caps the
   computed `max_height`. The upstream edge-margin cap still applies.

Without `centered(true)` or an app `max_h`, every value is the upstream one,
so upstream behavior is the default. Blink calls
`.centered(true).max_h(relative(0.9))` (Cookies: `0.8`) in
`crates/blink/src/ui/settings_dialog.rs`, `ui/group_settings.rs`, and
`ui/cookies_dialog.rs`. The headless test
`ui::settings_dialog::ui_tests::dialogs_are_centered_and_capped` measures the
three dialogs.

## Updating

Copy the new upstream release over this directory, then apply the diff of the
three files above again (`diff -u` against the registry copy shows it).
