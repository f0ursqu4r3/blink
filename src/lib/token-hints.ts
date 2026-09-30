import { interpolate, type InterpolationContext } from './interpolation'

/**
 * Editor hints for `{{name}}`, `{{_.name}}` and `{{!NAME}}` references.
 * Hints show the value a reference resolves to.
 */

export type TokenOption = {
  /** Text between the braces, such as `host` or `_.host`. */
  name: string
  scope: 'local' | 'global'
  /** The resolved value, or the raw value if it cannot resolve. */
  value: string
}

export type TokenSpan = {
  text: string
  /** Name inside the braces, for `{{name}}` references. */
  name?: string
  /** Set for a token reference. `env` values resolve only when sending. */
  token?: 'resolved' | 'unresolved' | 'env'
}

const REFERENCE_RE = /\{\{!([^{}]*)\}\}|\{\{(_\.)?([^{}]+?)\}\}/g

/**
 * The value `{{name}}` resolves to, with nested references resolved.
 * Returns undefined for an unknown name.
 */
export function tokenValue(name: string, ctx?: InterpolationContext): string | undefined {
  if (!ctx) return undefined
  const workspaceOnly = name.startsWith('_.')
  const key = workspaceOnly ? name.slice(2) : name
  if (!isResolved(key, workspaceOnly, ctx)) return undefined
  try {
    return interpolate(`{{${name}}}`, ctx)
  } catch {
    // A cycle or a missing nested token: show the raw value.
    return workspaceOnly || !Object.prototype.hasOwnProperty.call(ctx.definitions, key)
      ? ctx.workspaceDefinitions?.[key]
      : ctx.definitions[key]
  }
}

/** Every name a field can reference, local names first. */
export function tokenOptions(ctx?: InterpolationContext): TokenOption[] {
  if (!ctx) return []
  const local = Object.keys(ctx.definitions)
  const global = Object.keys(ctx.workspaceDefinitions ?? {})
  const option = (name: string, scope: TokenOption['scope']) => ({
    name,
    scope,
    value: tokenValue(name, ctx) ?? '',
  })
  return [
    ...local.map((name) => option(name, 'local')),
    // A bare global name resolves when no local token has the same name.
    ...global.filter((name) => !local.includes(name)).map((name) => option(name, 'global')),
    ...global.map((name) => option(`_.${name}`, 'global')),
  ]
}

function isResolved(name: string, workspaceOnly: boolean, ctx?: InterpolationContext) {
  const has = (record: Record<string, string> | undefined) =>
    !!record && Object.prototype.hasOwnProperty.call(record, name)
  return workspaceOnly
    ? has(ctx?.workspaceDefinitions)
    : has(ctx?.definitions) || has(ctx?.workspaceDefinitions)
}

/** Split `text` into plain runs and token references. */
export function tokenSpans(text: string, ctx?: InterpolationContext): TokenSpan[] {
  const spans: TokenSpan[] = []
  let last = 0
  for (const match of text.matchAll(REFERENCE_RE)) {
    const start = match.index
    if (start > last) spans.push({ text: text.slice(last, start) })
    const env = match[1] !== undefined
    const token = env
      ? 'env'
      : isResolved(match[3].trim(), match[2] === '_.', ctx)
        ? 'resolved'
        : 'unresolved'
    spans.push({
      text: match[0],
      token,
      ...(env ? {} : { name: `${match[2] ?? ''}${match[3].trim()}` }),
    })
    last = start + match[0].length
  }
  if (last < text.length) spans.push({ text: text.slice(last) })
  return spans
}

/** Token ranges in `text`, for editors that decorate by offset. */
export function tokenRanges(text: string, ctx?: InterpolationContext) {
  let offset = 0
  return tokenSpans(text, ctx).flatMap((span) => {
    const from = offset
    offset += span.text.length
    return span.token ? [{ from, to: offset, token: span.token, name: span.name }] : []
  })
}

/**
 * `text` with each defined `{{name}}` replaced by its value. Undefined
 * references and `{{!NAME}}` stay as typed.
 */
export function resolveForDisplay(text: string, ctx?: InterpolationContext) {
  return tokenSpans(text, ctx)
    .map((span) => (span.name ? (tokenValue(span.name, ctx) ?? span.text) : span.text))
    .join('')
}

/** One-line hint for a reference, such as `endpoint = users`. */
export function tokenHint(span: TokenSpan, ctx?: InterpolationContext) {
  if (span.token === 'env') return `${span.text} reads the environment when sending`
  if (!span.name) return ''
  const value = tokenValue(span.name, ctx)
  return value === undefined ? `${span.name} is not defined` : `${span.name} = ${value}`
}

/**
 * The open `{{` reference before `cursor`, if the user is typing one.
 * `from` is the offset just after `{{`.
 */
export function tokenQueryAt(text: string, cursor: number): { from: number; query: string } | null {
  const match = /\{\{([^{}\s]*)$/.exec(text.slice(0, cursor))
  if (!match) return null
  return { from: cursor - match[1].length, query: match[1] }
}

/** Options that match `query`, prefix matches first. */
export function matchTokenOptions(options: TokenOption[], query: string) {
  const q = query.toLowerCase()
  const matches = options.filter((o) => o.name.toLowerCase().includes(q))
  return [
    ...matches.filter((o) => o.name.toLowerCase().startsWith(q)),
    ...matches.filter((o) => !o.name.toLowerCase().startsWith(q)),
  ]
}

/**
 * Insert `name` for the reference that starts at `from`. Replaces the typed
 * query and any `}}` already after the cursor.
 */
export function applyTokenOption(
  text: string,
  from: number,
  cursor: number,
  name: string,
): { text: string; cursor: number } {
  const closing = text.startsWith('}}', cursor) ? 2 : 0
  const insert = `${name}}}`
  return {
    text: text.slice(0, from) + insert + text.slice(cursor + closing),
    cursor: from + insert.length,
  }
}
