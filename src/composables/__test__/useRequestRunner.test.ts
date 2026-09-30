/**
 * Tests for useRequestRunner reactive context source.
 *
 * Uses effectScope + computed to simulate the reactive setup the UI does.
 * Exercises: MaybeRefOrGetter (ref, computed, getter), stale fingerprint
 * derived from resolved RequestInput (not raw draftFingerprint), and
 * backward-compatible no-context usage.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { ref, reactive, computed, effectScope, nextTick, type EffectScope } from 'vue'
import { useRequestRunner } from '@/composables/useRequestRunner'
import { createSession, requestFingerprint } from '@/lib/session'
import type { ResolvedRequestContext } from '@/lib/authorization'
import { sendRequest } from '@/lib/transport'
import { releaseResponse } from '@/lib/response-body'
import { defaultTransportOptions } from '@/lib/transport-options'

// Mock transport so send() doesn't make real network calls
vi.mock('@/lib/transport', () => ({
  CANCELED: 'Request canceled.',
  sendRequest: vi.fn(() =>
    Promise.resolve({
      status: 200,
      statusText: 'OK',
      durationMs: 5,
      sizeBytes: 2,
      headers: [],
      body: '{}',
    }),
  ),
  nativeTransport: false,
}))
vi.mock('@/lib/response-body', () => ({ releaseResponse: vi.fn() }))

let scope: EffectScope
beforeEach(() => {
  scope = effectScope()
})
afterEach(() => {
  scope.stop()
  vi.restoreAllMocks()
})

function makeCtx(authType: 'none' | 'bearer' = 'none'): ResolvedRequestContext {
  return {
    auth: authType === 'bearer' ? { type: 'bearer', token: 'tok' } : { type: 'none' },
    definitions: {},
    workspaceDefinitions: {},
  }
}

describe('useRequestRunner – reactive context source', () => {
  it('works without context source (backward compat)', () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session)
    })
    expect(runner!.prepared.value.error).toBe('')
    expect(runner!.prepared.value.request).not.toBeNull()
  })

  it('accepts a plain Ref as context source and tracks it', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    const ctxRef = ref<ResolvedRequestContext | undefined>(makeCtx('none'))
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef)
    })
    expect(runner!.prepared.value.request).not.toBeNull()
    // Swap to bearer context
    ctxRef.value = makeCtx('bearer')
    await nextTick()
    // prepared should now have picked up bearer auth
    const req = runner!.prepared.value.request
    expect(req?.headers.some((h) => h.key === 'Authorization')).toBe(true)
  })

  it('accepts a ComputedRef as context source', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    const authType = ref<'none' | 'bearer'>('none')
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      const ctxComputed = computed<ResolvedRequestContext>(() => makeCtx(authType.value))
      runner = useRequestRunner(session, ctxComputed)
    })
    expect(runner!.prepared.value.request?.headers.some((h) => h.key === 'Authorization')).toBe(
      false,
    )
    authType.value = 'bearer'
    await nextTick()
    expect(runner!.prepared.value.request?.headers.some((h) => h.key === 'Authorization')).toBe(
      true,
    )
  })

  it('accepts a getter function as context source', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    const ctxRef = ref<ResolvedRequestContext>(makeCtx('none'))
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session, () => ctxRef.value)
    })
    expect(runner!.curl.value).not.toContain('Authorization')
    ctxRef.value = makeCtx('bearer')
    await nextTick()
    expect(runner!.curl.value).toContain('Authorization')
  })

  it('stale uses resolved fingerprint: group auth change marks stale', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    const ctxRef = ref<ResolvedRequestContext>(makeCtx('none'))
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef)
    })

    // Simulate what send() stores
    const req = runner!.prepared.value.request!
    session.sentFingerprint = requestFingerprint(req, 'none')
    session.response = {
      status: 200,
      statusText: 'OK',
      durationMs: 5,
      sizeBytes: 2,
      headers: [],
      body: '{}',
    }

    expect(runner!.stale.value).toBe(false)

    // Change group auth → effective bearer auth
    ctxRef.value = makeCtx('bearer')
    await nextTick()

    // stale should now be true because resolved request now has Authorization header
    expect(runner!.stale.value).toBe(true)
  })

  it('stale is false when response matches current resolved fingerprint', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    const ctxRef = ref<ResolvedRequestContext>(makeCtx('none'))
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef)
    })

    const req = runner!.prepared.value.request!
    session.sentFingerprint = requestFingerprint(req, 'none')
    session.response = {
      status: 200,
      statusText: 'OK',
      durationMs: 5,
      sizeBytes: 2,
      headers: [],
      body: '{}',
    }

    expect(runner!.stale.value).toBe(false)
  })

  it('stale is false with no response', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session, () => makeCtx('none'))
    })
    expect(runner!.stale.value).toBe(false)
  })

  it('send() stores requestFingerprint (resolved) not draftFingerprint', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test'
    const ctxRef = ref<ResolvedRequestContext>(makeCtx('bearer'))
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef)
    })

    await runner!.send()
    // sentFingerprint must contain the Authorization header from resolved bearer
    expect(session.sentFingerprint).toContain('Authorization')
    // And must not equal just the draft fingerprint (which has no Authorization)
    const { draftFingerprint } = await import('@/lib/session')
    expect(session.sentFingerprint).not.toBe(draftFingerprint(session.draft))
  })
})

describe('sentUrl', () => {
  it('captures the resolved request URL when a send starts', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test/{{path}}'
    const ctxRef = ref<ResolvedRequestContext>({
      auth: { type: 'none' },
      definitions: { path: 'widgets' },
      workspaceDefinitions: {},
    })
    let runner: ReturnType<typeof useRequestRunner>
    scope.run(() => {
      runner = useRequestRunner(session, ctxRef)
    })
    expect(runner!.sentUrl.value).toBe('')
    await runner!.send()
    expect(runner!.sentUrl.value).toBe(runner!.prepared.value.request!.url)
    expect(runner!.sentUrl.value).toBe('https://example.test/widgets')
  })
})

describe('transport options and body release', () => {
  it('sends with the given options and releases the previous body', async () => {
    const session = createSession()
    session.draft.url = 'https://example.test/'
    const previous = {
      status: 200,
      statusText: 'OK',
      durationMs: 1,
      sizeBytes: 0,
      headers: [],
      body: '',
      bodyId: 'old',
    }
    session.response = previous
    const options = { ...defaultTransportOptions(), timeoutSeconds: 7 }
    const { send } = scope.run(() => useRequestRunner(session, undefined, () => options))!
    await send()
    expect(vi.mocked(releaseResponse)).toHaveBeenCalledWith(previous)
    const calls = vi.mocked(sendRequest).mock.calls
    expect(calls[calls.length - 1][1]).toEqual(options)
  })

  it('releases a result that arrives after the scope stops', async () => {
    let resolve!: (value: unknown) => void
    vi.mocked(sendRequest).mockImplementationOnce(
      () => new Promise((done) => (resolve = done)) as never,
    )
    const session = createSession()
    session.draft.url = 'https://example.test/'
    const local = effectScope()
    const { send } = local.run(() => useRequestRunner(session))!
    const pending = send()
    local.stop()
    const late = {
      status: 200,
      statusText: 'OK',
      durationMs: 1,
      sizeBytes: 0,
      headers: [],
      body: '',
      bodyId: 'late',
    }
    resolve(late)
    await pending
    expect(vi.mocked(releaseResponse)).toHaveBeenCalledWith(late)
    expect(session.response).toBeNull()
  })

  it('cancel aborts the running send and reports it', async () => {
    vi.mocked(sendRequest).mockImplementationOnce(
      (_request, _options, controls) =>
        new Promise((_done, fail) =>
          controls?.signal?.addEventListener('abort', () => fail(new Error('Request canceled.'))),
        ),
    )
    const session = createSession()
    session.draft.url = 'https://example.test/'
    const { send, cancel } = scope.run(() => useRequestRunner(session))!
    const pending = send()
    expect(session.busy).toBe(true)
    cancel()
    await pending
    expect(session.busy).toBe(false)
    expect(session.error).toBe('Request canceled.')
  })

  it('keeps the sent request when the draft changes during a send', async () => {
    let resolve!: (value: unknown) => void
    vi.mocked(sendRequest).mockImplementationOnce(
      () => new Promise((done) => (resolve = done)) as never,
    )
    const session = reactive(createSession())
    session.draft.url = 'https://example.test/a'
    const { send, stale } = scope.run(() => useRequestRunner(session))!
    const pending = send()
    session.draft.url = 'https://example.test/b'
    resolve({
      status: 200,
      statusText: 'OK',
      durationMs: 1,
      sizeBytes: 0,
      headers: [],
      body: '',
    })
    await pending
    const calls = vi.mocked(sendRequest).mock.calls
    expect(calls[calls.length - 1][0].url).toBe('https://example.test/a')
    expect(stale.value).toBe(true)
  })
})

describe('history', () => {
  it('records each send, newest first, but not a cancel', async () => {
    const session = reactive(createSession())
    session.draft.url = 'https://example.test/a'
    const scope = effectScope()
    const runner = scope.run(() => useRequestRunner(session))!
    await runner.send()
    await runner.send()
    expect(session.history?.map((entry) => entry.id)).toEqual([2, 1])
    expect(session.history?.[0]).toMatchObject({
      method: 'GET',
      url: 'https://example.test/a',
      status: 200,
    })
    vi.mocked(sendRequest).mockRejectedValueOnce(new Error('Request canceled.'))
    await runner.send()
    vi.mocked(sendRequest).mockRejectedValueOnce(new Error('Offline'))
    await runner.send()
    expect(session.history?.map((entry) => entry.error)).toEqual(['Offline', undefined, undefined])
    scope.stop()
  })
})

describe('checks', () => {
  it('runs assertions and passes captures to the hook', async () => {
    const { createAssertion, createCapture } = await import('@/lib/checks')
    const session = reactive(createSession())
    session.draft.url = 'https://example.test/a'
    session.draft.assertions = [
      createAssertion('status', 'equals', '200'),
      createAssertion('json', 'equals', 'abc', '.token'),
    ]
    session.draft.captures = [createCapture('token', 'json', '.token')]
    vi.mocked(sendRequest).mockResolvedValueOnce({
      status: 200,
      statusText: 'OK',
      durationMs: 5,
      sizeBytes: 15,
      headers: [],
      body: '{"token":"abc"}',
    })
    const captured: Record<string, string>[] = []
    const scope = effectScope()
    const runner = scope.run(() =>
      useRequestRunner(session, undefined, undefined, {
        onCapture: (values) => captured.push(values),
      }),
    )!
    await runner.send()
    expect(session.testResults?.map((result) => result.pass)).toEqual([true, true])
    expect(captured).toEqual([{ token: 'abc' }])
    session.draft.assertions[0].expected = '201'
    await runner.recheck()
    expect(session.testResults?.[0].pass).toBe(false)
    scope.stop()
  })
})

describe('event streams', () => {
  it('shows live events and keeps them when canceled', async () => {
    const session = reactive(createSession())
    session.draft.url = 'https://example.test/sse'
    vi.mocked(sendRequest).mockImplementationOnce(
      (_request, _options, controls) =>
        new Promise((_resolve, reject) => {
          controls?.onStream?.({
            kind: 'head',
            status: 200,
            statusText: 'OK',
            headers: [['content-type', 'text/event-stream']],
          })
          controls?.onStream?.({ kind: 'chunk', text: 'data: 1\n\nda' })
          controls?.onStream?.({ kind: 'chunk', text: 'ta: 2\n\n' })
          controls?.signal?.addEventListener('abort', () => reject(new Error('Request canceled.')))
        }),
    )
    const scope = effectScope()
    const runner = scope.run(() => useRequestRunner(session))!
    const sending = runner.send()
    await nextTick()
    expect(session.stream?.events.map((event) => event.data)).toEqual(['1', '2'])
    runner.cancel()
    await sending
    expect(session.error).toBe('')
    expect(session.stream).toBeUndefined()
    expect(session.response).toMatchObject({
      status: 200,
      body: 'data: 1\n\ndata: 2\n\n',
      sizeBytes: 18,
    })
    expect(session.history?.[0].status).toBe(200)
    scope.stop()
  })
})
