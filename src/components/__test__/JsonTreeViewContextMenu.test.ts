import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import JsonTreeView from '../JsonTreeView.vue'

const written: string[] = []

beforeEach(() => {
  written.length = 0
  vi.stubGlobal('navigator', {
    clipboard: {
      writeText: vi.fn((t: string) => {
        written.push(t)
        return Promise.resolve()
      }),
    },
  })
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('JsonTreeView context menu', () => {
  it('shows copy path and copy value in context menu for leaf row', async () => {
    const text = JSON.stringify({ name: 'Grace' })
    const wrapper = mount(JsonTreeView, {
      attachTo: document.body,
      props: { text, active: true },
    })
    await nextTick()
    const nameRow = wrapper.findAll('.tree-row').find((r) => r.text().includes('name'))
    await nameRow!.trigger('contextmenu')
    expect(document.body.querySelector('[data-testid="ctx-copy-path"]')).not.toBeNull()
    expect(document.body.querySelector('[data-testid="ctx-copy-value"]')).not.toBeNull()
    wrapper.unmount()
  })

  it('shows collapse/expand for container rows', async () => {
    const text = JSON.stringify({ profile: { age: 30 } })
    const wrapper = mount(JsonTreeView, {
      attachTo: document.body,
      props: { text, active: true },
    })
    await nextTick()
    const profileRow = wrapper.findAll('.tree-row').find((r) => r.text().includes('profile'))
    await profileRow!.trigger('contextmenu')
    // should have collapse/expand option
    const toggleItem = document.body.querySelector('[data-testid="ctx-toggle"]')
    expect(toggleItem).not.toBeNull()
    wrapper.unmount()
  })

  it('does NOT show collapse/expand for leaf rows', async () => {
    const text = JSON.stringify({ name: 'Grace' })
    const wrapper = mount(JsonTreeView, {
      attachTo: document.body,
      props: { text, active: true },
    })
    await nextTick()
    const nameRow = wrapper.findAll('.tree-row').find((r) => r.text().includes('"name"'))
    await nameRow!.trigger('contextmenu')
    const toggleItem = document.body.querySelector('[data-testid="ctx-toggle"]')
    expect(toggleItem).toBeNull()
    wrapper.unmount()
  })

  it('copies the JSON path when Copy path is clicked', async () => {
    const text = JSON.stringify({ name: 'Grace' })
    const wrapper = mount(JsonTreeView, {
      attachTo: document.body,
      props: { text, active: true },
    })
    await nextTick()
    const nameRow = wrapper.findAll('.tree-row').find((r) => r.text().includes('"name"'))
    await nameRow!.trigger('contextmenu')
    const copyPath = document.body.querySelector('[data-testid="ctx-copy-path"]') as HTMLElement
    copyPath.click()
    await nextTick()
    expect(written).toContain('$/name')
    wrapper.unmount()
  })

  it('copies the value label when Copy value is clicked', async () => {
    const text = JSON.stringify({ name: 'Grace' })
    const wrapper = mount(JsonTreeView, {
      attachTo: document.body,
      props: { text, active: true },
    })
    await nextTick()
    const nameRow = wrapper.findAll('.tree-row').find((r) => r.text().includes('"name"'))
    await nameRow!.trigger('contextmenu')
    const copyValue = document.body.querySelector('[data-testid="ctx-copy-value"]') as HTMLElement
    copyValue.click()
    await nextTick()
    expect(written).toContain('"Grace"')
    wrapper.unmount()
  })

  it('separator is present for container rows', async () => {
    const text = JSON.stringify({ profile: { age: 30 } })
    const wrapper = mount(JsonTreeView, {
      attachTo: document.body,
      props: { text, active: true },
    })
    await nextTick()
    const profileRow = wrapper.findAll('.tree-row').find((r) => r.text().includes('profile'))
    await profileRow!.trigger('contextmenu')
    const sep = document.body.querySelector('[data-testid="ctx-separator"]')
    expect(sep).not.toBeNull()
    wrapper.unmount()
  })

  it('Collapse all collapses every container below the root; Expand all restores them', async () => {
    const text = JSON.stringify({ a: { b: { c: 1 } }, d: [1] })
    const wrapper = mount(JsonTreeView, {
      attachTo: document.body,
      props: { text, active: true },
    })
    await nextTick()
    const labels = () => wrapper.findAll('.tree-row').map((row) => row.text().replace(/\s+/g, ' '))
    await wrapper.findAll('.tree-row')[0].trigger('contextmenu')
    ;(document.body.querySelector('[data-testid="ctx-collapse-all"]') as HTMLElement).click()
    await nextTick()
    expect(labels().some((label) => label.includes('c'))).toBe(false)
    expect(labels().length).toBe(3)
    await wrapper.findAll('.tree-row')[0].trigger('contextmenu')
    ;(document.body.querySelector('[data-testid="ctx-expand-all"]') as HTMLElement).click()
    await nextTick()
    expect(labels().length).toBe(6)
    wrapper.unmount()
  })
})
