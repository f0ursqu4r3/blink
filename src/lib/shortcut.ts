const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform)

// "ctrl" is the Control key on every platform; "mod" is ⌘ on macOS.
const modifiers = ['mod', 'ctrl', 'shift', 'alt']
// VS Code order: ⌃⌥⇧⌘ on macOS, Ctrl+Alt+Shift elsewhere.
const macOrder = ['ctrl', 'alt', 'shift', 'mod']
const wordOrder = ['mod', 'ctrl', 'alt', 'shift']
const macSymbols: Record<string, string> = {
  ctrl: '⌃',
  tab: 'Tab',
  alt: '⌥',
  shift: '⇧',
  mod: '⌘',
  enter: '↵',
  esc: 'Esc',
}
const words: Record<string, string> = {
  ctrl: 'Ctrl',
  tab: 'Tab',
  alt: 'Alt',
  shift: 'Shift',
  mod: 'Ctrl',
  enter: 'Enter',
  esc: 'Esc',
}

/**
 * A shortcut hint for menus, such as "⇧⌘D" on macOS and "Ctrl+Shift+D"
 * elsewhere. Keys: "mod", "ctrl", "shift", "alt", "enter", "esc", "tab", or one
 * character.
 */
export function shortcutLabel(keys: string[], mac = isMac) {
  const order = mac ? macOrder : wordOrder
  const names = mac ? macSymbols : words
  const sorted = [
    ...keys
      .filter((key) => modifiers.includes(key))
      .sort((a, b) => order.indexOf(a) - order.indexOf(b)),
    ...keys.filter((key) => !modifiers.includes(key)),
  ]
  return sorted.map((key) => names[key] ?? key.toUpperCase()).join(mac ? '' : '+')
}
