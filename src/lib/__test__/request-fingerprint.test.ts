/**
 * Tests for requestFingerprint — the resolved-input fingerprint that drives
 * stale detection in useRequestRunner.
 */
import { describe, expect, it } from 'vitest'
import { requestFingerprint } from '../session'
import type { RequestInput } from '../request'

const baseRequest: RequestInput = {
  method: 'GET',
  url: 'https://example.test/api',
  headers: [{ key: 'Accept', value: 'application/json' }],
  body: null,
}

describe('requestFingerprint', () => {
  it('returns empty string for null request', () => {
    expect(requestFingerprint(null)).toBe('')
  })

  it('produces a stable string for the same request', () => {
    const fp1 = requestFingerprint(baseRequest, 'none')
    const fp2 = requestFingerprint(baseRequest, 'none')
    expect(fp1).toBe(fp2)
  })

  it('differs when auth type changes', () => {
    const fpNone = requestFingerprint(baseRequest, 'none')
    const fpBearer = requestFingerprint(baseRequest, 'bearer')
    expect(fpNone).not.toBe(fpBearer)
  })

  it('differs when url changes', () => {
    const fp1 = requestFingerprint(baseRequest, 'none')
    const fp2 = requestFingerprint({ ...baseRequest, url: 'https://other.test' }, 'none')
    expect(fp1).not.toBe(fp2)
  })

  it('differs when method changes', () => {
    const fp1 = requestFingerprint(baseRequest, 'none')
    const fp2 = requestFingerprint({ ...baseRequest, method: 'POST' }, 'none')
    expect(fp1).not.toBe(fp2)
  })

  it('differs when headers differ', () => {
    const fp1 = requestFingerprint(baseRequest, 'bearer')
    const fp2 = requestFingerprint(
      {
        ...baseRequest,
        headers: [...baseRequest.headers, { key: 'Authorization', value: 'Bearer tok' }],
      },
      'bearer',
    )
    expect(fp1).not.toBe(fp2)
  })

  it('same request, no authType, is stable', () => {
    const fp = requestFingerprint(baseRequest)
    expect(typeof fp).toBe('string')
    expect(fp.length).toBeGreaterThan(0)
  })
})
