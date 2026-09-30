import { afterEach, describe, expect, it } from 'vitest'
import { createThemeState } from '../useTheme'
import { THEME_KEY, applyTheme, type ThemeSetting } from '@/lib/theme'

const paper: ThemeSetting = {
  name: 'Paper',
  text: 'background = #ffffff\nforeground = #111111',
  accent: 3,
}
const styleText = () => document.getElementById('blink-theme')?.textContent

afterEach(() => {
  localStorage.clear()
  applyTheme(document, null)
})

describe('theme state', () => {
  it('applies the stored theme when created', () => {
    localStorage.setItem(THEME_KEY, JSON.stringify(paper))
    const state = createThemeState(localStorage, document)
    expect(state.name.value).toBe('Paper')
    expect(styleText()).toContain('--term-bg:#ffffff;')
  })

  it('previews, then reverts to the default', () => {
    const state = createThemeState(localStorage, document)
    state.preview(paper)
    expect(document.documentElement.dataset.theme).toBe('ghostty')
    state.revert()
    expect(document.documentElement.dataset.theme).toBeUndefined()
    expect(state.draft.value).toBeNull()
    expect(localStorage.getItem(THEME_KEY)).toBeNull()
  })

  it('keeps the last valid colors while the text is invalid', () => {
    const state = createThemeState(localStorage, document)
    state.preview(paper)
    state.preview({ ...paper, text: 'foreground = white' })
    expect(state.error.value).toBe('line 1: foreground is not a hex colour')
    expect(styleText()).toContain('--term-bg:#ffffff;')
    expect(state.commit()).toBe('line 1: foreground is not a hex colour')
    expect(localStorage.getItem(THEME_KEY)).toBeNull()
  })

  it('commits the draft to storage', () => {
    const state = createThemeState(localStorage, document)
    state.preview(paper)
    expect(state.commit()).toBe('')
    expect(JSON.parse(localStorage.getItem(THEME_KEY)!)).toEqual(paper)
    expect(state.name.value).toBe('Paper')
    state.preview(null)
    expect(state.commit()).toBe('')
    expect(localStorage.getItem(THEME_KEY)).toBeNull()
    expect(state.name.value).toBe('Blink')
  })

  it('reports a storage failure and keeps the theme for the session', () => {
    const full = {
      getItem: () => null,
      setItem: () => {
        throw new Error('quota')
      },
      removeItem: () => {},
    } as unknown as Storage
    const state = createThemeState(full, document)
    state.preview(paper)
    expect(state.commit()).toBe('Theme not saved: local storage is full or disabled.')
    expect(state.saveError.value).toBe('Theme not saved: local storage is full or disabled.')
    state.revert()
    expect(styleText()).toContain('--term-bg:#ffffff;')
  })

  it('commits without writing when the draft has not changed and storage is unavailable', () => {
    const state = createThemeState(null, document)
    expect(state.commit()).toBe('')
  })
})
