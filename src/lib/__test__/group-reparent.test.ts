import { describe, expect, it } from 'vitest'
import { canNestGroup } from '../groups'

describe('group reparenting', () => {
  it('rejects self and descendant parents', () => {
    const groups = [
      { id: 1, name: 'A', parentId: null, collapsed: false },
      { id: 2, name: 'B', parentId: 1, collapsed: false },
      { id: 3, name: 'C', parentId: 2, collapsed: false },
    ]
    expect(canNestGroup(groups, 1, 1)).toBe(false)
    expect(canNestGroup(groups, 1, 3)).toBe(false)
    expect(canNestGroup(groups, 3, null)).toBe(true)
  })
})
