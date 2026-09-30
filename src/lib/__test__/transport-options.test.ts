import { describe, expect, it } from 'vitest'
import { defaultTransportOptions, proxyUrlError, transportFieldErrors } from '../transport-options'

describe('transport options', () => {
  it('accepts the defaults', () => {
    expect(defaultTransportOptions()).toEqual({
      timeoutSeconds: 30,
      connectTimeoutSeconds: 10,
      followRedirects: false,
      maxRedirects: 10,
      inspectionLimitMiB: 4,
      verifyTls: true,
      proxyUrl: '',
      storeCookies: true,
    })
    expect(transportFieldErrors(defaultTransportOptions())).toEqual({})
  })
  it('rejects values out of range, decimals and NaN', () => {
    const errors = transportFieldErrors({
      ...defaultTransportOptions(),
      timeoutSeconds: 601,
      connectTimeoutSeconds: 1.5,
      followRedirects: true,
      maxRedirects: 0,
      inspectionLimitMiB: Number.NaN,
    })
    expect(errors).toEqual({
      timeoutSeconds: 'Enter a whole number from 1 to 600.',
      connectTimeoutSeconds: 'Enter a whole number from 1 to 600.',
      maxRedirects: 'Enter a whole number from 1 to 20.',
      inspectionLimitMiB: 'Enter a whole number from 1 to 16.',
    })
  })
  it('limits the connect timeout to the total timeout', () => {
    expect(
      transportFieldErrors({
        ...defaultTransportOptions(),
        timeoutSeconds: 5,
        connectTimeoutSeconds: 6,
      }),
    ).toEqual({ connectTimeoutSeconds: 'Enter a whole number from 1 to 5.' })
  })
  it('accepts an empty, http, https, or socks5 proxy URL', () => {
    for (const url of ['', ' ', 'http://127.0.0.1:8080', 'socks5h://proxy:1080'])
      expect(proxyUrlError(url)).toBe('')
    for (const url of ['proxy:8080', 'ftp://proxy', 'http://'])
      expect(proxyUrlError(url)).toContain('proxy URL')
  })
})
