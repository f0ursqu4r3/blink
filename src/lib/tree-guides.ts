/** Tree lines for one row of an indented list. */
export type TreeGuide = {
  /** Ancestor depths whose line passes through this row. */
  through: number[]
  /** The row's own elbow: "mid" (├) or "last" (└). */
  elbow: 'mid' | 'last'
}

/**
 * Guides for rows given their levels, in display order. Level 0 rows have
 * none. A depth `d` line connects the children of a row at level `d - 1`.
 */
export function treeGuides(levels: number[]): (TreeGuide | null)[] {
  // continues[d] for row i: a later row at depth d comes before any row
  // shallower than d. Scan from the end.
  const result: (TreeGuide | null)[] = new Array(levels.length).fill(null)
  let open = new Set<number>()
  for (let i = levels.length - 1; i >= 0; i--) {
    const level = levels[i]
    if (level > 0)
      result[i] = {
        through: [...open].filter((depth) => depth < level).sort((a, b) => a - b),
        elbow: open.has(level) ? 'mid' : 'last',
      }
    // Rows above see this row: depths deeper than it end here.
    open = new Set([...open].filter((depth) => depth < level))
    if (level > 0) open.add(level)
  }
  return result
}
