import { describe, expect, it } from 'vitest'
import { countRequests, importHttp, parseImport } from '../import'

const openapi = `openapi: 3.0.0
info:
  title: Pet Store
servers:
  - url: https://{env}.pets.test/v1/
    variables:
      env:
        default: api
components:
  securitySchemes:
    auth:
      type: http
      scheme: bearer
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: string
          example: Rex
        age:
          type: integer
paths:
  /pets/{petId}:
    parameters:
      - name: petId
        in: path
        required: true
        schema:
          type: integer
          example: 7
    get:
      tags: [pets]
      parameters:
        - name: expand
          in: query
          schema:
            type: string
        - name: X-Trace
          in: header
          required: true
          schema:
            type: string
    put:
      tags: [pets]
      requestBody:
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/Pet"
  /health:
    get: {}
`

describe('OpenAPI import', () => {
  it('builds tag groups, tokens, params, and bodies', async () => {
    const { format, root, skipped } = await parseImport(openapi)
    expect(format).toBe('openapi')
    expect(skipped).toEqual(['Bearer authentication: set a token in the group settings.'])
    expect(root.name).toBe('Pet Store')
    expect(root.definitions).toEqual({
      baseUrl: 'https://api.pets.test/v1',
      petId: '7',
    })
    expect(root.auth).toBeUndefined()
    expect(root.requests.map((r) => r.url)).toEqual(['{{baseUrl}}/health'])
    const [get, put] = root.groups[0].requests
    expect(root.groups[0].name).toBe('pets')
    expect(get.url).toBe('{{baseUrl}}/pets/{{petId}}')
    expect(get.query).toMatchObject([{ key: 'expand', enabled: false }])
    expect(get.headers.map((h) => [h.key, h.enabled])).toEqual([
      ['Accept', true],
      ['X-Trace', true],
    ])
    expect(put.method).toBe('PUT')
    expect(put.bodyMode).toBe('json')
    expect(JSON.parse(put.body)).toEqual({ name: 'Rex', age: 0 })
    expect(countRequests(root)).toBe(3)
  })
  it('reads Swagger 2 JSON', async () => {
    const { root } = await parseImport(
      JSON.stringify({
        swagger: '2.0',
        info: { title: 'Old' },
        host: 'old.test',
        basePath: '/api',
        schemes: ['http'],
        paths: { '/a': { post: {} } },
      }),
    )
    expect(root.definitions?.baseUrl).toBe('http://old.test/api')
    expect(root.requests[0].method).toBe('POST')
  })
})

describe('Postman import', () => {
  it('builds folders, variables, auth, and bodies', async () => {
    const { format, root, skipped } = await parseImport(
      JSON.stringify({
        info: {
          name: 'Shop',
          schema: 'https://schema.getpostman.com/json/collection/v2.1.0/collection.json',
        },
        variable: [{ key: 'host', value: 'https://shop.test' }],
        auth: { type: 'bearer', bearer: [{ key: 'token', value: '{{tok}}' }] },
        item: [
          {
            name: 'Orders',
            item: [
              {
                name: 'Create',
                request: {
                  method: 'POST',
                  url: { raw: '{{host}}/orders' },
                  header: [
                    { key: 'X-A', value: '1' },
                    { key: 'X-B', value: '2', disabled: true },
                  ],
                  body: {
                    mode: 'raw',
                    raw: '{"a":1}',
                    options: { raw: { language: 'json' } },
                  },
                },
              },
              {
                name: 'Upload',
                request: {
                  method: 'POST',
                  url: '{{host}}/upload',
                  body: {
                    mode: 'formdata',
                    formdata: [
                      { key: 'note', value: 'x', type: 'text' },
                      { key: 'file', type: 'file', src: '/a' },
                    ],
                  },
                  auth: { type: 'noauth' },
                },
              },
            ],
          },
          { name: 'Ping', request: 'https://shop.test/ping' },
        ],
      }),
    )
    expect(format).toBe('postman')
    expect(root.name).toBe('Shop')
    expect(root.definitions).toEqual({ host: 'https://shop.test' })
    expect(root.auth).toEqual({ type: 'bearer', token: '{{tok}}' })
    expect(root.requests[0].url).toBe('https://shop.test/ping')
    const [create, upload] = root.groups[0].requests
    expect(create.bodyMode).toBe('json')
    expect(create.headers.map((h) => h.enabled)).toEqual([true, false])
    expect(upload.bodyMode).toBe('multipart')
    expect(upload.localAuth).toEqual({ type: 'none' })
    expect(skipped).toEqual(['Upload: file fields need a file'])
  })
})

describe('.http import', () => {
  it('reads requests, headers, bodies, and file variables', () => {
    const { root } = importHttp(
      `@host = https://api.test
# comment
GET {{host}}/users
    ?page=2
Accept: application/json

### Create
POST {{host}}/users HTTP/1.1
Content-Type: application/json

{
  "name": "Ada"
}

###
https://api.test/plain
`,
      'users',
    )
    expect(root.name).toBe('users')
    expect(root.definitions).toEqual({ host: 'https://api.test' })
    expect(root.requests.map((r) => [r.method, r.url])).toEqual([
      ['GET', '{{host}}/users?page=2'],
      ['POST', '{{host}}/users'],
      ['GET', 'https://api.test/plain'],
    ])
    expect(root.requests[1].bodyMode).toBe('json')
    expect(JSON.parse(root.requests[1].body)).toEqual({ name: 'Ada' })
  })
  it('rejects files without requests', async () => {
    await expect(parseImport('hello world')).rejects.toThrow('No requests')
    await expect(parseImport('{"a":1}')).rejects.toThrow('can import')
  })
})
