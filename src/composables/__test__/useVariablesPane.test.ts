import { afterEach, describe, expect, it } from 'vitest'
import {
  MIN_QUERY_HEIGHT,
  MIN_VARIABLES_HEIGHT,
  resizeVariables,
  useVariablesPane,
} from '../useVariablesPane'

afterEach(() => {
  const pane = useVariablesPane()
  pane.height.value = 128
  pane.collapsed.value = false
})

describe('resizeVariables', () => {
  it('grows by the drag distance while the query keeps its minimum', () => {
    expect(resizeVariables(128, 50, 300)).toBe(178)
  })
  it('stops growing when the query editor reaches its minimum', () => {
    expect(resizeVariables(128, 1000, 300)).toBe(128 + 300 - MIN_QUERY_HEIGHT)
  })
  it('stops shrinking at the variables minimum', () => {
    expect(resizeVariables(128, -1000, 300)).toBe(MIN_VARIABLES_HEIGHT)
  })
  it('never goes below the minimum when the query is already small', () => {
    expect(resizeVariables(64, 40, 50)).toBe(MIN_VARIABLES_HEIGHT)
  })
})

describe('useVariablesPane', () => {
  it('starts expanded at 128px', () => {
    const pane = useVariablesPane()
    expect(pane.height.value).toBe(128)
    expect(pane.collapsed.value).toBe(false)
  })
  it('shares state between callers', () => {
    useVariablesPane().collapsed.value = true
    useVariablesPane().height.value = 200
    expect(useVariablesPane().collapsed.value).toBe(true)
    expect(useVariablesPane().height.value).toBe(200)
  })
})
