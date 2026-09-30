import type { RequestGroup } from './groups'
import { displayMethod, sessionLabel, type LabelTokens, type RequestSession } from './session'

export type RequestMatch = {
  id: number
  method: string
  label: string
  url: string
  groupPath: string
}

/** Group names from the root to `groupId`, joined with " / ". */
export function groupPath(groups: RequestGroup[], groupId: number | null) {
  const byId = new Map(groups.map((group) => [group.id, group]))
  const names: string[] = []
  const seen = new Set<number>()
  let group = groupId === null ? undefined : byId.get(groupId)
  while (group && !seen.has(group.id)) {
    seen.add(group.id)
    names.unshift(group.name)
    group = group.parentId === null ? undefined : byId.get(group.parentId)
  }
  return names.join(' / ')
}

export type FuzzyMatch = { score: number; indices: number[] }
const boundary = (text: string, index: number) =>
  index === 0 ||
  !/[a-z0-9]/i.test(text[index - 1]) ||
  (/[a-z]/.test(text[index - 1]) && /[A-Z]/.test(text[index]))

/**
 * Match `pattern` in `text` without case. An exact substring scores highest,
 * more so at a word start. Otherwise the characters must appear in order;
 * consecutive and word-start characters score more. Null when no match.
 */
export function fuzzyMatch(text: string, pattern: string): FuzzyMatch | null {
  const haystack = text.toLowerCase()
  const needle = pattern.toLowerCase()
  if (!needle) return { score: 0, indices: [] }
  let best = -1
  for (let at = haystack.indexOf(needle); at >= 0; at = haystack.indexOf(needle, at + 1)) {
    if (best < 0) best = at
    if (boundary(text, at)) {
      best = at
      break
    }
  }
  if (best >= 0)
    return {
      score: 1000 + (boundary(text, best) ? 100 : 0),
      indices: Array.from({ length: needle.length }, (_, i) => best + i),
    }
  const indices: number[] = []
  let score = 0
  let from = 0
  for (const char of needle) {
    const at = haystack.indexOf(char, from)
    if (at < 0) return null
    score += 1
    if (indices.length && at === indices[indices.length - 1] + 1) score += 5
    if (boundary(text, at)) score += 8
    indices.push(at)
    from = at + 1
  }
  return { score, indices }
}

const words = (query: string) => query.trim().toLowerCase().split(/\s+/).filter(Boolean)

export type RankedRequestMatch = RequestMatch & { labelIndices: number[] }

/**
 * Requests where every word of `query` matches the label or group path
 * (fuzzy), or the URL or method (substring). Best matches first.
 */
export function matchRequests(
  sessions: RequestSession[],
  groups: RequestGroup[],
  query: string,
  globalDefinitions: Record<string, string> = {},
): RankedRequestMatch[] {
  const tokens: LabelTokens = { groups, globalDefinitions }
  const needles = words(query)
  const ranked: { match: RankedRequestMatch; score: number; order: number }[] = []
  sessions.forEach((session, order) => {
    const match: RankedRequestMatch = {
      id: session.id,
      method: displayMethod(session),
      label: sessionLabel(session, tokens),
      url: session.draft.url,
      groupPath: groupPath(groups, session.groupId),
      labelIndices: [],
    }
    let score = 0
    const indices = new Set<number>()
    for (const needle of needles) {
      const label = fuzzyMatch(match.label, needle)
      const group = fuzzyMatch(match.groupPath, needle)
      const exact = [match.url, match.method].some((text) => text.toLowerCase().includes(needle))
        ? 900
        : -1
      const best = Math.max(label?.score ?? -1, group?.score ?? -1, exact)
      if (best < 0) return
      score += best
      if (label && label.score === best) label.indices.forEach((index) => indices.add(index))
    }
    match.labelIndices = [...indices].sort((a, b) => a - b)
    ranked.push({ match, score, order })
  })
  return ranked.sort((a, b) => b.score - a.score || a.order - b.order).map(({ match }) => match)
}

/** An app action in the command center. `>` in the search switches to these. */
export type Command = {
  id: string
  label: string
  /** Keys for `shortcutLabel`, such as `["mod", "\\"]`. */
  shortcut?: string[]
  disabled?: boolean
}

export const COMMAND_PREFIX = '>'
export const isCommandQuery = (query: string) => query.startsWith(COMMAND_PREFIX)

export type RankedCommand = Command & { indices: number[] }

/** Commands whose label fuzzy-matches every word of `query`. Best first. */
export function matchCommands(commands: Command[], query: string): RankedCommand[] {
  const needles = words(query.slice(isCommandQuery(query) ? COMMAND_PREFIX.length : 0))
  const ranked: { command: RankedCommand; score: number; order: number }[] = []
  commands.forEach((command, order) => {
    let score = 0
    const indices = new Set<number>()
    for (const needle of needles) {
      const match = fuzzyMatch(command.label, needle)
      if (!match) return
      score += match.score
      match.indices.forEach((index) => indices.add(index))
    }
    ranked.push({
      command: { ...command, indices: [...indices].sort((a, b) => a - b) },
      score,
      order,
    })
  })
  return ranked.sort((a, b) => b.score - a.score || a.order - b.order).map(({ command }) => command)
}

/** Split `text` into runs, marking the characters at `indices`. */
export function highlightRuns(text: string, indices: number[]) {
  const marked = new Set(indices)
  const runs: { text: string; match: boolean }[] = []
  for (let i = 0; i < text.length; i++) {
    const match = marked.has(i)
    const last = runs[runs.length - 1]
    if (last && last.match === match) last.text += text[i]
    else runs.push({ text: text[i], match })
  }
  return runs
}
