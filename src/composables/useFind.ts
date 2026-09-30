import { computed, nextTick, onScopeDispose, ref, watch, type Ref } from 'vue'

export type FindOccurrence = { row: number; ordinal: number }

const MATCHES = 'blink-find'
const CURRENT = 'blink-find-current'
/** Painting stops at this many ranges; counting does not. */
const PAINT_LIMIT = 2000

type HighlightRegistry = Map<string, unknown>
const registry = (): HighlightRegistry | undefined =>
  typeof CSS !== 'undefined' && 'highlights' in CSS
    ? (CSS as unknown as { highlights: HighlightRegistry }).highlights
    : undefined
const HighlightClass = () =>
  (globalThis as { Highlight?: new (...ranges: Range[]) => unknown }).Highlight

/** Occurrences of `needle` (lower case) in `text`, without case. */
function positions(text: string, needle: string) {
  const found: number[] = []
  const haystack = text.toLocaleLowerCase()
  for (
    let at = haystack.indexOf(needle);
    at >= 0;
    at = haystack.indexOf(needle, at + needle.length)
  )
    found.push(at)
  return found
}

/**
 * Find in a virtual list: counts every occurrence in `texts` (one per row),
 * moves between them, and highlights the ones in rendered rows. Rendered
 * rows carry `data-index`.
 */
export function useFind(options: {
  element: Ref<HTMLElement | undefined>
  texts: () => string[]
  query: () => string
  active: () => boolean
  /** Changes when the rendered rows change. */
  rendered: () => unknown
  scrollToIndex: (index: number) => void
}) {
  const needle = computed(() => options.query().trim().toLocaleLowerCase())
  const occurrences = computed<FindOccurrence[]>(() => {
    if (!needle.value) return []
    const list: FindOccurrence[] = []
    options
      .texts()
      .forEach((text, row) =>
        positions(text, needle.value).forEach((_, ordinal) => list.push({ row, ordinal })),
      )
    return list
  })
  const current = ref(0)
  watch(needle, () => {
    current.value = 0
    reveal()
  })
  watch(occurrences, (list) => {
    if (current.value >= list.length) current.value = 0
  })

  function reveal() {
    const occurrence = occurrences.value[current.value]
    if (occurrence) options.scrollToIndex(occurrence.row)
  }
  /** Move to the next (`1`) or previous (`-1`) occurrence. */
  function step(direction: 1 | -1) {
    const count = occurrences.value.length
    if (!count) return
    current.value = (current.value + direction + count) % count
    reveal()
  }

  let owner = false
  function clear() {
    if (!owner) return
    registry()?.delete(MATCHES)
    registry()?.delete(CURRENT)
    owner = false
  }
  function paint() {
    const highlights = registry()
    const Highlight = HighlightClass()
    const root = options.element.value
    if (!highlights || !Highlight || !root) return
    if (!options.active() || !needle.value) return clear()
    const target = occurrences.value[current.value]
    const all: Range[] = []
    const selected: Range[] = []
    for (const row of root.querySelectorAll<HTMLElement>('[data-index]')) {
      if (all.length >= PAINT_LIMIT) break
      const index = Number(row.dataset.index)
      // Map offsets in the row text to text nodes.
      const nodes: { node: Text; start: number }[] = []
      let text = ''
      const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT)
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        nodes.push({ node: node as Text, start: text.length })
        text += node.nodeValue ?? ''
      }
      const locate = (offset: number) => {
        let entry = nodes[0]
        for (const candidate of nodes)
          if (candidate.start <= offset) entry = candidate
          else break
        return { node: entry.node, offset: offset - entry.start }
      }
      positions(text, needle.value).forEach((at, ordinal) => {
        const range = document.createRange()
        const start = locate(at)
        const end = locate(at + needle.value.length)
        range.setStart(start.node, start.offset)
        range.setEnd(end.node, end.offset)
        if (target && target.row === index && target.ordinal === ordinal) selected.push(range)
        else all.push(range)
      })
    }
    highlights.set(MATCHES, new Highlight(...all))
    highlights.set(CURRENT, new Highlight(...selected))
    owner = true
  }
  watch(
    [occurrences, current, () => options.active(), () => options.rendered(), options.element],
    () => void nextTick(paint),
    { flush: 'post' },
  )
  onScopeDispose(clear)

  const count = computed(() => occurrences.value.length)
  return { count, current, step }
}
