/** A parse error position. `line` and `column` are 1-based; `offset` is 0-based. */
export type TextLocation = {
  line: number
  column: number
  offset: number
  reason: string
}

export function locationFromOffset(text: string, offset: number, reason: string): TextLocation {
  const clamped = Math.max(0, Math.min(offset, text.length))
  const before = text.slice(0, clamped)
  const line = before.split('\n').length
  const column = clamped - (before.lastIndexOf('\n') + 1) + 1
  return { line, column, offset: clamped, reason }
}

export function locationFromLineColumn(
  text: string,
  line: number,
  column: number,
  reason: string,
): TextLocation {
  const lines = text.split('\n')
  let offset = 0
  for (let i = 0; i < line - 1 && i < lines.length; i++) offset += lines[i].length + 1
  return locationFromOffset(text, offset + column - 1, reason)
}

export const describeLocation = (location: TextLocation) =>
  `line ${location.line}, column ${location.column}`
