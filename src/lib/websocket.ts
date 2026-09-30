import { Channel, invoke, isTauri } from '@tauri-apps/api/core'
import { buildRequest, type Draft, type Header } from './request'
import type { ResolvedRequestContext } from './authorization'
import { interpolate } from './interpolation'

export const isWebSocketUrl = (url: string) => /^wss?:\/\//i.test(url.trim())

export type SocketEvent =
  | { kind: 'open'; status: number; headers: [string, string][] }
  | { kind: 'message'; text: string; binary: boolean; size: number }
  | { kind: 'close'; code: number | null; reason: string }
  | { kind: 'error'; message: string }

export type SocketMessage = {
  id: number
  direction: 'in' | 'out' | 'system'
  text: string
  /** Epoch milliseconds. */
  at: number
  binary?: boolean
  size: number
}
export type SocketState = 'connecting' | 'open' | 'closing' | 'closed'
export type SocketSession = {
  state: SocketState
  messages: SocketMessage[]
  /** The upgrade response headers. */
  headers: Header[]
}
/** The log keeps at most this many messages; older ones drop. */
export const SOCKET_MESSAGE_LIMIT = 5000

/**
 * The URL and headers to connect with, resolved as a send resolves them:
 * tokens, query rows, headers, and auth. Throws a message for the user.
 */
export function buildWebSocketRequest(
  draft: Draft,
  ctx?: ResolvedRequestContext,
): { url: string; headers: Header[] } {
  const raw = ctx ? interpolate(draft.url.trim(), ctx) : draft.url.trim()
  if (!isWebSocketUrl(raw)) throw new Error('Enter a ws:// or wss:// URL.')
  // Build as an HTTP GET, which applies every rule, then restore the scheme.
  const request = buildRequest(
    {
      ...draft,
      url: raw.replace(/^ws/i, 'http'),
      method: 'GET',
      bodyMode: 'none',
    },
    ctx,
  )
  return {
    url: request.url.replace(/^http/i, 'ws'),
    headers: request.headers.filter(
      ({ key }) => !['accept', 'content-type'].includes(key.toLowerCase()),
    ),
  }
}

export type SocketHandle = {
  send(text: string): Promise<void>
  close(): void
}

let connectionSequence = 0

/** Open a connection. The desktop app sends headers; a browser cannot. */
export function openSocket(
  url: string,
  headers: Header[],
  connectTimeoutSeconds: number,
  onEvent: (event: SocketEvent) => void,
): SocketHandle {
  if (isTauri()) {
    const connectionId = `socket-${++connectionSequence}`
    const channel = new Channel<SocketEvent>()
    channel.onmessage = onEvent
    void invoke('ws_connect', {
      connectionId,
      url,
      headers,
      connectTimeoutSeconds,
      onEvent: channel,
    }).catch((error) =>
      onEvent({
        kind: 'error',
        message: error instanceof Error ? error.message : String(error),
      }),
    )
    return {
      send: (text) => invoke('ws_send', { connectionId, text }),
      close: () => void invoke('ws_close', { connectionId }).catch(() => {}),
    }
  }
  let socket: WebSocket
  try {
    socket = new WebSocket(url)
  } catch (error) {
    queueMicrotask(() =>
      onEvent({
        kind: 'error',
        message: error instanceof Error ? error.message : String(error),
      }),
    )
    return { send: async () => {}, close: () => {} }
  }
  socket.binaryType = 'arraybuffer'
  let failed = false
  socket.onopen = () => onEvent({ kind: 'open', status: 101, headers: [] })
  socket.onmessage = (message) => {
    if (typeof message.data === 'string')
      onEvent({
        kind: 'message',
        text: message.data,
        binary: false,
        size: message.data.length,
      })
    else {
      const bytes = new Uint8Array(message.data as ArrayBuffer)
      const preview = Array.from(bytes.slice(0, 64), (b) => b.toString(16).padStart(2, '0')).join(
        ' ',
      )
      onEvent({
        kind: 'message',
        text: bytes.length > 64 ? `${preview} …` : preview,
        binary: true,
        size: bytes.length,
      })
    }
  }
  socket.onerror = () => {
    failed = true
    onEvent({ kind: 'error', message: 'Connection failed.' })
  }
  socket.onclose = (event) => {
    if (!failed) onEvent({ kind: 'close', code: event.code, reason: event.reason })
  }
  return {
    async send(text) {
      if (socket.readyState !== WebSocket.OPEN) throw new Error('Not connected.')
      socket.send(text)
    },
    close: () => socket.close(1000),
  }
}
