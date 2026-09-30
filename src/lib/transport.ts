import { Channel, invoke, isTauri } from '@tauri-apps/api/core'
import { isEventStream } from './sse'
import type { ApiResponse, RequestInput } from './request'
import { storeBlob } from './response-body'
import { FILES_UNAVAILABLE } from './request-files'
import { browserTiming } from './timing'
import {
  defaultTransportOptions,
  DOWNLOAD_LIMIT,
  MIB,
  type TransportOptions,
} from './transport-options'
export const nativeTransport = isTauri()

/** Parts of an event-stream response, sent while it arrives. */
export type StreamMessage =
  | {
      kind: 'head'
      status: number
      statusText: string
      headers: [string, string][]
    }
  | { kind: 'chunk'; text: string }

export type SendControls = {
  /** Abort cancels the request. */
  signal?: AbortSignal
  downloadLimit?: number
  /**
   * Receives a text/event-stream body as it arrives. The total timeout then
   * covers only the response headers.
   */
  onStream?: (message: StreamMessage) => void
}

// Per RFC 3629, the byte right after certain lead bytes is restricted beyond
// the usual continuation-byte range 80–BF. Outside that range the sequence is
// invalid (not merely incomplete), so from_utf8/TextDecoder reports it as an
// error over the lead byte alone.
function validSecondByte(lead: number, second: number) {
  if (lead === 0xe0) return second >= 0xa0 && second <= 0xbf
  if (lead === 0xed) return second >= 0x80 && second <= 0x9f
  if (lead === 0xf0) return second >= 0x90 && second <= 0xbf
  if (lead === 0xf4) return second >= 0x80 && second <= 0x8f
  return second >= 0x80 && second <= 0xbf
}

// Bytes of a UTF-8 character that the preview cut in two. 0 when the preview
// ends on a character boundary (or on an invalid sequence, which the decoder
// itself reports as binary).
function incompleteTail(bytes: Uint8Array) {
  for (let cut = 1; cut <= Math.min(3, bytes.length); cut++) {
    const byte = bytes[bytes.length - cut]
    if (byte >= 0x80 && byte <= 0xbf) continue
    // Not a lead byte: the decoder reports it as invalid.
    if (byte < 0xc2 || byte > 0xf4) return 0
    const length = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1
    if (length <= cut) return 0
    if (cut >= 2 && !validSecondByte(byte, bytes[bytes.length - cut + 1])) return 0
    return cut
  }
  return 0
}

/** Same rule as the desktop transport: NUL or invalid UTF-8 is binary. */
export function decodePreview(bytes: Uint8Array) {
  if (bytes.includes(0)) return { text: '', binary: true }
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(
      bytes.subarray(0, bytes.length - incompleteTail(bytes)),
    )
    return { text, binary: false }
  } catch {
    return { text: '', binary: true }
  }
}

export const CANCELED = 'Request canceled.'
let requestSequence = 0

export async function sendRequest(
  request: RequestInput,
  options: TransportOptions = defaultTransportOptions(),
  { signal, downloadLimit = DOWNLOAD_LIMIT, onStream }: SendControls = {},
): Promise<ApiResponse> {
  if (signal?.aborted) throw new Error(CANCELED)
  if (nativeTransport) {
    const requestId = `request-${++requestSequence}`
    const cancel = () => void invoke('cancel_request', { requestId })
    signal?.addEventListener('abort', cancel, { once: true })
    const onStreamChannel = new Channel<StreamMessage>()
    onStreamChannel.onmessage = (message) => onStream?.(message)
    try {
      return await invoke<ApiResponse>('send_request', {
        request,
        options,
        requestId,
        onStream: onStreamChannel,
      })
    } finally {
      signal?.removeEventListener('abort', cancel)
    }
  }
  if (request.bodyFile || request.multipart?.some((part) => part.file))
    throw new Error(FILES_UNAVAILABLE)
  const controller = new AbortController()
  let timedOut = false
  const timeout = setTimeout(() => {
    timedOut = true
    controller.abort()
  }, options.timeoutSeconds * 1000)
  const abort = () => controller.abort()
  signal?.addEventListener('abort', abort, { once: true })
  const start = performance.now()
  try {
    const headers = new Headers()
    request.headers.forEach(({ key, value }) => headers.append(key, value))
    let body: BodyInit | null = request.body
    if (request.multipart) {
      const form = new FormData()
      request.multipart.forEach(({ key, value }) => form.append(key, value))
      body = form
    }
    const result = await fetch(request.url, {
      method: request.method,
      headers,
      body,
      signal: controller.signal,
      credentials: 'omit',
      cache: 'no-store',
      redirect: options.followRedirects ? 'follow' : 'manual',
      referrerPolicy: 'no-referrer',
    })
    const headersAt = performance.now()
    const streaming = Boolean(onStream) && isEventStream(result.headers.get('content-type') ?? '')
    const decoder = new TextDecoder()
    if (streaming) {
      // A stream stays open; only cancel ends it.
      clearTimeout(timeout)
      onStream!({
        kind: 'head',
        status: result.status,
        statusText: result.statusText,
        headers: Array.from(result.headers),
      })
    }
    if (result.type === 'opaqueredirect')
      throw new Error('Browser preview cannot inspect redirects. Use the desktop app.')
    const previewLimit = options.inspectionLimitMiB * MIB
    const reader = result.body?.getReader()
    let sizeBytes = 0
    const chunks: Uint8Array[] = []
    if (reader) {
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        sizeBytes += value.byteLength
        if (streaming)
          onStream!({
            kind: 'chunk',
            text: decoder.decode(value, { stream: true }),
          })
        if (sizeBytes > downloadLimit) {
          await reader.cancel()
          throw new Error('Response exceeds the 1 GiB download limit.')
        }
        chunks.push(value)
      }
    }
    const preview = new Uint8Array(Math.min(sizeBytes, previewLimit))
    let offset = 0
    for (const chunk of chunks) {
      if (offset >= preview.length) break
      const part = chunk.subarray(0, preview.length - offset)
      preview.set(part, offset)
      offset += part.byteLength
    }
    const { text, binary } = decodePreview(preview)
    const end = performance.now()
    const entries =
      typeof performance.getEntriesByName === 'function'
        ? (performance.getEntriesByName(result.url, 'resource') as PerformanceResourceTiming[])
        : []
    const entry = [...entries].reverse().find((candidate) => candidate.startTime >= start)
    const contentType = result.headers.get('content-type') ?? ''
    return {
      status: result.status,
      statusText: result.statusText,
      durationMs: Math.round(end - start),
      timing: browserTiming(entry, {
        waitMs: headersAt - start,
        downloadMs: end - headersAt,
      }),
      sizeBytes,
      headers: Array.from(result.headers, ([key, value]) => ({ key, value })),
      body: text,
      bodyId: storeBlob(new Blob(chunks as BlobPart[], { type: contentType })),
      truncated: sizeBytes > preview.length,
      binary,
      ...(result.redirected ? { finalUrl: result.url } : {}),
    }
  } catch (error) {
    if (timedOut) throw new Error(`Request timed out after ${options.timeoutSeconds} seconds.`)
    if (controller.signal.aborted) throw new Error(CANCELED)
    throw error
  } finally {
    clearTimeout(timeout)
    signal?.removeEventListener('abort', abort)
  }
}
