# Blink Design

## Scene

A developer is comparing HTTP exchanges across temporary request tabs. Blink
feels like an industrial shipboard terminal: dark, compact, precise, and quiet.
The visual reference is Weyland-Yutani equipment, not a branded replica. Avoid fake
telemetry, decorative warnings, scanline overlays, and cinematic animations.

## Visual system

- **Color strategy:** Near-black warm surfaces, thin graphite dividers,
  off-white text, and industrial amber for primary actions and active tabs.
  Reserve muted green for successful responses and coral for errors.
- **Typography:** Local system fonts only. Monospace for URL, headers, body,
  metrics, and compact uppercase section labels. System sans-serif for tabs
  and explanatory text. No web font requests.
- **Layout:** A compact 36-pixel request tab strip and 34-pixel URL control
  sit beside a 244-pixel Browser sidebar. The Browser lists ungrouped requests
  and nested groups. Request and response panels share the remaining width in
  a resizable split. Below 900 pixels, panels stack without hiding core request
  controls. Below 760 pixels, the Browser opens from the header as an overlay.
- **Shape:** Mostly square surfaces. Use a 2-pixel radius for controls.
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
- Keep the Browser sidebar visible. Groups can nest. Creating or duplicating a
  request places it in the active request's group. Deleting a group moves its
  requests to the parent and promotes direct child groups.
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
