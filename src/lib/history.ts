import type { ApiResponse, Header, RequestInput, ResponseTiming } from './request'
import { formatJson } from './json'

/** One send of a request: its response or its error. */
export type HistoryEntry = {
  id: number
  /** Epoch milliseconds. */
  sentAt: number
  method: string
  url: string
  status?: number
  statusText?: string
  error?: string
  durationMs: number
  sizeBytes: number
  headers: Header[]
  /** Empty when `bodyOmitted`. */
  body: string
  /** The body was binary, truncated, or over the history limit. */
  bodyOmitted?: boolean
  timing?: ResponseTiming
}

export const HISTORY_LIMIT = 25
/** History keeps bodies up to this size, so the workspace stays small. */
export const HISTORY_BODY_LIMIT = 64 * 1024

export function historyEntry(
  id: number,
  sentAt: number,
  request: RequestInput,
  outcome: { response: ApiResponse } | { error: string; durationMs: number },
): HistoryEntry {
  const base = { id, sentAt, method: request.method, url: request.url }
  if ('error' in outcome)
    return {
      ...base,
      error: outcome.error,
      durationMs: outcome.durationMs,
      sizeBytes: 0,
      headers: [],
      body: '',
    }
  const { response } = outcome
  const omit =
    Boolean(response.truncated || response.binary) || response.body.length > HISTORY_BODY_LIMIT
  return {
    ...base,
    status: response.status,
    statusText: response.statusText,
    durationMs: response.durationMs,
    sizeBytes: response.sizeBytes,
    headers: response.headers.map(({ key, value }) => ({ key, value })),
    body: omit ? '' : response.body,
    ...(omit ? { bodyOmitted: true } : {}),
    ...(response.timing ? { timing: { ...response.timing } } : {}),
  }
}

/** The newest entry first, capped at HISTORY_LIMIT. */
export function addHistory(history: HistoryEntry[] | undefined, entry: HistoryEntry) {
  return [entry, ...(history ?? [])].slice(0, HISTORY_LIMIT)
}

export function nextHistoryId(history: HistoryEntry[] | undefined) {
  return (history ?? []).reduce((max, entry) => Math.max(max, entry.id), 0) + 1
}

/** Body lines for a diff: formatted JSON when the body parses. */
export function diffText(entry: HistoryEntry) {
  if (entry.error) return [`Error: ${entry.error}`]
  if (entry.bodyOmitted) return ['(body not kept)']
  try {
    return formatJson(entry.body).split('\n')
  } catch {
    return entry.body.split('\n')
  }
}

/** Header lines, sorted by name, for a diff. */
export const headerLines = (entry: HistoryEntry) =>
  entry.headers.map(({ key, value }) => `${key.toLowerCase()}: ${value}`).sort()

export function relativeTime(sentAt: number, now = Date.now()) {
  const seconds = Math.max(0, Math.round((now - sentAt) / 1000))
  if (seconds < 45) return 'just now'
  const minutes = Math.round(seconds / 60)
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  return new Date(sentAt).toLocaleDateString()
}
