import { describe, expect, it, vi } from 'vitest'
import { isContextMenuKey, openContextMenuAt } from '../menu-keyboard'

describe('isContextMenuKey', () => {
  it('accepts Shift+F10 and the Context Menu key', () => {
    expect(isContextMenuKey(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true }))).toBe(
      true,
    )
    expect(isContextMenuKey(new KeyboardEvent('keydown', { key: 'ContextMenu' }))).toBe(true)
  })

  it('rejects F10 alone and F10 with other modifiers', () => {
    expect(isContextMenuKey(new KeyboardEvent('keydown', { key: 'F10' }))).toBe(false)
    expect(
      isContextMenuKey(
        new KeyboardEvent('keydown', {
          key: 'F10',
          shiftKey: true,
          metaKey: true,
        }),
      ),
    ).toBe(false)
  })
})

describe('openContextMenuAt', () => {
  it('dispatches a bubbling contextmenu event at the element center', () => {
    const el = document.createElement('button')
    el.getBoundingClientRect = () => ({ left: 10, top: 20, width: 100, height: 40 }) as DOMRect
    const listener = vi.fn()
    document.body.append(el)
    document.body.addEventListener('contextmenu', listener)
    openContextMenuAt(el)
    const event = listener.mock.calls[0][0] as MouseEvent
    expect(event.clientX).toBe(60)
    expect(event.clientY).toBe(40)
    document.body.removeEventListener('contextmenu', listener)
    el.remove()
  })
})
