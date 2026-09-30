export type DiffLine = {
  kind: 'same' | 'add' | 'remove'
  text: string
  /** 1-based line numbers in the old and new text. */
  before?: number
  after?: number
}

/** Beyond this many changes the diff stops; callers show a notice. */
export const DIFF_EDIT_LIMIT = 4000

/**
 * Line diff with the Myers O(ND) algorithm. Null when the texts differ by
 * more than `limit` lines.
 */
export function diffLines(
  before: string[],
  after: string[],
  limit = DIFF_EDIT_LIMIT,
): DiffLine[] | null {
  const n = before.length
  const m = after.length
  const max = Math.min(n + m, limit)
  const offset = max + 1
  const v = new Int32Array(2 * max + 3)
  const trace: Int32Array[] = []
  let found = n === 0 && m === 0
  for (let d = 0; d <= max && !found; d++) {
    trace.push(v.slice())
    for (let k = -d; k <= d; k += 2) {
      let x =
        k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1])
          ? v[offset + k + 1]
          : v[offset + k - 1] + 1
      let y = x - k
      while (x < n && y < m && before[x] === after[y]) {
        x++
        y++
      }
      v[offset + k] = x
      if (x >= n && y >= m) {
        found = true
        break
      }
    }
  }
  if (!found) return null
  // Walk the trace back from the end.
  const lines: DiffLine[] = []
  let x = n
  let y = m
  for (let d = trace.length - 1; d >= 0; d--) {
    const row = trace[d]
    const k = x - y
    const previousK =
      k === -d || (k !== d && row[offset + k - 1] < row[offset + k + 1]) ? k + 1 : k - 1
    const previousX = d === 0 ? 0 : row[offset + previousK]
    const previousY = previousX - previousK
    while (x > previousX && y > previousY) {
      lines.push({ kind: 'same', text: before[x - 1], before: x, after: y })
      x--
      y--
    }
    if (d === 0) break
    if (x === previousX) lines.push({ kind: 'add', text: after[y - 1], after: y })
    else lines.push({ kind: 'remove', text: before[x - 1], before: x })
    x = previousX
    y = previousY
  }
  return lines.reverse()
}

export type DiffHunk = { lines: DiffLine[] } | { hidden: number }

/** Keep `context` unchanged lines around each change; fold the rest. */
export function foldDiff(lines: DiffLine[], context = 3): DiffHunk[] {
  const keep = new Uint8Array(lines.length)
  lines.forEach((line, index) => {
    if (line.kind === 'same') return
    for (
      let i = Math.max(0, index - context);
      i <= Math.min(lines.length - 1, index + context);
      i++
    )
      keep[i] = 1
  })
  const hunks: DiffHunk[] = []
  let hidden = 0
  lines.forEach((line, index) => {
    if (!keep[index]) {
      hidden++
      return
    }
    if (hidden) hunks.push({ hidden })
    hidden = 0
    const last = hunks[hunks.length - 1]
    if (last && 'lines' in last) last.lines.push(line)
    else hunks.push({ lines: [line] })
  })
  if (hidden) hunks.push({ hidden })
  return hunks
}
