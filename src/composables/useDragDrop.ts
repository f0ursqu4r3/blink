import { getCurrentInstance, nextTick, onBeforeUnmount, reactive, readonly } from 'vue'
import type { DragPayload, DropZone, Point } from '@/lib/drag-drop'

export type DropHit = {
  key: string
  zone: DropZone
  commit: () => void
  expand?: () => void
}
export type DropSurface = {
  el: () => HTMLElement | null | undefined
  axis: 'x' | 'y'
  /** Element that scrolls at the edges. Defaults to `el`. */
  scroller?: () => HTMLElement | null | undefined
  resolve: (payload: DragPayload, point: Point) => DropHit | null
}
export type DragPreview = { label: string; method?: string; folder?: boolean }
export type DragSource = {
  payload: () => DragPayload
  preview: () => DragPreview
  onStart?: () => void
}

const START_DISTANCE = 4
const TOUCH_DELAY = 250
const TOUCH_SLOP = 8
const EXPAND_DELAY = 600
const EDGE = 24
const MAX_SCROLL = 12

// One drag session for the whole app, so the sidebar and the tab bar can
// drop onto each other.
const state = reactive({
  payload: null as DragPayload | null,
  preview: null as DragPreview | null,
  point: { x: 0, y: 0 } as Point,
  hit: null as { key: string; zone: DropZone } | null,
})
const surfaces = new Set<DropSurface>()
let current: DropHit | null = null
let press: {
  source: DragSource
  origin: Point
  touch: boolean
  el: Element | null
  timer?: ReturnType<typeof setTimeout>
} | null = null
let expandTimer: ReturnType<typeof setTimeout> | undefined
let scrollFrame = 0

function inside(box: DOMRect, point: Point) {
  return point.x >= box.left && point.x <= box.right && point.y >= box.top && point.y <= box.bottom
}

function surfaceAt(point: Point) {
  for (const surface of surfaces) {
    const el = surface.el()
    if (el && inside(el.getBoundingClientRect(), point)) return surface
  }
  return null
}

function edgeSpeed(start: number, end: number, value: number) {
  if (value < start + EDGE) return -Math.ceil(((start + EDGE - value) / EDGE) * MAX_SCROLL)
  if (value > end - EDGE) return Math.ceil(((value - end + EDGE) / EDGE) * MAX_SCROLL)
  return 0
}

function scrollSpeed(surface: DropSurface) {
  const scroller = surface.scroller?.() ?? surface.el()
  if (!scroller) return { scroller: null, speed: 0 }
  const box = scroller.getBoundingClientRect()
  const speed =
    surface.axis === 'y'
      ? edgeSpeed(box.top, box.bottom, state.point.y)
      : edgeSpeed(box.left, box.right, state.point.x)
  return { scroller, speed }
}

function scrollStep() {
  scrollFrame = 0
  const surface = state.payload ? surfaceAt(state.point) : null
  if (!surface) return
  const { scroller, speed } = scrollSpeed(surface)
  if (!scroller || !speed) return
  const axis = surface.axis === 'y' ? 'scrollTop' : 'scrollLeft'
  const before = scroller[axis]
  scroller[axis] += speed
  if (scroller[axis] !== before) update(state.point)
}

function update(point: Point) {
  state.point = point
  const surface = surfaceAt(point)
  const hit = surface && state.payload ? surface.resolve(state.payload, point) : null
  if (hit?.key !== current?.key || hit?.zone !== current?.zone) {
    clearTimeout(expandTimer)
    const expand = hit?.zone === 'into' ? hit.expand : undefined
    if (expand) expandTimer = setTimeout(expand, EXPAND_DELAY)
  }
  current = hit
  state.hit = hit ? { key: hit.key, zone: hit.zone } : null
  document.documentElement.toggleAttribute('data-drag-invalid', !hit)
  if (surface && !scrollFrame && scrollSpeed(surface).speed)
    scrollFrame = requestAnimationFrame(scrollStep)
}

function activate(point: Point) {
  if (!press) return
  clearTimeout(press.timer)
  // A touch or pen move on the source clears the context menu trigger's
  // long-press timer, so holding still during the drag does not open it.
  if (press.touch)
    press.el?.dispatchEvent(
      new PointerEvent('pointermove', {
        bubbles: true,
        clientX: point.x,
        clientY: point.y,
        pointerType: 'touch',
      }),
    )
  press.source.onStart?.()
  state.payload = press.source.payload()
  state.preview = press.source.preview()
  document.documentElement.setAttribute('data-dragging', '')
  update(point)
}

function onMove(event: PointerEvent) {
  const point = { x: event.clientX, y: event.clientY }
  if (state.payload) return update(point)
  if (!press) return
  const distance = Math.hypot(point.x - press.origin.x, point.y - press.origin.y)
  if (press.touch) {
    if (distance > TOUCH_SLOP) end()
  } else if (distance >= START_DISTANCE) activate(point)
}

function onUp() {
  const dragged = state.payload !== null
  const hit = dragged ? current : null
  end()
  if (dragged) suppressClick()
  if (hit) commit(hit)
}

function onKey(event: KeyboardEvent) {
  if (event.key !== 'Escape') return
  // Before the drag starts, Escape belongs to the page (dialogs, menus).
  if (!state.payload) return end()
  event.preventDefault()
  event.stopPropagation()
  end()
}

/**
 * macOS Ctrl+click opens a context menu from a primary-button press: drop the
 * pending press so moving to the menu does not start a drag. A long-press
 * that already started a drag keeps it.
 */
function onContextMenu(event: MouseEvent) {
  if (state.payload) event.preventDefault()
  else end()
}

/** No text selection from a press on a row, before or during the drag. */
function onSelectStart(event: Event) {
  event.preventDefault()
}

function onTouchMove(event: TouchEvent) {
  // Keep the page from scrolling under an active touch drag.
  if (state.payload && event.cancelable) event.preventDefault()
}

function listen() {
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', end)
  window.addEventListener('keydown', onKey, { capture: true })
  window.addEventListener('blur', end)
  window.addEventListener('contextmenu', onContextMenu)
  window.addEventListener('selectstart', onSelectStart)
  window.addEventListener('touchmove', onTouchMove, { passive: false })
}

function unlisten() {
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  window.removeEventListener('pointercancel', end)
  window.removeEventListener('keydown', onKey, { capture: true })
  window.removeEventListener('blur', end)
  window.removeEventListener('contextmenu', onContextMenu)
  window.removeEventListener('selectstart', onSelectStart)
  window.removeEventListener('touchmove', onTouchMove)
}

function end() {
  if (press) clearTimeout(press.timer)
  press = null
  clearTimeout(expandTimer)
  if (scrollFrame) cancelAnimationFrame(scrollFrame)
  scrollFrame = 0
  current = null
  state.payload = null
  state.preview = null
  state.hit = null
  document.documentElement.removeAttribute('data-dragging')
  document.documentElement.removeAttribute('data-drag-invalid')
  unlisten()
}

/** The release of a drag also fires `click` on the row under it. Drop it. */
function suppressClick() {
  const block = (event: MouseEvent) => {
    event.preventDefault()
    event.stopPropagation()
  }
  window.addEventListener('click', block, { capture: true, once: true })
  setTimeout(() => window.removeEventListener('click', block, { capture: true }), 0)
}

function rowRects() {
  const rects = new Map<string, DOMRect>()
  for (const surface of surfaces)
    surface
      .el()
      ?.querySelectorAll<HTMLElement>('[data-drop-key]')
      .forEach((el) => rects.set(el.dataset.dropKey ?? '', el.getBoundingClientRect()))
  return rects
}

/** FLIP: slide rows from their old position to the new one. */
function animateFrom(before: Map<string, DOMRect>) {
  if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return
  for (const surface of surfaces)
    surface
      .el()
      ?.querySelectorAll<HTMLElement>('[data-drop-key]')
      .forEach((el) => {
        const previous = before.get(el.dataset.dropKey ?? '')
        if (!previous) return
        const next = el.getBoundingClientRect()
        const dx = previous.left - next.left
        const dy = previous.top - next.top
        if (dx || dy)
          el.animate?.([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], {
            duration: 150,
            easing: 'ease-out',
          })
      })
}

function commit(hit: DropHit) {
  const before = rowRects()
  hit.commit()
  void nextTick(() => animateFrom(before))
}

function startPress(event: PointerEvent, source: DragSource) {
  if (event.button !== 0 || press || state.payload) return
  const target = event.target as Element | null
  if (target?.closest?.('input, textarea, select, [data-no-drag]')) return
  press = {
    source,
    origin: { x: event.clientX, y: event.clientY },
    touch: event.pointerType !== 'mouse',
    el: event.currentTarget as Element | null,
  }
  if (press.touch)
    press.timer = setTimeout(() => {
      if (press) activate(press.origin)
    }, TOUCH_DELAY)
  listen()
}

function registerSurface(surface: DropSurface) {
  surfaces.add(surface)
  const unregister = () => {
    surfaces.delete(surface)
  }
  if (getCurrentInstance()) onBeforeUnmount(unregister)
  return unregister
}

export function useDragDrop() {
  return {
    state: readonly(state),
    startPress,
    registerSurface,
    cancel: end,
  }
}
