import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { nextTick } from 'vue'
import App from '../App.vue'
beforeEach(() => localStorage.clear())

const wrappers: ReturnType<typeof mount>[] = []
function render() {
  const wrapper = mount(App, { attachTo: document.body })
  wrappers.push(wrapper)
  return wrapper
}
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount())
  vi.unstubAllGlobals()
})

describe('compact request console', () => {
  it('starts blank without fake navigation or network traffic', () => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)
    const app = render()
    expect(app.get<HTMLInputElement>('[data-request-url]').element.value).toBe('')
    expect(app.text()).not.toContain('Session activity')
    expect(app.text()).not.toContain('Settings')
    expect(app.get('[data-send]').attributes('disabled')).toBeDefined()
    expect(fetch).not.toHaveBeenCalled()
  })

  it('organizes the active request in a browser group', async () => {
    const app = render()

    await app.get('[aria-label="Add top-level group"]').trigger('click')
    await app.get('[aria-label="Top-level group name"]').setValue('Platform')
    await app.get('.top-level-form').trigger('submit')
    await app.get('[data-request-id]').trigger('click')
    await app.get('[aria-label="More actions for Platform"]').trigger('keydown', { key: 'Enter' })
    const move = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"]')).find(
      (item) => item.textContent?.trim() === 'Move selection here',
    )!
    move.click()
    await flushPromises()
    await app.get('[aria-label="More actions for Platform"]').trigger('keydown', { key: 'Enter' })
    const updatedMove = Array.from(
      document.querySelectorAll<HTMLElement>('[role="menuitem"]'),
    ).find((item) => item.textContent?.trim() === 'Move selection here')
    expect(updatedMove).toBeUndefined()
  })

  it('hides and restores the request browser from the title bar', async () => {
    const app = render()
    const toggle = app.get('[data-title-browser]')

    expect(toggle.attributes('aria-pressed')).toBe('true')
    expect(toggle.attributes('aria-label')).toBe('Hide request browser')
    await toggle.trigger('click')
    expect(toggle.attributes('aria-pressed')).toBe('false')
    expect(toggle.attributes('aria-label')).toBe('Show request browser')
    expect(app.get('[data-request-browser]').isVisible()).toBe(false)
    await toggle.trigger('click')
    expect(app.get('[data-request-browser]').isVisible()).toBe(true)
    expect(toggle.attributes('aria-label')).toBe('Hide request browser')
  })

  it('does not render the activity bar', () => {
    const app = render()
    expect(app.find('[data-activity-bar]').exists()).toBe(false)
  })

  it('opens the command center with Cmd+P and selects a request', async () => {
    const app = render()
    await app.get('[data-request-url]').setValue('https://example.test/users')
    await app.get('[data-new-request]').trigger('click')

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', metaKey: true }))
    await flushPromises()
    const input = app.get('[role="combobox"]')
    await input.setValue('users')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()

    expect(app.get('[role="tab"][aria-selected="true"]').text()).toContain('/users')
  })

  it('does not open the command center while a dialog is open', async () => {
    const app = render()
    await app.get('[data-title-settings]').trigger('click')
    await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', metaKey: true }))
    await nextTick()
    expect(document.querySelector('[data-surface="command-center"]')).toBeNull()
  })

  it('collapses every group from the browser header', async () => {
    const app = render()
    await app.get('[aria-label="Add top-level group"]').trigger('click')
    await app.get('[aria-label="Top-level group name"]').setValue('Platform')
    await app.get('.top-level-form').trigger('submit')
    expect(app.get('[aria-label="Collapse Platform"]')).toBeTruthy()
    await app.get('[aria-label="Collapse all groups"]').trigger('click')
    expect(app.get('[aria-label="Expand Platform"]')).toBeTruthy()
  })

  it('shows the theme name in the status bar', () => {
    const app = render()
    expect(app.get('[data-theme-name]').text()).toBe('Blink')
  })

  it('keeps the shell free of promotional labels', () => {
    const app = render()

    expect(app.text()).not.toContain('HTTP OPERATIONS')
    expect(app.text()).not.toContain('BROWSER PREVIEW')
    expect(app.text()).not.toContain('Enter an endpoint. Send. Inspect.')
  })

  it('sends a GET without a body and exposes actual response headers', async () => {
    const fetch = vi.fn().mockResolvedValue(
      new Response('{"ok":true}', {
        status: 200,
        headers: {
          'content-type': 'application/json',
          'x-request-id': 'test-42',
        },
      }),
    )
    vi.stubGlobal('fetch', fetch)
    const app = render()
    await app.get('[data-request-url]').setValue('https://example.test/check')
    await app.get('[data-send]').trigger('click')
    await flushPromises()
    expect(fetch).toHaveBeenCalledOnce()
    expect(fetch.mock.calls[0][1].body).toBeNull()
    expect(app.get('[data-response-body]').text()).toContain('"ok": true')
    await app.get('[data-response-headers]').trigger('mousedown', { button: 0, ctrlKey: false })
    expect(app.text()).toContain('test-42')
  })

  it('renders HTTP failures as responses, not transport failures', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(new Response('Denied', { status: 401, statusText: 'Unauthorized' })),
    )
    const app = render()
    await app.get('[data-request-url]').setValue('https://example.test')
    await app.get('[data-send]').trigger('click')
    await flushPromises()
    expect(app.get('[data-response-status]').attributes('data-tone')).toBe('error')
    expect(app.get('[data-response-body]').text()).toContain('Denied')
  })

  it('keeps the draft editable during a request and handles an empty response', async () => {
    let resolve!: (response: Response) => void
    const fetch = vi.fn(
      () =>
        new Promise<Response>((done) => {
          resolve = done
        }),
    )
    vi.stubGlobal('fetch', fetch)
    const app = render()
    await app.get('[data-request-url]').setValue('https://example.test')
    await app.get('[data-send]').trigger('click')
    expect(app.get('[data-request-url]').attributes('disabled')).toBeUndefined()
    expect(app.find('[data-send]').exists()).toBe(false)
    expect(app.find('[data-cancel]').exists()).toBe(true)
    await app.get('.request-bar').trigger('submit')
    expect(fetch).toHaveBeenCalledOnce()
    resolve(new Response(null, { status: 204, statusText: 'No Content' }))
    await flushPromises()
    expect(app.text()).toContain('Empty response body')
    expect(app.find('[data-send]').exists()).toBe(true)
  })
  it('imports a pasted cURL command into the draft', async () => {
    const app = render()
    const url = app.get('[data-request-url]')
    const paste = new Event('paste', { bubbles: true, cancelable: true })
    Object.defineProperty(paste, 'clipboardData', {
      value: {
        getData: () => "curl -sX POST https://api.test/items -H 'X-A: 1' -d '{\"a\":1}'",
      },
    })
    url.element.dispatchEvent(paste)
    await flushPromises()
    expect(paste.defaultPrevented).toBe(true)
    expect(app.get<HTMLInputElement>('input[data-method]').element.value).toBe('POST')
    expect(app.get('[data-import-notice]').text()).toContain('Ignored: -s')
    await app.get('.curl-button').trigger('click')
    expect(app.get('[data-curl-preview]').text()).toContain("--header 'X-A: 1'")
  })
  it('toggles the request and response layout from the title bar', async () => {
    const app = render()
    const toggle = app.get('[data-title-layout]')
    const panels = () => app.get('[data-request-pane] .panels')
    expect(toggle.attributes('aria-pressed')).toBe('false')
    expect(panels().attributes('data-layout')).toBe('horizontal')
    expect(app.get('[data-panel-resize]').attributes('aria-orientation')).toBe('vertical')
    await toggle.trigger('click')
    expect(toggle.attributes('aria-pressed')).toBe('true')
    expect(toggle.attributes('aria-label')).toBe('Place request and response side by side')
    expect(panels().attributes('data-layout')).toBe('vertical')
    expect(app.get('[data-panel-resize]').attributes('aria-orientation')).toBe('horizontal')
    await toggle.trigger('click')
    expect(panels().attributes('data-layout')).toBe('horizontal')
  })
  it('toggles the layout with Cmd/Ctrl+\\ and from the command list', async () => {
    const app = render()
    const layout = () => app.get('[data-request-pane] .panels').attributes('data-layout')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '\\', ctrlKey: true }))
    await flushPromises()
    expect(layout()).toBe('vertical')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'P', ctrlKey: true, shiftKey: true }))
    await flushPromises()
    const input = app.get<HTMLInputElement>('[data-surface="command-center"] [role="combobox"]')
    expect(input.element.value).toBe('>')
    await input.setValue('>side by side')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(layout()).toBe('horizontal')
  })
  it('cancels a running request', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        (_url: string, init: RequestInit) =>
          new Promise<Response>((_resolve, reject) =>
            init.signal!.addEventListener('abort', () =>
              reject(new DOMException('Aborted', 'AbortError')),
            ),
          ),
      ),
    )
    const app = render()
    await app.get('[data-request-url]').setValue('https://example.test')
    await app.get('[data-send]').trigger('click')
    await app.get('[data-cancel]').trigger('click')
    await flushPromises()
    expect(app.get('[role="alert"]').text()).toContain('Request canceled.')
    expect(app.find('[data-send]').exists()).toBe(true)
  })
  it('reports clipboard failures rather than claiming a copy succeeded', async () => {
    vi.stubGlobal('navigator', {
      ...navigator,
      clipboard: { writeText: vi.fn().mockRejectedValue(new Error('Denied')) },
    })
    const app = render()
    await app.get('[data-request-url]').setValue('https://example.test')
    await app.get('.curl-button').trigger('click')
    await app.get('[data-copy-code]').trigger('click')
    await flushPromises()
    expect(app.get('[role="alert"]').text()).toContain('Clipboard unavailable')
    expect(app.get('[data-copy-code]').text()).not.toContain('Copied')
  })
  it('keeps request errors visible and supports retry', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Connection refused')))
    const app = render()
    await app.get('[data-request-url]').setValue('http://localhost:9876')
    await app.get('[data-send]').trigger('click')
    await flushPromises()
    expect(app.get('[role="alert"]').text()).toContain('Connection refused')
    expect(app.get('[data-send]').attributes('disabled')).toBeUndefined()
  })
})

describe('tab menu actions', () => {
  async function openTabMenu(id: string) {
    document
      .querySelector(`[data-testid="tab-ctx-trigger-${id}"]`)!
      .dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }))
    await flushPromises()
  }
  function tabIds() {
    return [...document.querySelectorAll<HTMLElement>('[data-tab-id]')].map(
      (tab) => tab.dataset.tabId!,
    )
  }

  it('Close others closes every other tab and keeps the requests', async () => {
    const app = render()
    await app.get('[data-new-request]').trigger('click')
    await app.get('[data-new-request]').trigger('click')
    const [first, second, third] = tabIds()
    await openTabMenu(second)
    ;(
      document.querySelector(`[data-testid="tab-ctx-close-others-${second}"]`) as HTMLElement
    ).click()
    await flushPromises()
    expect(tabIds()).toEqual([second])
    expect([first, third].every((id) => document.querySelector(`[data-request-id="${id}"]`))).toBe(
      true,
    )
  })

  it('Reveal in Browser shows a hidden Browser and selects the request', async () => {
    const app = render()
    await app.get('[data-title-browser]').trigger('click')
    const [id] = tabIds()
    await openTabMenu(id)
    ;(document.querySelector(`[data-testid="tab-ctx-reveal-${id}"]`) as HTMLElement).click()
    await flushPromises()
    expect(app.get('[data-request-browser]').isVisible()).toBe(true)
    expect(document.querySelector(`[data-request-id="${id}"]`)?.getAttribute('aria-selected')).toBe(
      'true',
    )
  })
})

describe('group focus', () => {
  function tabIds() {
    return [...document.querySelectorAll<HTMLElement>('[data-tab-id]')].map(
      (tab) => tab.dataset.tabId!,
    )
  }
  function activeTabId() {
    return document
      .querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')
      ?.closest<HTMLElement>('[data-tab-id]')?.dataset.tabId
  }
  async function focusPlatform(app: ReturnType<typeof render>) {
    await app.get('[aria-label="More actions for Platform"]').trigger('keydown', { key: 'Enter' })
    Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"]'))
      .find((item) => item.textContent?.trim() === 'Focus')!
      .click()
    await flushPromises()
  }
  /** One ungrouped tab with a URL, then one tab in Platform; the first is active. */
  async function setup() {
    const app = render()
    await app.get('[data-request-url]').setValue('https://example.test/loose')
    await app.get('[aria-label="Add top-level group"]').trigger('click')
    await app.get('[aria-label="Top-level group name"]').setValue('Platform')
    await app.get('.top-level-form').trigger('submit')
    await app.get('[aria-label="New request in Platform"]').trigger('click')
    await flushPromises()
    const [loose, inside] = tabIds()
    await app.get(`[data-tab-id="${loose}"] [role="tab"]`).trigger('click')
    return { app, loose, inside }
  }

  it("shows only the focused group's tabs and restores the rest", async () => {
    const { app, loose, inside } = await setup()
    expect(activeTabId()).toBe(loose)

    await focusPlatform(app)
    expect(tabIds()).toEqual([inside])
    expect(activeTabId()).toBe(inside)
    expect(app.find('[data-browser-focus]').exists()).toBe(true)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(app.find('[data-browser-focus]').exists()).toBe(false)
    expect(tabIds()).toEqual([loose, inside])
    expect(activeTabId()).toBe(loose)
  })

  it('unfocuses when a request outside the group opens', async () => {
    const { app, loose } = await setup()
    await focusPlatform(app)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', metaKey: true }))
    await flushPromises()
    const input = app.get('[role="combobox"]')
    await input.setValue('loose')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()

    expect(app.find('[data-browser-focus]').exists()).toBe(false)
    expect(activeTabId()).toBe(loose)
  })

  it('adds new requests to the focused group', async () => {
    const { app, inside } = await setup()
    await focusPlatform(app)
    await app.get('[data-browser-new-request]').trigger('click')
    await flushPromises()

    expect(tabIds()).toHaveLength(2)
    expect(tabIds()[0]).toBe(inside)
    expect(app.find('[data-browser-focus]').exists()).toBe(true)
  })
})
