// Writes workspace snapshots with the TS app's encoder, for the Rust
// compatibility tests. Run from the repository root:
//   bun crates/blink-core/tests/fixtures/workspace/generate.ts
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { pair } from '../../../../../src/lib/request'
import { createSession, draftFingerprint } from '../../../../../src/lib/session'
import { createGroup } from '../../../../../src/lib/groups'
import { createEnvironment } from '../../../../../src/lib/environments'
import { createAssertion, createCapture } from '../../../../../src/lib/checks'
import { defaultPreferences } from '../../../../../src/lib/preferences'
import { decodeWorkspace, encodeWorkspace } from '../../../../../src/lib/workspace'

const dir = import.meta.dir

/** Each fixture is the input snapshot and the TS round trip of it. */
function write(name: string, input: string) {
  const decoded = decodeWorkspace(input)
  const roundTrip = encodeWorkspace(
    decoded.sessions,
    decoded.activeId,
    decoded.groups,
    decoded.globalDefinitions,
    decoded.preferences,
    decoded.openIds,
  )
  // Compact, exactly as the app saves them.
  writeFileSync(join(dir, `${name}.json`), input)
  writeFileSync(join(dir, `${name}.roundtrip.json`), roundTrip)
}

function full() {
  const api = createGroup('API')
  const users = createGroup('Users', api.id)
  const dev = { ...createEnvironment('DEV', 'info'), values: { host: 'dev.example.test' } }
  const prod = {
    ...createEnvironment('PROD', 'destructive'),
    protected: true,
    values: { host: 'example.test', key: 'p' },
  }
  api.localAuth = { type: 'bearer', token: '{{key}}' }
  api.localDefinitions = { host: 'base.example.test', version: 'v1' }
  api.environments = [dev, prod]
  api.activeEnvironmentId = dev.id
  api.defaultMethod = 'POST'
  api.defaultUrl = 'https://{{host}}/{{version}}'
  users.localDefinitions = { version: 'v2' }
  users.collapsed = true
  const empty = createGroup('Empty')
  empty.activeEnvironmentId = null

  const json = createSession()
  json.groupId = users.id
  json.draft.method = 'POST'
  json.draft.url = 'https://{{host}}/users?page=1'
  json.draft.query = [pair('filter', 'two words'), { ...pair('off', 'x'), enabled: false }]
  json.draft.headers.push(pair('X-Trace', 'é ✓ "quoted"\ttab'))
  json.draft.bodyMode = 'json'
  json.draft.body = '{"id":9223372036854775807,"price":0.1}'
  json.draft.localAuth = { type: 'basic', username: 'alice', password: 's3cr3t' }
  json.draft.assertions = [
    createAssertion(),
    { ...createAssertion(), source: 'json', path: '.id', operator: 'exists', expected: '' },
  ]
  json.draft.captures = [{ ...createCapture(), name: 'userId', path: '.id' }]
  json.view = { requestTab: 'body', responseTab: 'headers', pretty: false, wrap: true, responseScroll: 250.5 }
  json.response = {
    status: 201,
    statusText: 'Created',
    durationMs: 12.75,
    headers: [{ key: 'Content-Type', value: 'application/json' }],
    body: '{"id":1}',
    sizeBytes: 8,
    bodyId: 'body-1',
    finalUrl: 'https://example.test/users/1',
    redirectCount: 1,
    timing: { dnsMs: 1, connectMs: 2.5, tlsMs: 3, waitMs: 4, downloadMs: 0.25 },
  }
  json.sentFingerprint = draftFingerprint(json.draft)
  json.history = [
    {
      id: 2,
      sentAt: 1_700_000_000_500,
      method: 'POST',
      url: 'https://example.test/users?page=1',
      status: 201,
      statusText: 'Created',
      durationMs: 12,
      sizeBytes: 8,
      headers: [{ key: 'a', value: '1' }],
      body: '{"id":1}',
      timing: { waitMs: 10, downloadMs: 2 },
    },
    {
      id: 1,
      sentAt: 1_700_000_000_000,
      method: 'POST',
      url: 'https://example.test/users?page=1',
      error: 'Connection refused',
      durationMs: 3,
      sizeBytes: 0,
      headers: [],
      body: '',
    },
  ]

  const form = createSession()
  form.groupId = api.id
  form.draft.method = 'PUT'
  form.draft.url = 'https://{{host}}/upload'
  form.draft.bodyMode = 'multipart'
  form.draft.form = [pair('title', 'Ada'), { ...pair('upload', '/tmp/photo.png'), file: true }]
  form.draft.localAuth = { type: 'none' }
  form.busy = true
  form.response = {
    status: 200,
    statusText: 'OK',
    durationMs: 1,
    headers: [],
    body: 'x'.repeat(64),
    sizeBytes: 5 * 1024 * 1024,
    bodyId: 'body-2',
    truncated: true,
    binary: false,
  }

  const graphql = createSession()
  graphql.draft.method = 'POST'
  graphql.draft.url = 'https://example.test/graphql'
  graphql.draft.bodyMode = 'graphql'
  graphql.draft.body = '{ viewer { id } }'
  graphql.draft.variables = '{"first":10}'
  graphql.draft.auth = 'bearer'
  graphql.draft.token = 'flat-token'
  graphql.error = 'Previous send failed.'
  graphql.history = []

  const file = createSession()
  file.draft.method = 'PURGE'
  file.draft.url = 'https://example.test/cache'
  file.draft.bodyMode = 'file'
  file.draft.bodyFile = '/tmp/a.bin'
  file.response = {
    status: 204,
    statusText: 'No Content',
    durationMs: 0,
    headers: [],
    body: '',
    sizeBytes: 0,
    binary: true,
  }

  const preferences = {
    ...defaultPreferences(),
    defaultMethod: 'PATCH',
    defaultBodyMode: 'json' as const,
    wrap: true,
    paneLayout: 'vertical' as const,
    codeTarget: 'python' as const,
    zoom: 1.25,
    timeoutSeconds: 90,
    followRedirects: true,
    proxyUrl: 'http://127.0.0.1:8080',
  }
  return encodeWorkspace(
    [json, form, graphql, file],
    form.id,
    [api, users, empty],
    { apiVersion: 'v2', _private: 'yes' },
    preferences,
    [graphql.id, form.id],
  )
}

function noTabs() {
  const session = createSession()
  return encodeWorkspace([session], null, [], {}, defaultPreferences(), [])
}

/** A legacy snapshot: the v4 shape with fields the old version did not have removed. */
function legacy(version: 1 | 2 | 3) {
  const first = createSession()
  first.draft.url = 'https://example.test/users'
  first.draft.auth = 'bearer'
  first.draft.token = 'legacy-token'
  first.response = {
    status: 200,
    statusText: 'OK',
    durationMs: 8,
    headers: [],
    body: '{}',
    sizeBytes: 2,
  }
  first.sentFingerprint = draftFingerprint(first.draft)
  const second = createSession()
  second.draft.url = 'https://example.test/{{missing}}'
  second.draft.auth = 'basic'
  second.draft.username = 'bob'
  second.draft.password = 'pw'
  second.response = { ...first.response }
  second.sentFingerprint = 'stale'
  second.busy = true
  const group = { id: 7, name: 'Legacy', parentId: null, collapsed: false }
  const raw = JSON.parse(encodeWorkspace([first, second], second.id, [group], {}))
  raw.version = version
  delete raw.openIds
  if (version < 3) {
    delete raw.globalDefinitions
    delete raw.preferences
  }
  if (version === 1) {
    delete raw.groups
    for (const tab of raw.tabs) delete tab.groupId
  } else raw.tabs[0].groupId = group.id
  return JSON.stringify(raw)
}

write('v4-full', full())
write('v4-no-tabs', noTabs())
write('v3', legacy(3))
write('v2', legacy(2))
write('v1', legacy(1))
