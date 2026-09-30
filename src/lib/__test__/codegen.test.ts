import { describe, expect, it } from 'vitest'
import { codeTargets, generateCode, isCodeTarget } from '../codegen'
import { toCurl, type RequestInput } from '../request'
import { defaultTransportOptions } from '../transport-options'

const post: RequestInput = {
  method: 'POST',
  url: 'https://api.example.test/items?q=a%20b',
  headers: [
    { key: 'Content-Type', value: 'application/json' },
    { key: 'Authorization', value: "Bearer it's" },
  ],
  body: '{"name":"line\nbreak"}',
}
const multipart: RequestInput = {
  method: 'POST',
  url: 'https://api.example.test/upload',
  headers: [],
  body: null,
  multipart: [
    { key: 'note', value: 'hello', file: false },
    { key: 'file', value: '/tmp/a b.png', file: true },
  ],
}

describe('generateCode', () => {
  it('reuses toCurl for cURL', () => {
    expect(generateCode('curl', post)).toBe(toCurl(post))
  })
  it('knows every target id', () => {
    for (const target of codeTargets) expect(isCodeTarget(target.id)).toBe(true)
    expect(isCodeTarget('perl')).toBe(false)
  })
  it('writes fetch with headers, body, and timeout', () => {
    const code = generateCode('fetch', post)
    expect(code).toContain('await fetch("https://api.example.test/items?q=a%20b"')
    expect(code).toContain('["Authorization", "Bearer it\'s"]')
    expect(code).toContain('body: "{\\"name\\":\\"line\\nbreak\\"}"')
    expect(code).toContain('redirect: "manual"')
    expect(code).toContain('AbortSignal.timeout(30000)')
  })
  it('writes fetch multipart with openAsBlob', () => {
    const code = generateCode('fetch', multipart)
    expect(code).toContain('import { openAsBlob } from "node:fs";')
    expect(code).toContain('form.append("note", "hello");')
    expect(code).toContain('form.append("file", await openAsBlob("/tmp/a b.png"), "a b.png");')
  })
  it('writes Python with transport options', () => {
    const code = generateCode('python', post, {
      ...defaultTransportOptions(),
      verifyTls: false,
      followRedirects: true,
      proxyUrl: 'http://proxy.test:8080',
    })
    expect(code).toContain('session.max_redirects = 10')
    expect(code).toContain('verify=False')
    expect(code).toContain('"https": "http://proxy.test:8080"')
    expect(code).toContain('allow_redirects=True')
    expect(code).toContain('timeout=(10, 30)')
  })
  it('writes Python multipart parts in order', () => {
    const code = generateCode('python', multipart)
    expect(code.indexOf('("note", (None, "hello"))')).toBeLessThan(
      code.indexOf('("file", open("/tmp/a b.png", "rb"))'),
    )
  })
  it('writes Go with sorted imports and no-redirect policy', () => {
    const code = generateCode('go', post)
    expect(code).toContain('\t"strings"')
    expect(code).toContain('return http.ErrUseLastResponse')
    expect(code).toContain('req.Header.Add("Authorization", "Bearer it\'s")')
    const imports = code.slice(code.indexOf('(') + 1, code.indexOf(')')).trim()
    const names = imports.split('\n').map((line) => line.trim())
    expect(names).toEqual([...names].sort())
  })
  it('writes Go multipart with the form content type', () => {
    const code = generateCode('go', multipart)
    expect(code).toContain('form.CreateFormFile("file", "a b.png")')
    expect(code).toContain('form.FormDataContentType()')
  })
  it('writes HTTPie with raw body and shell quoting', () => {
    const code = generateCode('httpie', post)
    expect(code).toContain('--raw \'{"name":"line\nbreak"}\'')
    expect(code).toContain(`'Authorization:Bearer it'"'"'s'`)
  })
  it('writes HTTPie file body from stdin', () => {
    const code = generateCode('httpie', {
      ...post,
      body: null,
      bodyFile: '/tmp/body.bin',
    })
    expect(code).not.toContain('--ignore-stdin')
    expect(code).toContain("< '/tmp/body.bin'")
  })
  it('writes Rust with Rust string escapes', () => {
    const code = generateCode('rust', {
      ...post,
      body: 'a"\\\b',
      method: 'PURGE',
    })
    expect(code).toContain('.body("a\\"\\\\\\u{8}")')
    expect(code).toContain('reqwest::Method::from_bytes(b"PURGE")?')
    expect(code).toContain('reqwest::redirect::Policy::none()')
  })
})
