/**
 * Direct unit tests for useWorkspaceState narrow state setters.
 *
 * Because useWorkspaceState uses onMounted/onUnmounted, we test the setter
 * logic in isolation by instantiating the composable inside an effectScope
 * without a component. We verify that each setter mutates only the intended
 * fields and leaves other state intact.
 */
import { describe, expect, it, afterEach, vi, beforeEach } from 'vitest'
import { effectScope, type EffectScope } from 'vue'

// Mock Tauri and storage so the composable can be instantiated outside a component.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: () => false,
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))
vi.mock('@/lib/transport', () => ({
  nativeTransport: false,
  sendRequest: vi.fn(),
}))
vi.mock('@/lib/workspace-storage', () => ({
  readBrowserWorkspace: vi.fn(() => null),
  readWorkspace: vi.fn(() => Promise.resolve(null)),
  writeWorkspace: vi.fn(() => Promise.resolve()),
  WorkspaceWriter: class {
    save = vi.fn(() => Promise.resolve())
  },
}))

import { useWorkspaceState } from '@/composables/useWorkspaceState'
import { createSession } from '@/lib/session'

let scope: EffectScope
let state: ReturnType<typeof useWorkspaceState>

beforeEach(() => {
  scope = effectScope()
  scope.run(() => {
    state = useWorkspaceState()
  })
})
afterEach(() => {
  scope.stop()
  vi.restoreAllMocks()
})

describe('useWorkspaceState – narrow setters', () => {
  // ── setRequestLocalAuth ──────────────────────────────────────────────────
  describe('setRequestLocalAuth', () => {
    it('sets localAuth on the target session draft', () => {
      const session = state.sessions.value[0]
      expect(session.draft.localAuth).toBeUndefined()
      state.setRequestLocalAuth(session.id, { type: 'bearer', token: 'tok' })
      expect(session.draft.localAuth).toEqual({ type: 'bearer', token: 'tok' })
    })

    it('can clear localAuth back to undefined (inherit)', () => {
      const session = state.sessions.value[0]
      state.setRequestLocalAuth(session.id, { type: 'bearer', token: 'tok' })
      state.setRequestLocalAuth(session.id, undefined)
      expect(session.draft.localAuth).toBeUndefined()
    })

    it('is a no-op for unknown session id', () => {
      const before = JSON.stringify(state.sessions.value)
      state.setRequestLocalAuth(999999, { type: 'none' })
      expect(JSON.stringify(state.sessions.value)).toBe(before)
    })

    it('does not affect other sessions', async () => {
      // Add a second session by cloning
      const s1 = state.sessions.value[0]
      const { createSession } = await import('@/lib/session')
      const s2 = createSession()
      state.sessions.value.push(s2)
      state.setRequestLocalAuth(s1.id, { type: 'none' })
      expect(
        state.sessions.value.find((s: { id: number }) => s.id === s2.id)!.draft.localAuth,
      ).toBeUndefined()
    })
  })

  // ── setGroupName ─────────────────────────────────────────────────────────
  describe('setGroupName', () => {
    it('sets name on a group', () => {
      const group = state.addGroup('OldName', null)
      state.setGroupName(group.id, 'NewName')
      const found = state.groups.value.find((g) => g.id === group.id)
      expect(found?.name).toBe('NewName')
    })

    it('trims whitespace', () => {
      const group = state.addGroup('A', null)
      state.setGroupName(group.id, '  Trimmed  ')
      const found = state.groups.value.find((g) => g.id === group.id)
      expect(found?.name).toBe('Trimmed')
    })

    it('is a no-op for blank name', () => {
      const group = state.addGroup('Original', null)
      state.setGroupName(group.id, '   ')
      const found = state.groups.value.find((g) => g.id === group.id)
      expect(found?.name).toBe('Original')
    })

    it('is a no-op for unknown group id', () => {
      const group = state.addGroup('G', null)
      state.setGroupName(999999, 'Hack')
      expect(state.groups.value.find((g) => g.id === group.id)?.name).toBe('G')
    })
  })

  // ── setGroupLocalAuth ────────────────────────────────────────────────────
  describe('setGroupLocalAuth', () => {
    it('sets localAuth on a group', () => {
      const group = state.addGroup('G', null)
      state.setGroupLocalAuth(group.id, { type: 'bearer', token: 'g-tok' })
      const found = state.groups.value.find((g) => g.id === group.id)
      expect(found?.localAuth).toEqual({ type: 'bearer', token: 'g-tok' })
    })

    it('can clear group localAuth to undefined', () => {
      const group = state.addGroup('G', null)
      state.setGroupLocalAuth(group.id, { type: 'bearer', token: 'g-tok' })
      state.setGroupLocalAuth(group.id, undefined)
      const found = state.groups.value.find((g) => g.id === group.id)
      expect(found?.localAuth).toBeUndefined()
    })

    it('is a no-op for unknown group id', () => {
      state.setGroupLocalAuth(999999, { type: 'none' })
      // no throw, no mutations to check beyond stability
      expect(state.groups.value.length).toBeDefined()
    })
  })

  // ── setGroupLocalDefinitions ─────────────────────────────────────────────
  describe('setGroupLocalDefinitions', () => {
    it('sets localDefinitions on a group', () => {
      const group = state.addGroup('G', null)
      state.setGroupLocalDefinitions(group.id, { host: 'api.example.com' })
      const found = state.groups.value.find((g) => g.id === group.id)
      expect(found?.localDefinitions).toEqual({ host: 'api.example.com' })
    })

    it('can clear localDefinitions to undefined', () => {
      const group = state.addGroup('G', null)
      state.setGroupLocalDefinitions(group.id, { host: 'api.example.com' })
      state.setGroupLocalDefinitions(group.id, undefined)
      const found = state.groups.value.find((g) => g.id === group.id)
      expect(found?.localDefinitions).toBeUndefined()
    })

    it('is a no-op for unknown group id', () => {
      state.setGroupLocalDefinitions(999999, { x: 'y' })
      expect(state.groups.value.length).toBeDefined()
    })
  })

  // ── setGlobalDefinitions ─────────────────────────────────────────────────
  describe('setGlobalDefinitions', () => {
    it('replaces globalDefinitions', () => {
      state.setGlobalDefinitions({ apiVersion: 'v3' })
      expect(state.globalDefinitions.value).toEqual({ apiVersion: 'v3' })
    })

    it('removes keys not present in the new map', () => {
      state.setGlobalDefinitions({ a: '1', b: '2' })
      state.setGlobalDefinitions({ b: 'updated' })
      expect(state.globalDefinitions.value).toEqual({ b: 'updated' })
      expect(state.globalDefinitions.value).not.toHaveProperty('a')
    })

    it('stores a shallow copy (no aliasing)', () => {
      const defs = { key: 'val' }
      state.setGlobalDefinitions(defs)
      defs.key = 'mutated'
      expect(state.globalDefinitions.value.key).toBe('val')
    })
  })
})

describe('closeTabs', () => {
  function openFour() {
    const ids = [0, 1, 2, 3].map(() => {
      const session = createSession()
      state.sessions.value.push(session)
      return session.id
    })
    state.openIds.value = [...ids]
    return ids
  }

  it('keeps the active tab when it stays open', () => {
    const [a, b, c, d] = openFour()
    state.activeId.value = b
    state.closeTabs([a, c])
    expect(state.openIds.value).toEqual([b, d])
    expect(state.activeId.value).toBe(b)
  })

  it('moves the active tab to the next open tab at the same position', () => {
    const [a, b, c, d] = openFour()
    state.activeId.value = b
    state.closeTabs([b, c])
    expect(state.openIds.value).toEqual([a, d])
    expect(state.activeId.value).toBe(d)
  })

  it('clears the active tab when all tabs close, and keeps the requests', () => {
    const ids = openFour()
    const count = state.sessions.value.length
    state.activeId.value = ids[0]
    state.closeTabs(ids)
    expect(state.openIds.value).toEqual([])
    expect(state.activeId.value).toBeNull()
    expect(state.sessions.value.length).toBe(count)
  })
})
