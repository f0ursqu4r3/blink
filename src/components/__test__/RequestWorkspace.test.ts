import { afterEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import RequestWorkspace from '../RequestWorkspace.vue'
import { createSession } from '@/lib/session'

const wrappers: ReturnType<typeof mount>[] = []

afterEach(() => wrappers.splice(0).forEach((wrapper) => wrapper.unmount()))

describe('request workspace panels', () => {
  it('resizes the request and response panels from an accessible separator', async () => {
    const workspace = mount(RequestWorkspace, {
      attachTo: document.body,
      props: { active: true, session: createSession() },
    })
    wrappers.push(workspace)

    const resize = workspace.get('[role="separator"][aria-label="Resize panels"]')
    expect(resize.attributes('aria-orientation')).toBe('vertical')
    const before = resize.attributes('aria-valuenow')
    await resize.trigger('keydown', { key: 'ArrowRight' })

    expect(resize.attributes('aria-valuenow')).not.toBe(before)
  })
})
