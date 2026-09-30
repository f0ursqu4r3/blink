import { ref } from 'vue'

export const MIN_VARIABLES_HEIGHT = 64
export const MIN_QUERY_HEIGHT = 96

// One size for all tabs; resets when the app restarts.
const height = ref(128)
const collapsed = ref(false)

export const useVariablesPane = () => ({ height, collapsed })

/**
 * New variables height after moving the divider up by `delta` px.
 * `queryHeight` is the query editor height at the start of the move.
 */
export function resizeVariables(start: number, delta: number, queryHeight: number): number {
  const max = Math.max(MIN_VARIABLES_HEIGHT, start + queryHeight - MIN_QUERY_HEIGHT)
  return Math.min(max, Math.max(MIN_VARIABLES_HEIGHT, start + delta))
}
