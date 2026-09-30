import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { EditorView } from '@codemirror/view'
import CodeEditor from '../CodeEditor.vue'

beforeAll(() => {
  // jsdom lacks layout APIs CodeMirror calls during measure.
  const rect = () => ({
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    bottom: 0,
    right: 0,
    width: 0,
    height: 0,
    toJSON: () => ({}),
  })
  Range.prototype.getBoundingClientRect = rect as never
  Range.prototype.getClientRects = (() => ({
    length: 0,
    item: () => null,
    [Symbol.iterator]: [][Symbol.iterator],
  })) as never
})

const wrappers: ReturnType<typeof mount>[] = []
afterEach(() => wrappers.splice(0).forEach((w) => w.unmount()))

async function mountEditor(props: Record<string, unknown> = {}) {
  const wrapper = mount(CodeEditor, {
    attachTo: document.body,
    props: { modelValue: '{"a":1}', language: 'json', testId: 'ed', ...props },
  })
  wrappers.push(wrapper)
  await vi.waitFor(() => expect(wrapper.find('.cm-editor').exists()).toBe(true))
  const content = wrapper.find('.cm-content').element as HTMLElement
  return { wrapper, view: EditorView.findFromDOM(content)! }
}

describe('CodeEditor', () => {
  it('renders the model value', async () => {
    const { view } = await mountEditor()
    expect(view.state.doc.toString()).toBe('{"a":1}')
  })

  it('applies content attributes', async () => {
    const { wrapper } = await mountEditor({
      id: 'body',
      ariaLabelledby: 'lbl',
    })
    const content = wrapper.find('.cm-content')
    expect(content.attributes('id')).toBe('body')
    expect(content.attributes('aria-labelledby')).toBe('lbl')
    expect(content.attributes('data-testid')).toBe('ed')
  })

  it('emits update:modelValue on edit', async () => {
    const { wrapper, view } = await mountEditor()
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: '[]' },
    })
    expect(wrapper.emitted('update:modelValue')?.slice(-1)[0]).toEqual(['[]'])
  })

  it('replaces the document on external change without re-emitting', async () => {
    const { wrapper, view } = await mountEditor()
    await wrapper.setProps({ modelValue: '{\n  "a": 1\n}' })
    expect(view.state.doc.toString()).toBe('{\n  "a": 1\n}')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
  })

  it('keeps the selection when the external value is unchanged', async () => {
    const { wrapper, view } = await mountEditor()
    view.dispatch({ selection: { anchor: 3 } })
    await wrapper.setProps({ modelValue: '{"a":1}' })
    expect(view.state.selection.main.head).toBe(3)
  })

  it('is read-only while disabled', async () => {
    const { wrapper, view } = await mountEditor({ disabled: true })
    expect(view.state.readOnly).toBe(true)
    expect(wrapper.find('.cm-content').attributes('contenteditable')).toBe('false')
    await wrapper.setProps({ disabled: false })
    await flushPromises()
    expect(view.state.readOnly).toBe(false)
  })

  it('does not bind Mod-Enter or Mod-l', async () => {
    const { view } = await mountEditor()
    view.dispatch({ selection: { anchor: view.state.doc.length } })
    for (const key of ['Enter', 'l']) {
      view.contentDOM.dispatchEvent(
        new KeyboardEvent('keydown', {
          key,
          metaKey: true,
          ctrlKey: true,
          bubbles: true,
        }),
      )
    }
    expect(view.state.doc.toString()).toBe('{"a":1}')
    expect(view.state.selection.main.empty).toBe(true)
  })

  it('switches language without losing text', async () => {
    const { wrapper, view } = await mountEditor()
    await wrapper.setProps({ language: 'graphql' })
    expect(view.state.doc.toString()).toBe('{"a":1}')
  })

  it('stops context menu propagation', async () => {
    const { wrapper } = await mountEditor()
    let reached = false
    document.body.addEventListener('contextmenu', () => (reached = true), {
      once: true,
    })
    wrapper
      .find('.cm-content')
      .element.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
    expect(reached).toBe(false)
  })
  it('updates the placeholder when the prop changes', async () => {
    const { wrapper } = await mountEditor({
      modelValue: '',
      placeholder: 'json example',
    })
    expect(wrapper.find('.cm-placeholder').text()).toBe('json example')
    await wrapper.setProps({ placeholder: 'graphql example' })
    expect(wrapper.find('.cm-placeholder').text()).toBe('graphql example')
  })
  it('marks an error line and moves the cursor to it', async () => {
    const { wrapper, view } = await mountEditor({ modelValue: 'a\nbcd\ne' })
    ;(wrapper.vm as unknown as { markError(offset: number): void }).markError(4)
    await flushPromises()
    expect(view.state.selection.main.head).toBe(4)
    expect(view.hasFocus || document.activeElement === view.contentDOM).toBe(true)
    const marked = wrapper.findAll('.cm-errorLine')
    expect(marked).toHaveLength(1)
    expect(marked[0].text()).toBe('bcd')
  })
  it('clears the error line on the next edit', async () => {
    const { wrapper, view } = await mountEditor({ modelValue: 'a\nbcd' })
    ;(wrapper.vm as unknown as { markError(offset: number): void }).markError(3)
    await flushPromises()
    view.dispatch({ changes: { from: 0, insert: 'x' } })
    expect(wrapper.findAll('.cm-errorLine')).toHaveLength(0)
  })
})
