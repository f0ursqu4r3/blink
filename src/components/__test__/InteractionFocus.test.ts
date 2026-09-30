import { afterEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import KeyValueEditor from '../KeyValueEditor.vue'
import Button from '../ui/button/Button.vue'
import { pair, type Pair } from '@/lib/request'

enableAutoUnmount(afterEach)

function editor() {
  const wrapper = mount(KeyValueEditor, {
    attachTo: document.body,
    props: {
      label: 'Query',
      modelValue: [pair()],
      'onUpdate:modelValue': (rows: Pair[]) => wrapper.setProps({ modelValue: rows }),
    },
  })
  return wrapper
}

describe('editor focus and tooltipped controls', () => {
  it('adds and removes rows in the correct mounted request editor', async () => {
    const first = editor()
    const second = editor()
    await first.get('[data-add-row]').trigger('click')
    await second.get('[data-add-row]').trigger('click')
    await flushPromises()
    expect(document.activeElement).toBe(second.get('[aria-label="Query name 2"]').element)
    await second.get('[aria-label="Remove Query row 2"]').trigger('click')
    await flushPromises()
    expect(document.activeElement).toBe(second.get('[aria-label="Query name 1"]').element)
    await second.get('[aria-label="Remove Query row 1"]').trigger('click')
    await flushPromises()
    expect(document.activeElement).toBe(second.get('[data-add-row]').element)
    expect(first.findAll('[data-row-name]')).toHaveLength(2)
  })

  it('keeps native input context menus and blocks edits while disabled', async () => {
    const wrapper = editor()
    await wrapper.get('[aria-label="Query name 1"]').trigger('contextmenu')
    expect(document.querySelector('[role="menu"]')).toBeNull()
    await wrapper.setProps({ disabled: true })
    const before = wrapper.emitted('update:modelValue')?.length ?? 0
    await wrapper.get('[aria-label="Query name 1"]').trigger('input')
    expect(wrapper.emitted('update:modelValue')?.length ?? 0).toBe(before)
    await wrapper.get('[data-testid="kv-table-ctx-trigger"]').trigger('contextmenu')
    expect(
      document.querySelector('[data-testid="kv-ctx-add-row"]')?.getAttribute('data-disabled'),
    ).not.toBeNull()
  })

  it('preserves native button attributes and actions through a tooltip', async () => {
    const click = vi.fn()
    const wrapper = mount(Button, {
      attachTo: document.body,
      props: {
        title: 'Create a request',
        class: 'custom-control',
        variant: 'ghost',
      },
      attrs: {
        'data-control': 'new',
        'aria-label': 'New request',
        onClick: click,
      },
      slots: { default: 'New' },
    })
    const button = wrapper.get('button[data-control="new"]')
    expect(button.classes()).toContain('custom-control')
    expect(button.attributes('aria-label')).toBe('New request')
    expect(button.attributes('title')).toBeUndefined()
    await button.trigger('focusin')
    await flushPromises()
    expect(document.body.textContent).toContain('Create a request')
    await button.trigger('click')
    expect(click).toHaveBeenCalledOnce()
  })
})
