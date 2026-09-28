# Blink Design

## Scene

A developer is comparing HTTP exchanges across temporary request tabs. Blink
feels like an industrial shipboard terminal: dark, compact, precise, and quiet.
The visual reference is Weyland-Yutani equipment, not a branded replica. Avoid fake
telemetry, decorative warnings, scanline overlays, and cinematic animations.

## Visual system

- **Color strategy:** Default: near-black warm surfaces, thin graphite
  dividers, off-white text, and an industrial amber accent. Use the accent
  sparingly: active tab, focus rings and focused borders, primary buttons,
  checkboxes, the resize handle, sending/progress indicators, and the
  close-confirm bar border. Everything else uses neutral or semantic tokens.
  A Ghostty palette can replace the default: surfaces mix
  foreground into background, the accent comes from a chosen palette slot
  (default 4, blue), and semantic colors come from the palette: success slot
  2, warning slot 3, errors slot 1, info slot 4, keyword slot 5. HTTP method
  colors: GET success, POST warning, PUT/PATCH info, DELETE error,
  HEAD/OPTIONS keyword. Components use tokens only; never hardcode colors.
- **Typography:** Local system fonts only. Monospace for URL, headers, body,
  metrics, and compact uppercase section labels. System sans-serif for tabs
  and explanatory text. No web font requests.
- **Layout:** A 40-pixel title bar holds the Browser toggle right of the
  traffic lights, the centered command center (`Cmd/Ctrl+P`), and the
  Settings cog at the right; it doubles as the window drag area, and on
  macOS the traffic lights sit on its left. The Browser card and the editor
  card, which holds the request tab strip and the resizable request and
  response split, sit on a darker frame with 6-pixel gaps. A 24-pixel status
  bar closes the window. Below 900 pixels, panels stack without hiding core
  request controls. Below 760 pixels, the Browser toggle opens the Browser
  as an overlay.
- **Shape:** 8-pixel radius for cards, popovers, menus, and dialogs; 4-pixel
  radius for controls and tooltips.
- **Density:** Small icon-and-label controls. No floating toolbars, large
  buttons, repeated summaries, or redundant metadata badges.
- **Motion:** Short opacity and color transitions only. Respect reduced motion.

## Interaction rules

- Send is available only for valid HTTP(S) request data.
- Treat duplicate query and header rows as intentional; preserve their order.
- Query rows append to the URL. Do not silently rewrite the URL field.
- Validate JSON before sending; preserve large numeric values when formatting.
- Keep errors close to the request bar or response panel. Never label a
  completed HTTP error response as a transport failure.
- Disable request edits while sending. Ask before discarding a draft.
- Give each request tab its own draft, response, error, timer, and editor state.
  Keep inactive panels mounted but hidden, with unique input and panel IDs.
- Show endpoint paths before hosts so requests to the same API remain distinct.
  Do not include credentials, query strings, or fragments in tab labels.
- Preserve background sends across tab switches. Never write a completed
  response to whichever tab happens to be active.
- Duplicate draft values, not object references. Do not duplicate responses.
- The title bar toggle can hide the Browser; keep it visible by default. Groups
  can nest. Creating or duplicating a request places it in the active
  request's group. Deleting a group moves its requests to the parent and
  promotes direct child groups.
- Keep the add-tab action visible when the tab strip overflows. Support
  keyboard selection, protected closing, and a usable final blank tab.
- Mark responses as previous when their request draft changes after sending.
- Keep body/header tabs keyboard-accessible with visible focus indicators.
- Render response content as text. Never execute or embed response HTML.
- Restore the current open tabs, drafts, responses, and view settings locally.
  Never replay interrupted requests or build an implicit request history.
- Make saving and storage failures visible. Preserve corrupt snapshots until
  the user explicitly replaces them. Disclose local plaintext credentials.
- Explain browser limitations; use native HTTP for unrestricted inspection.
- Put secondary guidance in keyboard and touch accessible tooltips. Keep errors,
  effective authorization, and destructive confirmations visible.
- Apply application and group defaults only when creating a new request. Do not
  rewrite an existing request or a duplicate.
