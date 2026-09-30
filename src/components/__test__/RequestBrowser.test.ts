import { afterEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, mount } from '@vue/test-utils'
import RequestBrowser from '../RequestBrowser.vue'
import { createSession } from '@/lib/session'
enableAutoUnmount(afterEach)

describe('request browser', () => {
  it('shows only the focused group and its descendants', async () => {
    const loose = createSession()
    loose.draft.url = 'https://api.example.test/loose'
    const inside = createSession()
    inside.groupId = 2
    inside.draft.url = 'https://api.example.test/inside'
    const nested = createSession()
    nested.groupId = 3
    nested.draft.url = 'https://api.example.test/nested'
    const other = createSession()
    other.groupId = 4
    other.draft.url = 'https://api.example.test/other'
    const unfocus = vi.fn()
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [loose, inside, nested, other],
        activeId: inside.id,
        groups: [
          { id: 1, name: 'Platform', parentId: null, collapsed: false },
          { id: 2, name: 'Identity', parentId: 1, collapsed: true },
          { id: 3, name: 'Sessions', parentId: 2, collapsed: false },
          { id: 4, name: 'Billing', parentId: null, collapsed: false },
        ],
        focusedGroupId: 2,
        onUnfocus: unfocus,
      },
    })

    const text = browser.get('[data-browser-list]').text()
    // A collapsed group shows expanded while it is focused.
    expect(text).toContain('Identity')
    expect(text).toContain('/inside')
    expect(text).toContain('Sessions')
    expect(text).toContain('/nested')
    expect(text).not.toContain('Platform')
    expect(text).not.toContain('Billing')
    expect(text).not.toContain('/other')
    expect(text).not.toContain('/loose')
    expect(text).not.toContain('UNGROUPED')
    expect(browser.get('[data-browser-focus]').text()).toContain('Identity')

    await browser.get('[aria-label="Unfocus Identity"]').trigger('click')
    expect(unfocus).toHaveBeenCalled()
  })

  it('focuses a group from its menu', async () => {
    const focus = vi.fn()
    const browser = mount(RequestBrowser, {
      global: { stubs: { DropdownMenuPortal: { template: '<slot />' } } },
      props: {
        sessions: [createSession()],
        activeId: null,
        groups: [{ id: 1, name: 'Platform', parentId: null, collapsed: false }],
        onFocusGroup: focus,
      },
    })

    await browser
      .get('[aria-label="More actions for Platform"]')
      .trigger('keydown', { key: 'Enter' })
    await browser
      .findAll('[role="menuitem"]')
      .find((item) => item.text() === 'Focus')!
      .trigger('click')

    expect(focus).toHaveBeenCalledWith(1)
  })

  it('renders nested groups and moves the active request into a selected group', async () => {
    const active = createSession()
    active.draft.url = 'https://api.example.test/users'
    const nested = createSession()
    nested.groupId = 2
    const moveRequest = vi.fn()
    const browser = mount(RequestBrowser, {
      global: { stubs: { DropdownMenuPortal: { template: '<slot />' } } },
      props: {
        sessions: [active, nested],
        activeId: active.id,
        groups: [
          { id: 1, name: 'Platform', parentId: null, collapsed: false },
          { id: 2, name: 'Identity', parentId: 1, collapsed: false },
        ],
        selectedIds: [active.id],
        onMoveRequest: moveRequest,
      },
    })

    expect(browser.text()).toContain('Platform')
    expect(browser.text()).toContain('Identity')
    expect(browser.text()).toContain('/users')
    await browser
      .get('[aria-label="More actions for Identity"]')
      .trigger('keydown', { key: 'Enter' })
    const move = browser
      .findAll('[role="menuitem"]')
      .find((item) => item.text() === 'Move selection here')!
    await move.trigger('click')

    expect(moveRequest).toHaveBeenCalledWith(active.id, 2)
  })

  it('starts a nested group from the selected parent', async () => {
    const active = createSession()
    const createGroup = vi.fn()
    const browser = mount(RequestBrowser, {
      global: { stubs: { DropdownMenuPortal: { template: '<slot />' } } },
      props: {
        sessions: [active],
        activeId: active.id,
        groups: [{ id: 1, name: 'Platform', parentId: null, collapsed: false }],
        onCreateGroup: createGroup,
      },
    })

    await browser
      .get('[aria-label="More actions for Platform"]')
      .trigger('keydown', { key: 'Enter' })
    await browser.get('[aria-label="Add group inside Platform"]').trigger('click')
    await browser.get('[aria-label="Group name in Platform"]').setValue('Identity')
    await browser.get('.child-form').trigger('submit')

    expect(createGroup).toHaveBeenCalledWith('Identity', 1)
  })

  it('selects a request range', async () => {
    const first = createSession()
    first.draft.url = 'https://api.example.test/first'
    const second = createSession()
    second.draft.url = 'https://api.example.test/second'
    const third = createSession()
    third.draft.url = 'https://api.example.test/third'
    const updateSelection = vi.fn()
    const browser = mount(RequestBrowser, {
      props: {
        sessions: [first, second, third],
        activeId: first.id,
        selectedIds: [first.id],
        selectionAnchorId: first.id,
        groups: [{ id: 1, name: 'Platform', parentId: null, collapsed: false }],
        onUpdateSelection: updateSelection,
      },
    })

    await browser.get(`[data-request-id="${third.id}"]`).trigger('click', {
      shiftKey: true,
    })
    expect(updateSelection).toHaveBeenCalledWith([first.id, second.id, third.id], first.id)
  })
})
