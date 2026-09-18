# Blink Design

## Scene

A developer is at a desktop workstation during a debugging session. The app
stays quiet, bright, and legible beside code and logs. The request editor is
the visual focus. Response facts remain visible without competing for
attention.

## Visual system

- **Color strategy:** Restrained, with warm graphite neutrals and a single
  cyan-blue accent for active controls and positive response state.
- **Typography:** System UI for interface copy. A local monospace stack for
  URLs, methods, headers, and response bodies.
- **Layout:** Fixed left rail, request workspace, and response inspector. At
  narrow widths, the rail collapses and panels stack.
- **Motion:** Short opacity and color transitions only. Respect reduced motion.

## Interaction rules

- Send is available only for a valid absolute URL.
- Errors stay close to the response panel.
- No request history persists between launches in this first version.
