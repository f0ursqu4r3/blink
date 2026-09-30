import { afterEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import RequestTabs from '../RequestTabs.vue'
import { createSession } from '@/lib/session'

const wrappers: ReturnType<typeof mount>[] = []
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount())
  document.body.innerHTML = ''
})

const TAB = 210
// jsdom has no layout. Give the strip a width and each tab a position.
function layout(strip: HTMLElement, width: number, scrollLeft = 0) {
  const cells = strip.querySelectorAll<HTMLElement>('[data-tab-id]')
  Object.defineProperty(strip, 'clientWidth', {
    configurable: true,
    value: width,
  })
  Object.defineProperty(strip, 'scrollWidth', {
    configurable: true,
    value: cells.length * TAB,
  })
  strip.scrollLeft = scrollLeft
  cells.forEach((cell, index) => {
    Object.defineProperty(cell, 'offsetLeft', {
      configurable: true,
      value: index * TAB,
    })
    Object.defineProperty(cell, 'offsetWidth', {
      configurable: true,
      value: TAB,
    })
  })
  strip.dispatchEvent(new Event('scroll'))
}

function render(count: number) {
  const sessions = Array.from({ length: count }, () => createSession())
  const wrapper = mount(RequestTabs, {
    props: { sessions, activeId: sessions[0].id },
    attachTo: document.body,
  })
  wrappers.push(wrapper)
  const strip = wrapper.get('[role="tablist"]').element as HTMLElement
  return { wrapper, sessions, strip }
}

describe('tab bar overflow', () => {
  it('shows no fade and no overflow button when every tab fits', async () => {
    const { wrapper, strip } = render(2)
    layout(strip, 1000)
    await nextTick()
    expect(strip.dataset.overflowLeft).toBe('false')
    expect(strip.dataset.overflowRight).toBe('false')
    expect(wrapper.find('[data-tab-overflow]').exists()).toBe(false)
  })

  it('fades the sides that have hidden tabs and counts them', async () => {
    const { wrapper, strip } = render(6)
    layout(strip, 500, 0)
    await nextTick()
    expect(strip.dataset.overflowLeft).toBe('false')
    expect(strip.dataset.overflowRight).toBe('true')
    // Tabs 0 and 1 fit fully in 500 px; tabs 2..5 are hidden or cut.
    expect(wrapper.get('[data-tab-overflow]').text()).toContain('4')
    expect(wrapper.get('[data-tab-overflow]').attributes('aria-label')).toBe('4 more tabs')
    layout(strip, 500, 420)
    await nextTick()
    expect(strip.dataset.overflowLeft).toBe('true')
    expect(strip.dataset.overflowRight).toBe('true')
  })

  it('uses the singular label for exactly one hidden tab', async () => {
    const { wrapper, strip } = render(3)
    // Tabs 0 and 1 fit fully in 420 px; tab 2 is hidden.
    layout(strip, 420, 0)
    await nextTick()
    expect(wrapper.get('[data-tab-overflow]').text()).toContain('1')
    expect(wrapper.get('[data-tab-overflow]').attributes('aria-label')).toBe('1 more tab')
  })

  it('scrolls horizontally with a vertical wheel', async () => {
    const { strip } = render(6)
    layout(strip, 500, 0)
    const event = new WheelEvent('wheel', { deltaY: 120, cancelable: true })
    strip.dispatchEvent(event)
    expect(strip.scrollLeft).toBe(120)
    expect(event.defaultPrevented).toBe(true)
  })

  it('leaves a horizontal wheel to the browser', async () => {
    const { strip } = render(6)
    layout(strip, 500, 0)
    const event = new WheelEvent('wheel', {
      deltaX: 80,
      deltaY: 10,
      cancelable: true,
    })
    strip.dispatchEvent(event)
    expect(event.defaultPrevented).toBe(false)
  })

  it('selects a tab from the overflow menu', async () => {
    const { wrapper, sessions, strip } = render(6)
    layout(strip, 500, 0)
    await nextTick()
    const trigger = wrapper.get('[data-tab-overflow]')
    // reka DropdownMenu opens on pointerdown (button 0) or Enter.
    await trigger.trigger('keydown', { key: 'Enter' })
    await nextTick()
    const items = document.body.querySelectorAll<HTMLElement>('[data-tab-overflow-item]')
    expect(items).toHaveLength(6)
    items[5].click()
    await nextTick()
    expect(wrapper.emitted('select')).toContainEqual([sessions[5].id])
  })
})
