import { createDraft, isMethod, pair, type BodyMode, type Draft } from './request'
import type { AuthorizationConfig } from './authorization'

/** A group tree with requests, ready to add to the workspace. */
export type ImportedGroup = {
  name: string
  definitions?: Record<string, string>
  auth?: AuthorizationConfig
  groups: ImportedGroup[]
  requests: Draft[]
}
export type ImportFormat = 'openapi' | 'postman' | 'http'
export type ImportResult = {
  format: ImportFormat
  root: ImportedGroup
  /** Items skipped, with the reason. */
  skipped: string[]
}

const MAX_NAME = 80
const groupName = (name: unknown, fallback: string) =>
  (typeof name === 'string' && name.trim() ? name.trim() : fallback).slice(0, MAX_NAME)
const object = (value: unknown): Record<string, unknown> =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
const list = (value: unknown): unknown[] => (Array.isArray(value) ? value : [])
const str = (value: unknown) =>
  typeof value === 'string'
    ? value
    : typeof value === 'number' || typeof value === 'boolean'
      ? String(value)
      : ''

export function countRequests(group: ImportedGroup): number {
  return group.requests.length + group.groups.reduce((sum, child) => sum + countRequests(child), 0)
}

function draft(
  method: string,
  url: string,
  headers: [string, string, boolean?][] = [],
  body?: { mode: BodyMode; text?: string; form?: [string, string][] },
): Draft {
  const result = createDraft()
  result.method = isMethod(method.toUpperCase()) ? method.toUpperCase() : 'GET'
  result.url = url
  result.headers = headers.length
    ? headers.map(([key, value, enabled = true]) => ({
        ...pair(key, value),
        enabled,
      }))
    : [pair()]
  if (body) {
    result.bodyMode = body.mode
    result.body = body.text ?? ''
    if (body.form) result.form = body.form.map(([key, value]) => pair(key, value))
  }
  return result
}

// ── OpenAPI 3 and Swagger 2 ─────────────────────────────────────────────────

const OPERATIONS = ['get', 'put', 'post', 'delete', 'options', 'head', 'patch', 'trace']

function exampleOf(schema: Record<string, unknown>, depth = 0): unknown {
  if (depth > 6) return null
  if ('example' in schema) return schema.example
  if ('default' in schema) return schema.default
  const type = schema.type
  if (Array.isArray(schema.enum) && schema.enum.length) return schema.enum[0]
  if (type === 'object' || schema.properties) {
    const result: Record<string, unknown> = {}
    for (const [key, value] of Object.entries(object(schema.properties)))
      result[key] = exampleOf(object(value), depth + 1)
    return result
  }
  if (type === 'array') return [exampleOf(object(schema.items), depth + 1)]
  if (type === 'integer' || type === 'number') return 0
  if (type === 'boolean') return false
  if (type === 'string') return ''
  return null
}

/** Resolve a local `$ref` such as `#/components/schemas/User`. */
function resolver(spec: Record<string, unknown>) {
  return function resolve(value: unknown, seen = new Set<string>()): Record<string, unknown> {
    const item = object(value)
    const ref = item.$ref
    if (typeof ref !== 'string' || !ref.startsWith('#/') || seen.has(ref)) return item
    seen.add(ref)
    let target: unknown = spec
    for (const part of ref.slice(2).split('/'))
      target = object(target)[part.replace(/~1/g, '/').replace(/~0/g, '~')]
    return resolve(target, seen)
  }
}

function importOpenApi(spec: Record<string, unknown>): ImportResult {
  const resolve = resolver(spec)
  const info = object(spec.info)
  const skipped: string[] = []
  let baseUrl = ''
  if (spec.swagger) {
    const scheme = str(list(spec.schemes)[0]) || 'https'
    baseUrl = spec.host ? `${scheme}://${str(spec.host)}${str(spec.basePath)}` : ''
  } else {
    const server = object(list(spec.servers)[0])
    baseUrl = str(server.url)
    for (const [name, variable] of Object.entries(object(server.variables)))
      baseUrl = baseUrl.split(`{${name}}`).join(str(object(variable).default))
  }
  baseUrl = baseUrl.replace(/\/$/, '')
  const definitions: Record<string, string> = { baseUrl }
  const root: ImportedGroup = {
    name: groupName(info.title, 'OpenAPI import'),
    definitions,
    groups: [],
    requests: [],
  }
  const byTag = new Map<string, ImportedGroup>()
  const bearer = Object.values(object(object(spec.components).securitySchemes)).some((scheme) => {
    const value = object(scheme)
    return value.type === 'http' && str(value.scheme).toLowerCase() === 'bearer'
  })
  // An empty token would fail every send, so auth stays for the user to set.
  if (bearer) skipped.push('Bearer authentication: set a token in the group settings.')

  for (const [path, pathItem] of Object.entries(object(spec.paths))) {
    const shared = list(object(pathItem).parameters)
    for (const method of OPERATIONS) {
      if (!(method in object(pathItem))) continue
      const operation = object(object(pathItem)[method])
      const parameters = [...shared, ...list(operation.parameters)].map((p) => resolve(p))
      const query: [string, string, boolean][] = []
      const headers: [string, string, boolean?][] = []
      let url = `{{baseUrl}}${path}`
      for (const parameter of parameters) {
        const name = str(parameter.name)
        if (!name) continue
        const example = str(parameter.example ?? exampleOf(resolve(parameter.schema ?? parameter)))
        if (parameter.in === 'path') {
          // {id} becomes a token defined on the group.
          const token = name.replace(/[^\w.-]/g, '_')
          url = url.split(`{${name}}`).join(`{{${token}}}`)
          if (!(token in definitions)) definitions[token] = example
        } else if (parameter.in === 'query')
          query.push([name, example, parameter.required === true])
        else if (parameter.in === 'header')
          headers.push([name, example, parameter.required === true])
      }
      let body: Parameters<typeof draft>[3]
      const content = object(resolve(operation.requestBody).content)
      const json = Object.entries(content).find(([type]) => /json/i.test(type))
      const form = content['application/x-www-form-urlencoded']
      const swaggerBody = parameters.find((p) => p.in === 'body')
      if (json || swaggerBody) {
        const media = object(json?.[1])
        const schema = resolve(media.schema ?? swaggerBody?.schema)
        const example =
          media.example ??
          object(Object.values(object(media.examples))[0]).value ??
          exampleOf(schema)
        body = { mode: 'json', text: JSON.stringify(example ?? {}, null, 2) }
      } else if (form) {
        const schema = resolve(object(form).schema)
        body = {
          mode: 'form',
          form: Object.keys(object(schema.properties)).map((key) => [key, '']),
        }
      }
      const request = draft(method, url, [['Accept', 'application/json'], ...headers], body)
      request.query = query.length
        ? query.map(([key, value, enabled]) => ({
            ...pair(key, value),
            enabled,
          }))
        : [pair()]
      const tag = str(list(operation.tags)[0])
      let group = root
      if (tag) {
        group = byTag.get(tag) ?? {
          name: groupName(tag, 'Untagged'),
          groups: [],
          requests: [],
        }
        if (!byTag.has(tag)) {
          byTag.set(tag, group)
          root.groups.push(group)
        }
      }
      group.requests.push(request)
    }
  }
  if (!baseUrl) skipped.push('No server URL. Set baseUrl in the group settings.')
  return { format: 'openapi', root, skipped }
}

// ── Postman collection v2.0 / v2.1 ──────────────────────────────────────────

function postmanAuth(value: unknown): AuthorizationConfig | undefined {
  const auth = object(value)
  const entries = (type: string) =>
    Object.fromEntries(
      list(auth[type]).map((entry) => [str(object(entry).key), str(object(entry).value)]),
    )
  if (auth.type === 'noauth') return { type: 'none' }
  if (auth.type === 'bearer') return { type: 'bearer', token: entries('bearer').token ?? '' }
  if (auth.type === 'basic') {
    const basic = entries('basic')
    return {
      type: 'basic',
      username: basic.username ?? '',
      password: basic.password ?? '',
    }
  }
  return undefined
}

function postmanUrl(value: unknown) {
  if (typeof value === 'string') return value
  const url = object(value)
  if (url.raw) return str(url.raw)
  const host = list(url.host).map(str).join('.')
  const path = list(url.path).map(str).join('/')
  const protocol = str(url.protocol) || 'https'
  return `${protocol}://${host}${path ? `/${path}` : ''}`
}

function importPostman(collection: Record<string, unknown>): ImportResult {
  const skipped: string[] = []
  const info = object(collection.info)
  const definitions = Object.fromEntries(
    list(collection.variable)
      .map((entry) => [str(object(entry).key), str(object(entry).value)])
      .filter(([key]) => key && !key.startsWith('_')),
  )
  function folder(
    name: string,
    items: unknown[],
    auth: unknown,
    variables?: Record<string, string>,
  ): ImportedGroup {
    const group: ImportedGroup = {
      name: groupName(name, 'Postman import'),
      groups: [],
      requests: [],
    }
    if (variables && Object.keys(variables).length) group.definitions = variables
    const groupAuth = postmanAuth(auth)
    if (groupAuth) group.auth = groupAuth
    for (const raw of items) {
      const item = object(raw)
      if (Array.isArray(item.item)) {
        group.groups.push(folder(str(item.name), item.item, item.auth))
        continue
      }
      const request =
        typeof item.request === 'string'
          ? { url: item.request, method: 'GET' }
          : object(item.request)
      const url = postmanUrl(request.url)
      if (!url) {
        skipped.push(`${str(item.name) || 'Request'}: no URL`)
        continue
      }
      const headers = list(request.header).map((entry) => {
        const header = object(entry)
        return [str(header.key), str(header.value), header.disabled !== true] as [
          string,
          string,
          boolean,
        ]
      })
      const source = object(request.body)
      let body: Parameters<typeof draft>[3]
      if (source.mode === 'raw') {
        const language = str(object(object(source.options).raw).language)
        const text = str(source.raw)
        let json = language === 'json'
        if (!language)
          try {
            JSON.parse(text)
            json = true
          } catch {
            json = false
          }
        body = { mode: json ? 'json' : 'text', text }
      } else if (source.mode === 'urlencoded' || source.mode === 'formdata') {
        const rows = list(source[source.mode]).map((entry) => object(entry))
        if (rows.some((row) => row.type === 'file'))
          skipped.push(`${str(item.name)}: file fields need a file`)
        body = {
          mode: source.mode === 'urlencoded' ? 'form' : 'multipart',
          form: rows
            .filter((row) => row.disabled !== true)
            .map((row) => [str(row.key), row.type === 'file' ? '' : str(row.value)]),
        }
      } else if (source.mode === 'graphql') {
        const graphql = object(source.graphql)
        body = { mode: 'graphql', text: str(graphql.query) }
      }
      const result = draft(str(request.method) || 'GET', url, headers, body)
      if (source.mode === 'graphql') result.variables = str(object(source.graphql).variables)
      const auth = postmanAuth(request.auth)
      if (auth) result.localAuth = auth
      group.requests.push(result)
    }
    return group
  }
  const root = folder(str(info.name), list(collection.item), collection.auth, definitions)
  return { format: 'postman', root, skipped }
}

// ── .http / .rest files (JetBrains HTTP Client, VS Code REST Client) ───────

// "GET https://…", "POST {{host}}/…", or a bare URL. HTTP/1.1 is optional.
const REQUEST_LINE = /^(?:([A-Z]+)\s+)?((?:https?:\/\/|\{\{)\S*)(?:\s+HTTP\/[\d.]+)?\s*$/

export function importHttp(text: string, name = 'HTTP file'): ImportResult {
  const root: ImportedGroup = {
    name: groupName(name, 'HTTP file'),
    groups: [],
    requests: [],
  }
  const definitions: Record<string, string> = {}
  const skipped: string[] = []
  const blocks = text.replace(/\r\n?/g, '\n').split(/^###.*$/m)
  for (const block of blocks) {
    const lines = block.split('\n')
    let index = 0
    // File variables and comments before the request line.
    for (; index < lines.length; index++) {
      const line = lines[index].trim()
      const variable = /^@([\w.-]+)\s*=\s*(.*)$/.exec(line)
      if (variable) definitions[variable[1]] = variable[2].trim()
      else if (line && !line.startsWith('#') && !line.startsWith('//')) break
    }
    if (index >= lines.length) continue
    const match = REQUEST_LINE.exec(lines[index].trim())
    if (!match) {
      skipped.push(`Unknown request line: ${lines[index].trim().slice(0, 60)}`)
      continue
    }
    let url = match[2]
    const method = match[1] ?? 'GET'
    index++
    // Query continuation lines, such as "    ?page=2" or "    &size=10".
    while (index < lines.length && /^\s+[?&]/.test(lines[index])) url += lines[index++].trim()
    const headers: [string, string][] = []
    for (; index < lines.length && lines[index].trim(); index++) {
      const header = /^([^:\s]+)\s*:\s*(.*)$/.exec(lines[index].trim())
      if (header) headers.push([header[1], header[2]])
    }
    const body = lines
      .slice(index + 1)
      .join('\n')
      .trim()
    const contentType = headers.find(([key]) => key.toLowerCase() === 'content-type')?.[1] ?? ''
    const mode: BodyMode = /json/i.test(contentType) ? 'json' : 'text'
    root.requests.push(draft(method, url, headers, body ? { mode, text: body } : undefined))
  }
  if (Object.keys(definitions).length) root.definitions = definitions
  return { format: 'http', root, skipped }
}

/** Parse a file as OpenAPI (JSON or YAML), Postman, or an .http file. */
export async function parseImport(text: string, fileName = ''): Promise<ImportResult> {
  const trimmed = text.trim()
  if (!trimmed) throw new Error('The file is empty.')
  let data: unknown
  if (trimmed.startsWith('{')) {
    try {
      data = JSON.parse(trimmed)
    } catch {
      throw new Error('The file is not valid JSON.')
    }
  } else if (/^(openapi|swagger)\s*:/m.test(trimmed)) {
    const { parse } = await import('yaml')
    try {
      data = parse(trimmed)
    } catch {
      throw new Error('The file is not valid YAML.')
    }
  }
  const spec = object(data)
  if (spec.openapi || spec.swagger) return importOpenApi(spec)
  if (object(spec.info).schema || (object(spec.info).name && spec.item)) return importPostman(spec)
  if (data !== undefined)
    throw new Error('Blink can import OpenAPI, Postman collections, and .http files.')
  const result = importHttp(text, fileName.replace(/\.(http|rest)$/i, ''))
  if (!result.root.requests.length)
    throw new Error(
      'No requests found. Blink can import OpenAPI, Postman collections, and .http files.',
    )
  return result
}
