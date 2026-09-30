import { pair, type Pair } from './request'

/** Show token definitions as key-value rows, with one blank row to type in. */
export function definitionsToRows(definitions: Record<string, string>): Pair[] {
  const rows = Object.entries(definitions).map(([key, value]) => pair(key, value))
  return rows.length ? rows : [pair()]
}

/**
 * Collect key-value rows into token definitions. Blank rows are skipped.
 * Returns an error message when a row is not a valid token.
 */
export function rowsToDefinitions(
  rows: Pair[],
): { definitions: Record<string, string> } | { error: string } {
  const definitions: Record<string, string> = {}
  for (const row of rows) {
    const name = row.key.trim()
    if (!name && !row.value) continue
    if (!name) return { error: 'Enter a name for each token.' }
    if (name.startsWith('_')) return { error: 'Token names must not start with _.' }
    if (name.startsWith('!')) return { error: 'Token names must not start with !.' }
    if (/[{}]/.test(name)) return { error: 'Token names must not contain { or }.' }
    if (Object.prototype.hasOwnProperty.call(definitions, name))
      return { error: `Token "${name}" is defined more than once.` }
    definitions[name] = row.value
  }
  return { definitions }
}
