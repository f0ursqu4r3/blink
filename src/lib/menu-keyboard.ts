/** True for the keys that open a context menu: Shift+F10 and the Context Menu key. */
export function isContextMenuKey(event: KeyboardEvent) {
  if (event.key === 'ContextMenu') return true
  return event.key === 'F10' && event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey
}

/**
 * Open the context menu for `target` at its center. macOS WebKit does not
 * send a contextmenu event for Shift+F10, so Blink sends one.
 */
export function openContextMenuAt(target: Element) {
  const rect = target.getBoundingClientRect()
  target.dispatchEvent(
    new MouseEvent('contextmenu', {
      bubbles: true,
      cancelable: true,
      clientX: rect.left + rect.width / 2,
      clientY: rect.top + rect.height / 2,
    }),
  )
}
