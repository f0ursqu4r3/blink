import type { InterpolationContext } from './interpolation'
import { tokenSpans, tokenValue, type TokenSpan } from './token-hints'

/**
 * Show a field with each defined `{{name}}` replaced by its value, and map
 * edits on the shown text back to the raw text.
 *
 * A shown value acts as one unit: the caret stays at its edges, and an edit
 * that touches part of it replaces the whole reference. Backspace at its end
 * (or Delete at its start) removes one raw character, so `users` turns back
 * into `{{endpoint}` and the user can edit the reference.
 */

export type DisplaySegment = {
  span: TokenSpan
  /** Shown text: the value for a unit, else the raw text. */
  text: string
  /** True when the segment shows a value in place of its reference. */
  unit: boolean
  from: number
  to: number
  rawFrom: number
  rawTo: number
}

export type TokenDisplay = { text: string; segments: DisplaySegment[] }

export function tokenDisplay(raw: string, ctx?: InterpolationContext): TokenDisplay {
  const segments: DisplaySegment[] = []
  let from = 0
  let rawFrom = 0
  for (const span of tokenSpans(raw, ctx)) {
    const value = span.token === 'resolved' && span.name ? tokenValue(span.name, ctx) : undefined
    // An empty value would leave nothing to see or edit.
    const unit = !!value
    const text = unit ? value! : span.text
    segments.push({
      span,
      text,
      unit,
      from,
      to: from + text.length,
      rawFrom,
      rawTo: rawFrom + span.text.length,
    })
    from += text.length
    rawFrom += span.text.length
  }
  return { text: segments.map((s) => s.text).join(''), segments }
}

/** Raw offset for a shown offset. A unit's inside maps to its raw end. */
export function rawOffset(segments: DisplaySegment[], pos: number): number {
  for (const s of segments) {
    if (pos < s.from || pos > s.to) continue
    if (!s.unit) return s.rawFrom + (pos - s.from)
    return pos === s.from ? s.rawFrom : s.rawTo
  }
  return segments[segments.length - 1]?.rawTo ?? 0
}

/** Shown offset for a raw offset. A unit's inside maps to its shown end. */
export function displayOffset(segments: DisplaySegment[], raw: number): number {
  for (const s of segments) {
    if (raw < s.rawFrom || raw > s.rawTo) continue
    if (!s.unit) return s.from + (raw - s.rawFrom)
    return raw === s.rawFrom ? s.from : s.to
  }
  return segments[segments.length - 1]?.to ?? 0
}

const unitAround = (segments: DisplaySegment[], pos: number) =>
  segments.find((s) => s.unit && s.from < pos && pos < s.to)

/**
 * Move a caret out of a unit, to the edge in the direction it moved.
 * Returns `pos` when it is not inside a unit.
 */
export function snapCaret(segments: DisplaySegment[], pos: number, previous: number): number {
  const s = unitAround(segments, pos)
  if (!s) return pos
  if (previous <= s.from) return s.to
  if (previous >= s.to) return s.from
  return pos - s.from < s.to - pos ? s.from : s.to
}

/** Widen a shown range so it covers every unit it touches. */
export function widenRange(
  segments: DisplaySegment[],
  start: number,
  end: number,
): [number, number] {
  for (const s of segments) {
    if (!s.unit || s.from >= end || s.to <= start) continue
    start = Math.min(start, s.from)
    end = Math.max(end, s.to)
  }
  return [start, end]
}

export type RawEdit = { raw: string; caret: number }

/**
 * Apply a native edit, found by comparing the shown text before and after,
 * to the raw text. `caret` is the shown caret after the edit.
 */
export function applyDisplayEdit(
  raw: string,
  before: TokenDisplay,
  after: string,
  caret: number,
): RawEdit {
  const old = before.text
  const suffix = Math.min(after.length - caret, old.length)
  let start = 0
  const limit = Math.min(caret, old.length - suffix)
  while (start < limit && old[start] === after[start]) start++
  const oldEnd = old.length - suffix
  const inserted = after.slice(start, caret)

  let [from, to] = widenRange(before.segments, start, oldEnd)
  // Text typed inside a unit goes after it.
  const inside = from === to && unitAround(before.segments, from)
  if (inside) from = to = inside.to

  const rawFrom = rawOffset(before.segments, from)
  const rawTo = rawOffset(before.segments, to)
  return {
    raw: raw.slice(0, rawFrom) + inserted + raw.slice(rawTo),
    caret: rawFrom + inserted.length,
  }
}

/**
 * Backspace just after a unit, or Delete just before one: remove one raw
 * character so the reference shows again. Returns null for other positions.
 */
export function deleteIntoUnit(
  raw: string,
  segments: DisplaySegment[],
  pos: number,
  direction: 'backward' | 'forward',
): RawEdit | null {
  const s = segments.find(
    (seg) => seg.unit && (direction === 'backward' ? seg.to : seg.from) === pos,
  )
  if (!s) return null
  const at = direction === 'backward' ? s.rawTo - 1 : s.rawFrom
  return { raw: raw.slice(0, at) + raw.slice(at + 1), caret: at }
}

/** Raw text for a shown selection, with touched units copied whole. */
export function rawSlice(
  raw: string,
  segments: DisplaySegment[],
  start: number,
  end: number,
): string {
  const [from, to] = widenRange(segments, start, end)
  return raw.slice(rawOffset(segments, from), rawOffset(segments, to))
}
