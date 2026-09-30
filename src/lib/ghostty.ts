// Parser for Ghostty colour configuration (`key = value` lines), ported
// from term0. Only colour keys are read; other keys are ignored, so a full
// Ghostty config or a theme file both work.

export type Palette = {
  background: string
  foreground: string
  cursor: string
  cursorText: string
  selectionBackground: string
  selectionForeground: string
  /** ANSI colours 0 to 15, as lowercase `#rrggbb`. */
  ansi: string[]
}

export type ParseResult = { ok: true; palette: Palette } | { ok: false; error: string }

const keys: Record<string, Exclude<keyof Palette, 'ansi'>> = {
  background: 'background',
  foreground: 'foreground',
  'cursor-color': 'cursor',
  'cursor-text': 'cursorText',
  'selection-background': 'selectionBackground',
  'selection-foreground': 'selectionForeground',
}

const ansiCount = 16

/** `#rgb`, `#rrggbb`, `rgb` or `rrggbb` as lowercase `#rrggbb`. */
export function hexColour(value: string): string | null {
  const hex = value.trim().replace(/^#/, '').toLowerCase()
  if (/^[0-9a-f]{6}$/.test(hex)) return `#${hex}`
  if (/^[0-9a-f]{3}$/.test(hex)) return `#${[...hex].map((digit) => digit + digit).join('')}`
  return null
}

function unquote(value: string): string {
  const trimmed = value.trim()
  const quoted = trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"')
  return quoted ? trimmed.slice(1, -1) : trimmed
}

/**
 * Parse `text` over `base`: keys the text sets replace the base values.
 * Text with no colour key, or with any colour that is not a hex value, is
 * rejected as a whole.
 */
export function parseGhostty(text: string, base: Palette): ParseResult {
  const palette: Palette = { ...base, ansi: [...base.ansi] }
  let found = 0
  for (const [index, raw] of text.split(/\r?\n/).entries()) {
    const line = raw.trim()
    if (line === '' || line.startsWith('#')) continue
    const equals = line.indexOf('=')
    if (equals < 0) continue
    const key = line.slice(0, equals).trim()
    const value = unquote(line.slice(equals + 1))
    const where = `line ${index + 1}`
    if (key === 'palette') {
      const match = /^(\d+)\s*=\s*(.+)$/.exec(value)
      const slot = match ? Number(match[1]) : -1
      if (!match || slot >= ansiCount)
        return {
          ok: false,
          error: `${where}: expected palette = 0..15=#rrggbb`,
        }
      const colour = hexColour(match[2] ?? '')
      if (!colour)
        return {
          ok: false,
          error: `${where}: palette ${slot} is not a hex colour`,
        }
      palette.ansi[slot] = colour
      found += 1
      continue
    }
    const field = keys[key]
    if (!field) continue
    const colour = hexColour(value)
    if (!colour) return { ok: false, error: `${where}: ${key} is not a hex colour` }
    palette[field] = colour
    found += 1
  }
  if (found === 0) return { ok: false, error: 'no colour keys found' }
  return { ok: true, palette }
}
