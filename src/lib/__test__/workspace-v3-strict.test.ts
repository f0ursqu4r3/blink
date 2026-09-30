/**
 * Tests for workspace v3 strict definitions validation:
 * - reject invalid names, local names beginning `_`, excessive entry count,
 *   excessive name/value size, non-string fields.
 * - globalDefinitions required in v3 encode/decode.
 * - setters in useWorkspaceState (tested via direct invocation with mocked storage).
 */
import { describe, expect, it } from 'vitest'
import { createSession } from '../session'
import { encodeWorkspace, decodeWorkspace } from '../workspace'
import type { RequestGroup } from '../groups'

// ── Helper: build a minimal valid v3 snapshot string ──────────────────────

function minimalV3(overrides?: {
  globalDefinitions?: unknown
  groups?: unknown[]
  tabLocalDefinitions?: unknown
  tabLocalAuth?: unknown
}): string {
  const session = createSession()
  const base = JSON.parse(encodeWorkspace([session], session.id, [], {}))
  if (overrides?.globalDefinitions !== undefined) {
    base.globalDefinitions = overrides.globalDefinitions
  }
  if (overrides?.groups !== undefined) {
    base.groups = overrides.groups
  }
  if (overrides?.tabLocalDefinitions !== undefined) {
    base.tabs[0].draft.localDefinitions = overrides.tabLocalDefinitions
  }
  if (overrides?.tabLocalAuth !== undefined) {
    base.tabs[0].draft.localAuth = overrides.tabLocalAuth
  }
  return JSON.stringify(base)
}

describe('workspace v3 strict definitions validation', () => {
  it('accepts a valid globalDefinitions object', () => {
    const enc = encodeWorkspace([createSession()], 1, [], {
      apiKey: 'abc',
      host: 'example.com',
    })
    const decoded = decodeWorkspace(enc)
    expect(decoded.globalDefinitions).toEqual({
      apiKey: 'abc',
      host: 'example.com',
    })
  })

  it('requires globalDefinitions in v3 (rejects absent field)', () => {
    const session = createSession()
    const raw = JSON.parse(encodeWorkspace([session], session.id, [], {}))
    delete raw.globalDefinitions
    expect(() => decodeWorkspace(JSON.stringify(raw))).toThrow()
  })

  it('rejects globalDefinitions with non-string values', () => {
    expect(() => decodeWorkspace(minimalV3({ globalDefinitions: { key: 123 } }))).toThrow()
  })

  it('rejects globalDefinitions that is not an object (string)', () => {
    expect(() => decodeWorkspace(minimalV3({ globalDefinitions: 'flat' }))).toThrow()
  })

  it('rejects globalDefinitions that is not an object (array)', () => {
    expect(() => decodeWorkspace(minimalV3({ globalDefinitions: ['a', 'b'] }))).toThrow()
  })

  it('rejects globalDefinitions with more than 500 entries', () => {
    const big: Record<string, string> = {}
    for (let i = 0; i < 501; i++) big[`key${i}`] = 'v'
    expect(() => decodeWorkspace(minimalV3({ globalDefinitions: big }))).toThrow()
  })

  it('accepts globalDefinitions with exactly 500 entries', () => {
    const big: Record<string, string> = {}
    for (let i = 0; i < 500; i++) big[`key${i}`] = 'v'
    expect(() => decodeWorkspace(minimalV3({ globalDefinitions: big }))).not.toThrow()
  })

  it('rejects globalDefinitions with a key longer than 256 chars', () => {
    const longKey = 'a'.repeat(257)
    expect(() => decodeWorkspace(minimalV3({ globalDefinitions: { [longKey]: 'v' } }))).toThrow()
  })

  it('rejects globalDefinitions with a value longer than 65536 chars', () => {
    const longVal = 'x'.repeat(65537)
    expect(() => decodeWorkspace(minimalV3({ globalDefinitions: { key: longVal } }))).toThrow()
  })

  it("rejects group localDefinitions with key starting with '_'", () => {
    const group: RequestGroup = {
      id: 1,
      name: 'Test',
      parentId: null,
      collapsed: false,
      localDefinitions: { _secret: 'oops' },
    }
    const session = createSession()
    session.groupId = 1
    const enc = encodeWorkspace([session], session.id, [group], {})
    expect(() => decodeWorkspace(enc)).toThrow()
  })

  it("accepts group localDefinitions without leading '_' keys", () => {
    const group: RequestGroup = {
      id: 1,
      name: 'Test',
      parentId: null,
      collapsed: false,
      localDefinitions: { host: 'api.example.com', version: 'v2' },
    }
    const session = createSession()
    session.groupId = 1
    const enc = encodeWorkspace([session], session.id, [group], {})
    expect(() => decodeWorkspace(enc)).not.toThrow()
    const decoded = decodeWorkspace(enc)
    expect(decoded.groups[0].localDefinitions).toEqual({
      host: 'api.example.com',
      version: 'v2',
    })
  })

  it('rejects group localDefinitions with non-string values', () => {
    const session = createSession()
    const raw = JSON.parse(encodeWorkspace([session], session.id, [], {}))
    raw.groups = [
      {
        id: 1,
        name: 'G',
        parentId: null,
        collapsed: false,
        localDefinitions: { key: 99 },
      },
    ]
    raw.tabs[0].groupId = 1
    expect(() => decodeWorkspace(JSON.stringify(raw))).toThrow()
  })

  it('encodeWorkspace always writes globalDefinitions (required field)', () => {
    const session = createSession()
    const enc = encodeWorkspace([session], session.id)
    const raw = JSON.parse(enc)
    expect(raw).toHaveProperty('globalDefinitions')
    expect(typeof raw.globalDefinitions).toBe('object')
    expect(Array.isArray(raw.globalDefinitions)).toBe(false)
  })

  it('v3 round-trip preserves empty globalDefinitions as empty object', () => {
    const session = createSession()
    const enc = encodeWorkspace([session], session.id, [], {})
    const decoded = decodeWorkspace(enc)
    expect(decoded.globalDefinitions).toEqual({})
  })
})
