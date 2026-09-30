import { describe, expect, it } from 'vitest'
import { shortcutLabel } from '../shortcut'

describe('shortcutLabel', () => {
  it('uses symbols in VS Code order on macOS', () => {
    expect(shortcutLabel(['mod', 'shift', 'd'], true)).toBe('⇧⌘D')
    expect(shortcutLabel(['mod', 'enter'], true)).toBe('⌘↵')
    expect(shortcutLabel(['esc'], true)).toBe('Esc')
  })

  it('uses words joined by + on other platforms', () => {
    expect(shortcutLabel(['mod', 'shift', 'd'], false)).toBe('Ctrl+Shift+D')
    expect(shortcutLabel(['mod', 'enter'], false)).toBe('Ctrl+Enter')
    expect(shortcutLabel(['mod', 'w'], false)).toBe('Ctrl+W')
  })
})

describe('control and tab keys', () => {
  it('labels Control separately from the platform modifier', () => {
    expect(shortcutLabel(['ctrl', 'shift', 'tab'], true)).toBe('⌃⇧Tab')
    expect(shortcutLabel(['ctrl', 'tab'], false)).toBe('Ctrl+Tab')
  })
})
