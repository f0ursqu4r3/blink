import { describe, expect, it } from 'vitest'
import type { RequestGroup } from '../groups'
import { createSession } from '../session'
import { sessionCurl } from '../session-curl'

describe('sessionCurl', () => {
  it('builds a cURL command with inherited authorization', () => {
    const session = createSession()
    session.draft.url = 'https://api.example.test/users'
    session.groupId = 1
    const groups: RequestGroup[] = [
      {
        id: 1,
        name: 'API',
        parentId: null,
        collapsed: false,
        localAuth: { type: 'bearer', token: 'abc' },
      },
    ]
    const command = sessionCurl(session, groups, {})
    expect(command.startsWith('curl')).toBe(true)
    expect(command).toContain('https://api.example.test/users')
    expect(command).toContain('Bearer abc')
  })

  it('returns an empty string when the draft does not build', () => {
    const session = createSession()
    session.draft.url = 'not a url'
    expect(sessionCurl(session, [], {})).toBe('')
  })
})
