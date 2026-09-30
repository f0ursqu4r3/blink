import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import RequestWorkspace from '../RequestWorkspace.vue'
import { createSession } from '@/lib/session'
import { createGroup } from '@/lib/groups'

const wrappers: ReturnType<typeof mount>[] = []
afterEach(() => wrappers.splice(0).forEach((w) => w.unmount()))

// Stub fetch so useRequestRunner doesn't fail
vi.stubGlobal(
  'fetch',
  vi.fn(() => new Promise(() => {})),
)

describe('RequestWorkspace – resolved auth context', () => {
  it('effectiveAuth from group bearer auth is shown in RequestEditor without the token', async () => {
    const group = createGroup('Platform')
    group.localAuth = { type: 'bearer', token: 'secret-token-abc' }

    const session = createSession()
    session.groupId = group.id
    session.view.requestTab = 'auth' // start on auth tab so content is in DOM
    // localAuth undefined = inherit

    const wrapper = mount(RequestWorkspace, {
      attachTo: document.body,
      props: {
        active: true,
        session,
        groups: [group],
        globalDefinitions: {},
      },
    })
    wrappers.push(wrapper)
    await nextTick()

    // The effective auth note should appear in the auth tab content
    const note = wrapper.find("[data-testid='effective-auth-note']")
    expect(note.exists()).toBe(true)
    expect(note.text()).toContain('bearer')

    // The token value must not appear in the rendered HTML
    expect(wrapper.html()).not.toContain('secret-token-abc')
  })

  it('send() includes Authorization header from group bearer auth', async () => {
    const capturedRequests: Array<{ url: string; init: RequestInit }> = []

    vi.stubGlobal(
      'fetch',
      vi.fn((url: string, init: RequestInit) => {
        capturedRequests.push({ url, init })
        return new Promise(() => {}) // never resolves (ok for test)
      }),
    )

    const group = createGroup('Workspace')
    group.localAuth = { type: 'bearer', token: 'my-bearer-token' }

    const session = createSession()
    session.groupId = group.id
    session.draft.url = 'https://api.example.com/test'
    // localAuth undefined = inherit from group

    const wrapper = mount(RequestWorkspace, {
      attachTo: document.body,
      props: {
        active: true,
        session,
        groups: [group],
        globalDefinitions: {},
      },
    })
    wrappers.push(wrapper)

    await nextTick()

    // Click the send button
    const sendBtn = wrapper.find('[data-send]')
    expect(sendBtn.exists()).toBe(true)
    await sendBtn.trigger('click')
    await nextTick()

    // fetch should have been called with Authorization header
    expect(capturedRequests.length).toBeGreaterThan(0)
    const { init } = capturedRequests[0]
    const headers = init.headers as Headers
    const authHeader = headers.get('authorization')
    expect(authHeader).toBe('Bearer my-bearer-token')
  })
})
