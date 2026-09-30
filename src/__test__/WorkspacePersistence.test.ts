import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import App from '../App.vue'
import { WORKSPACE_KEY } from '@/lib/workspace'
const wrappers: ReturnType<typeof mount>[] = []
const render = () => {
  const app = mount(App, { attachTo: document.body })
  wrappers.push(app)
  return app
}
beforeEach(() => localStorage.clear())
afterEach(async () => {
  wrappers.splice(0).forEach((w) => w.unmount())
  await flushPromises()
  vi.restoreAllMocks()
})

describe('workspace recovery controls', () => {
  it('preserves corrupt state until replacement is explicitly confirmed', async () => {
    localStorage.setItem(WORKSPACE_KEY, 'broken')
    const app = render()
    expect(app.get('[role="alert"]').text()).toContain('has not been changed')
    expect(app.find('[data-request-url]').exists()).toBe(false)
    const button = (name: string) => app.findAll('button').find((item) => item.text() === name)!
    await button('Start fresh').trigger('click')
    expect(localStorage.getItem(WORKSPACE_KEY)).toBe('broken')
    await button('Replace saved workspace').trigger('click')
    await flushPromises()
    expect(app.get<HTMLInputElement>('[data-request-url]').element.value).toBe('')
    expect(JSON.parse(localStorage.getItem(WORKSPACE_KEY)!).tabs).toHaveLength(1)
  })
  it('shows storage failures and saves the latest draft after Retry', async () => {
    const app = render()
    const set = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('Storage quota exceeded', 'QuotaExceededError')
    })
    await app.get('[data-request-url]').setValue('https://example.test/latest')
    await flushPromises()
    expect(app.get('[role="alert"]').text()).toContain('Changes are not saved')
    expect(app.text()).toContain('NOT SAVED')
    set.mockRestore()
    await app
      .findAll('button')
      .find((b) => b.text() === 'Retry')!
      .trigger('click')
    await flushPromises()
    expect(app.find('[role="alert"]').exists()).toBe(false)
    expect(JSON.parse(localStorage.getItem(WORKSPACE_KEY)!).tabs[0].draft.url).toBe(
      'https://example.test/latest',
    )
  })
})
