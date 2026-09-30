import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import ThemeSettings from '../ThemeSettings.vue'
import { useTheme } from '@/composables/useTheme'
import { readGhosttyTheme } from '@/lib/ghostty-themes'

vi.mock('@/lib/ghostty-themes', () => ({
  canReadGhosttyThemes: true,
  listGhosttyThemes: vi.fn(async () => ['Monokai Pro']),
  readGhosttyTheme: vi.fn(async (name: string) => {
    if (name !== 'Monokai Pro') throw 'Theme not found.'
    return 'background = #262427\nforeground = #fcfcfa\npalette = 5=#a392e8\n'
  }),
}))

beforeEach(() => {
  localStorage.clear()
  useTheme().reload()
})
afterEach(() => useTheme().reload())

describe('ThemeSettings', () => {
  it('fills the colors from an installed theme and previews it', async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body })
    await flushPromises()
    await wrapper.get('#app-theme').setValue('Monokai Pro')
    await flushPromises()
    expect(wrapper.get<HTMLTextAreaElement>('[data-theme-colors]').element.value).toContain(
      '#262427',
    )
    expect(document.documentElement.dataset.theme).toBe('ghostty')
    expect(useTheme().draft.value?.name).toBe('Monokai Pro')
    wrapper.unmount()
  })

  it('names edited text Custom and changes the accent slot', async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body })
    await flushPromises()
    await wrapper.get('[data-theme-colors]').setValue('background = #101010\npalette = 5=#ff00ff')
    await wrapper.get('#app-theme-accent').setValue('5')
    expect(useTheme().draft.value).toMatchObject({ name: 'Custom', accent: 5 })
    expect(document.getElementById('blink-theme')?.textContent).toContain('--term-accent:#ff00ff;')
    wrapper.unmount()
  })

  it('defaults the accent to Blue (slot 4) with no theme saved', async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body })
    await flushPromises()
    await wrapper.get('[data-theme-colors]').setValue('background = #101010\npalette = 4=#0000ff')
    expect(useTheme().draft.value?.accent).toBe(4)
    expect(wrapper.get<HTMLSelectElement>('#app-theme-accent').element.value).toBe('4')
    wrapper.unmount()
  })

  it('shows parse errors and resets to the default', async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body })
    await flushPromises()
    await wrapper.get('[data-theme-colors]').setValue('foreground = white')
    expect(wrapper.get('[data-theme-error]').text()).toBe('line 1: foreground is not a hex colour')
    await wrapper.get('[data-theme-reset]').trigger('click')
    expect(wrapper.find('[data-theme-error]').exists()).toBe(false)
    expect(useTheme().draft.value).toBeNull()
    expect(document.documentElement.dataset.theme).toBeUndefined()
    wrapper.unmount()
  })

  it('clearing the text returns to the default theme', async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body })
    await flushPromises()
    await wrapper.get('[data-theme-colors]').setValue('background = #101010')
    await wrapper.get('[data-theme-colors]').setValue('')
    expect(useTheme().draft.value).toBeNull()
    expect(wrapper.find('[data-theme-error]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('keeps a newer edit when a slower theme read resolves late', async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body })
    await flushPromises()
    let resolveRead!: (text: string) => void
    const pending = new Promise<string>((resolve) => {
      resolveRead = resolve
    })
    vi.mocked(readGhosttyTheme).mockImplementationOnce(() => pending)

    await wrapper.get('#app-theme').setValue('Monokai Pro')
    await wrapper.get('[data-theme-colors]').setValue('background = #101010\npalette = 5=#ff00ff')

    resolveRead('background = #262427\nforeground = #fcfcfa\npalette = 5=#a392e8\n')
    await flushPromises()

    expect(useTheme().draft.value).toMatchObject({
      name: 'Custom',
      text: 'background = #101010\npalette = 5=#ff00ff',
    })
    wrapper.unmount()
  })

  it('drops a theme read that resolves after the component unmounts', async () => {
    const wrapper = mount(ThemeSettings, { attachTo: document.body })
    await flushPromises()
    let resolveRead!: (text: string) => void
    const pending = new Promise<string>((resolve) => {
      resolveRead = resolve
    })
    vi.mocked(readGhosttyTheme).mockImplementationOnce(() => pending)

    await wrapper.get('#app-theme').setValue('Monokai Pro')
    wrapper.unmount()

    resolveRead('background = #262427\nforeground = #fcfcfa\npalette = 5=#a392e8\n')
    await flushPromises()

    expect(document.documentElement.dataset.theme).toBeUndefined()
    expect(useTheme().draft.value).toBeNull()
  })
})
