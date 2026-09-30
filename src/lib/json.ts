import { parse, stringify } from 'lossless-json'
import { locationFromOffset, type TextLocation } from './text-location'

export const JSON_HIGHLIGHT_LIMIT = 64_000

/** Keep large IDs and decimal values exact when inspecting or formatting JSON. */
export function formatJson(text: string): string {
  return stringify(parse(text), null, 2) ?? text
}

/** lossless-json reports errors as "<reason> at position <offset>". */
export function jsonErrorLocation(text: string, error: unknown): TextLocation | null {
  if (!(error instanceof Error)) return null
  const match = /^(.*) at position (\d+)$/s.exec(error.message)
  return match ? locationFromOffset(text, Number(match[2]), match[1]) : null
}
