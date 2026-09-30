import { toCurl, type RequestInput } from './request'
import { defaultTransportOptions, type TransportOptions } from './transport-options'

/** Languages the request can be copied as. */
export const codeTargets = [
  { id: 'curl', label: 'cURL' },
  { id: 'fetch', label: 'JavaScript fetch' },
  { id: 'python', label: 'Python requests' },
  { id: 'go', label: 'Go net/http' },
  { id: 'httpie', label: 'HTTPie' },
  { id: 'rust', label: 'Rust reqwest' },
] as const
export type CodeTarget = (typeof codeTargets)[number]['id']
export const isCodeTarget = (value: unknown): value is CodeTarget =>
  codeTargets.some((target) => target.id === value)
export const codeTargetLabel = (id: CodeTarget) =>
  codeTargets.find((target) => target.id === id)?.label ?? id

// JSON string literals are valid string literals in JavaScript, Python and Go.
const str = (value: string) => JSON.stringify(value)
const shell = (value: string) => "'" + value.replace(/'/g, "'\"'\"'") + "'"

/** A Rust string literal. Rust has no \b, \f or \uXXXX escapes. */
function rustString(value: string) {
  let out = '"'
  for (const char of value) {
    const code = char.codePointAt(0)!
    if (char === '"') out += '\\"'
    else if (char === '\\') out += '\\\\'
    else if (char === '\n') out += '\\n'
    else if (char === '\r') out += '\\r'
    else if (char === '\t') out += '\\t'
    else if (code < 0x20 || code === 0x7f) out += `\\u{${code.toString(16)}}`
    else out += char
  }
  return out + '"'
}

function fetchCode(request: RequestInput, options: TransportOptions) {
  const lines: string[] = []
  const files = Boolean(request.bodyFile) || Boolean(request.multipart?.some((p) => p.file))
  if (files) lines.push('import { openAsBlob } from "node:fs";', '')
  let body = ''
  if (request.multipart) {
    lines.push('const form = new FormData();')
    for (const part of request.multipart)
      lines.push(
        part.file
          ? `form.append(${str(part.key)}, await openAsBlob(${str(part.value)}), ${str(part.value.split(/[\\/]/).pop() ?? part.value)});`
          : `form.append(${str(part.key)}, ${str(part.value)});`,
      )
    lines.push('')
    body = 'form'
  } else if (request.bodyFile) body = `await openAsBlob(${str(request.bodyFile)})`
  else if (request.body !== null) body = str(request.body)
  lines.push(`const response = await fetch(${str(request.url)}, {`)
  lines.push(`  method: ${str(request.method)},`)
  if (request.headers.length) {
    lines.push('  headers: [')
    for (const { key, value } of request.headers) lines.push(`    [${str(key)}, ${str(value)}],`)
    lines.push('  ],')
  }
  if (body) lines.push(`  body: ${body},`)
  if (!options.followRedirects) lines.push('  redirect: "manual",')
  lines.push(
    `  signal: AbortSignal.timeout(${options.timeoutSeconds * 1000}),`,
    '});',
    'console.log(response.status, response.statusText);',
    'console.log(await response.text());',
  )
  return lines.join('\n')
}

function pythonCode(request: RequestInput, options: TransportOptions) {
  const args = [`    ${str(request.method)},`, `    ${str(request.url)},`]
  if (request.headers.length) {
    args.push('    headers={')
    for (const { key, value } of request.headers) args.push(`        ${str(key)}: ${str(value)},`)
    args.push('    },')
  }
  if (request.multipart) {
    // A list keeps repeated keys and the part order.
    args.push('    files=[')
    for (const part of request.multipart)
      args.push(
        part.file
          ? `        (${str(part.key)}, open(${str(part.value)}, "rb")),`
          : `        (${str(part.key)}, (None, ${str(part.value)})),`,
      )
    args.push('    ],')
  } else if (request.bodyFile) args.push(`    data=open(${str(request.bodyFile)}, "rb"),`)
  else if (request.body !== null) args.push(`    data=${str(request.body)}.encode(),`)
  args.push(
    `    timeout=(${options.connectTimeoutSeconds}, ${options.timeoutSeconds}),`,
    `    allow_redirects=${options.followRedirects ? 'True' : 'False'},`,
  )
  if (!options.verifyTls) args.push('    verify=False,')
  if (options.proxyUrl)
    args.push(`    proxies={"http": ${str(options.proxyUrl)}, "https": ${str(options.proxyUrl)}},`)
  return [
    'import requests',
    '',
    ...(options.followRedirects
      ? [
          'session = requests.Session()',
          `session.max_redirects = ${options.maxRedirects}`,
          'response = session.request(',
        ]
      : ['response = requests.request(']),
    ...args,
    ')',
    'print(response.status_code, response.reason)',
    'print(response.text)',
  ].join('\n')
}

function goCode(request: RequestInput, options: TransportOptions) {
  const imports = new Set(['fmt', 'io', 'net/http', 'time'])
  const setup: string[] = []
  let body = 'nil'
  if (request.multipart) {
    imports.add('bytes')
    imports.add('mime/multipart')
    setup.push('\tvar body bytes.Buffer', '\tform := multipart.NewWriter(&body)')
    for (const part of request.multipart) {
      if (part.file) {
        imports.add('os')
        const name = part.value.split(/[\\/]/).pop() ?? part.value
        setup.push(
          '\t{',
          `\t\tfile, err := os.Open(${str(part.value)})`,
          '\t\tif err != nil {',
          '\t\t\tpanic(err)',
          '\t\t}',
          `\t\tpart, err := form.CreateFormFile(${str(part.key)}, ${str(name)})`,
          '\t\tif err != nil {',
          '\t\t\tpanic(err)',
          '\t\t}',
          '\t\tif _, err := io.Copy(part, file); err != nil {',
          '\t\t\tpanic(err)',
          '\t\t}',
          '\t\tfile.Close()',
          '\t}',
        )
      } else setup.push(`\tform.WriteField(${str(part.key)}, ${str(part.value)})`)
    }
    setup.push('\tform.Close()', '')
    body = '&body'
  } else if (request.bodyFile) {
    imports.add('os')
    setup.push(
      `\tbody, err := os.Open(${str(request.bodyFile)})`,
      '\tif err != nil {',
      '\t\tpanic(err)',
      '\t}',
      '\tdefer body.Close()',
      '',
    )
    body = 'body'
  } else if (request.body !== null) {
    imports.add('strings')
    body = `strings.NewReader(${str(request.body)})`
  }
  const headers = request.headers.map(
    ({ key, value }) => `\treq.Header.Add(${str(key)}, ${str(value)})`,
  )
  if (request.multipart)
    headers.push('\treq.Header.Set("Content-Type", form.FormDataContentType())')
  const transport: string[] = []
  if (!options.verifyTls) {
    imports.add('crypto/tls')
    transport.push('\t\t\tTLSClientConfig: &tls.Config{InsecureSkipVerify: true},')
  }
  if (options.proxyUrl) {
    imports.add('net/url')
    transport.push('\t\t\tProxy: http.ProxyURL(proxy),')
    setup.push(
      `\tproxy, err := url.Parse(${str(options.proxyUrl)})`,
      '\tif err != nil {',
      '\t\tpanic(err)',
      '\t}',
      '',
    )
  }
  const client = [
    '\tclient := &http.Client{',
    `\t\tTimeout: ${options.timeoutSeconds} * time.Second,`,
    ...(transport.length ? ['\t\tTransport: &http.Transport{', ...transport, '\t\t},'] : []),
    '\t\tCheckRedirect: func(req *http.Request, via []*http.Request) error {',
    ...(options.followRedirects
      ? [
          `\t\t\tif len(via) >= ${options.maxRedirects} {`,
          `\t\t\t\treturn fmt.Errorf("stopped after ${options.maxRedirects} redirects")`,
          '\t\t\t}',
          '\t\t\treturn nil',
        ]
      : ['\t\t\treturn http.ErrUseLastResponse']),
    '\t\t},',
    '\t}',
  ]
  const importLines = [...imports].sort().map((name) => `\t${str(name)}`)
  return [
    'package main',
    '',
    'import (',
    ...importLines,
    ')',
    '',
    'func main() {',
    ...setup,
    `\treq, err := http.NewRequest(${str(request.method)}, ${str(request.url)}, ${body})`,
    '\tif err != nil {',
    '\t\tpanic(err)',
    '\t}',
    ...headers,
    '',
    ...client,
    '\tresp, err := client.Do(req)',
    '\tif err != nil {',
    '\t\tpanic(err)',
    '\t}',
    '\tdefer resp.Body.Close()',
    '\tdata, err := io.ReadAll(resp.Body)',
    '\tif err != nil {',
    '\t\tpanic(err)',
    '\t}',
    '\tfmt.Println(resp.Status)',
    '\tfmt.Println(string(data))',
    '}',
  ].join('\n')
}

function httpieCode(request: RequestInput, options: TransportOptions) {
  const parts = [`http --ignore-stdin --timeout=${options.timeoutSeconds}`]
  if (options.followRedirects) parts.push(`--follow --max-redirects=${options.maxRedirects}`)
  if (!options.verifyTls) parts.push('--verify=no')
  if (options.proxyUrl)
    parts.push(
      `--proxy=${shell(`http:${options.proxyUrl}`)} --proxy=${shell(`https:${options.proxyUrl}`)}`,
    )
  if (request.multipart) parts.push('--multipart')
  if (request.bodyFile) parts[0] = parts[0].replace(' --ignore-stdin', '')
  else if (request.body !== null) parts.push(`--raw ${shell(request.body)}`)
  parts.push(request.method, shell(request.url))
  for (const { key, value } of request.headers) parts.push(shell(`${key}:${value}`))
  for (const part of request.multipart ?? [])
    parts.push(shell(`${part.key}${part.file ? '@' : '='}${part.value}`))
  let command = parts.join(' \\\n  ')
  if (request.bodyFile) command += ` \\\n  < ${shell(request.bodyFile)}`
  return command
}

const standardMethods = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'])

function rustCode(request: RequestInput, options: TransportOptions) {
  const s = rustString
  const builder = [
    '    let client = reqwest::blocking::Client::builder()',
    `        .timeout(Duration::from_secs(${options.timeoutSeconds}))`,
    `        .connect_timeout(Duration::from_secs(${options.connectTimeoutSeconds}))`,
    options.followRedirects
      ? `        .redirect(reqwest::redirect::Policy::limited(${options.maxRedirects}))`
      : '        .redirect(reqwest::redirect::Policy::none())',
    ...(options.verifyTls ? [] : ['        .danger_accept_invalid_certs(true)']),
    ...(options.proxyUrl ? [`        .proxy(reqwest::Proxy::all(${s(options.proxyUrl)})?)`] : []),
    '        .build()?;',
  ]
  const setup: string[] = []
  if (request.multipart) {
    setup.push('    let form = reqwest::blocking::multipart::Form::new()')
    for (const part of request.multipart)
      setup.push(
        part.file
          ? `        .file(${s(part.key)}, ${s(part.value)})?`
          : `        .text(${s(part.key)}, ${s(part.value)})`,
      )
    setup[setup.length - 1] += ';'
  }
  const method = standardMethods.has(request.method)
    ? `reqwest::Method::${request.method}`
    : `reqwest::Method::from_bytes(b${s(request.method)})?`
  const call = [
    '    let response = client',
    `        .request(${method}, ${s(request.url)})`,
    ...request.headers.map(({ key, value }) => `        .header(${s(key)}, ${s(value)})`),
  ]
  if (request.multipart) call.push('        .multipart(form)')
  else if (request.bodyFile) call.push(`        .body(std::fs::read(${s(request.bodyFile)})?)`)
  else if (request.body !== null) call.push(`        .body(${s(request.body)})`)
  call.push('        .send()?;')
  return [
    'use std::time::Duration;',
    '',
    '// Cargo.toml: reqwest = { version = "0.12", features = ["blocking", "multipart"] }',
    'fn main() -> Result<(), Box<dyn std::error::Error>> {',
    ...builder,
    ...setup,
    ...call,
    '    println!("{}", response.status());',
    '    println!("{}", response.text()?);',
    '    Ok(())',
    '}',
  ].join('\n')
}

/** Source code that sends what Blink sends with these transport options. */
export function generateCode(
  target: CodeTarget,
  request: RequestInput,
  options: TransportOptions = defaultTransportOptions(),
) {
  if (target === 'fetch') return fetchCode(request, options)
  if (target === 'python') return pythonCode(request, options)
  if (target === 'go') return goCode(request, options)
  if (target === 'httpie') return httpieCode(request, options)
  if (target === 'rust') return rustCode(request, options)
  return toCurl(request, options)
}

/** The highlight.js language for a target's snippet. */
export const codeLanguage = (target: CodeTarget) =>
  ({
    curl: 'bash',
    fetch: 'javascript',
    python: 'python',
    go: 'go',
    httpie: 'bash',
    rust: 'rust',
  })[target]
