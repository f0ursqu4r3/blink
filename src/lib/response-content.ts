import hljs from 'highlight.js/lib/common'

export type ResponseLanguage =
  | 'bash'
  | 'css'
  | 'graphql'
  | 'ini'
  | 'javascript'
  | 'json'
  | 'markdown'
  | 'plaintext'
  | 'sql'
  | 'xml'
  | 'yaml'

function normalizedContentType(contentType: string) {
  return contentType.split(';', 1)[0].trim().toLowerCase()
}

export function responseLanguage(contentType: string): ResponseLanguage {
  const type = normalizedContentType(contentType)
  if (type === 'application/json' || type.endsWith('+json')) return 'json'
  if (
    type === 'application/xml' ||
    type === 'text/xml' ||
    type === 'text/html' ||
    type.endsWith('+xml')
  )
    return 'xml'
  if (type === 'text/css') return 'css'
  if (
    type === 'application/javascript' ||
    type === 'application/ecmascript' ||
    type === 'text/javascript'
  )
    return 'javascript'
  if (type === 'application/yaml' || type === 'application/x-yaml' || type === 'text/yaml')
    return 'yaml'
  if (type === 'application/graphql' || type === 'application/x-graphql') return 'graphql'
  if (type === 'application/sql' || type === 'text/x-sql') return 'sql'
  if (type === 'application/x-sh' || type === 'text/x-shellscript') return 'bash'
  if (type === 'text/markdown') return 'markdown'
  if (
    type === 'application/x-www-form-urlencoded' ||
    type === 'text/ini' ||
    type === 'application/toml'
  )
    return 'ini'
  return 'plaintext'
}

export function highlightResponseLine(text: string, language: ResponseLanguage) {
  if (!text || language === 'plaintext') return escapeHtml(text)
  try {
    return hljs.highlight(text, { language, ignoreIllegals: true }).value
  } catch {
    return escapeHtml(text)
  }
}

/**
 * Highlighted HTML for a whole source text, such as generated code. Any
 * highlight.js language name; unknown names return escaped text.
 */
export function highlightSource(text: string, language: string) {
  if (!text || !hljs.getLanguage(language)) return escapeHtml(text)
  return hljs.highlight(text, { language, ignoreIllegals: true }).value
}

function escapeHtml(text: string) {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#x27;')
}
