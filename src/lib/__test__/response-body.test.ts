import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  canSaveResponse,
  releaseResponse,
  saveResponse,
  storeBlob,
  suggestedFileName,
} from '../response-body'
import type { ApiResponse } from '../request'

const base: ApiResponse = {
  status: 200,
  statusText: 'OK',
  durationMs: 1,
  sizeBytes: 2,
  headers: [],
  body: '{}',
}
const withHeader = (key: string, value: string): ApiResponse => ({
  ...base,
  headers: [{ key, value }],
})
afterEach(() => vi.restoreAllMocks())

describe('suggestedFileName', () => {
  it('prefers Content-Disposition', () => {
    expect(
      suggestedFileName(
        withHeader('Content-Disposition', 'attachment; filename="report.csv"'),
        'https://x.test/a/b',
      ),
    ).toBe('report.csv')
    expect(
      suggestedFileName(
        withHeader('content-disposition', "attachment; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"),
        'https://x.test/',
      ),
    ).toBe('résumé.pdf')
  })
  it('removes path separators and control characters', () => {
    expect(
      suggestedFileName(
        withHeader('Content-Disposition', 'attachment; filename="../../etc/pass\u0007wd"'),
        'https://x.test/',
      ),
    ).toBe('....etcpasswd')
  })
  it('uses the last URL segment, then the content type', () => {
    expect(suggestedFileName(base, 'https://x.test/files/logo.png?x=1')).toBe('logo.png')
    expect(
      suggestedFileName(
        { ...base, finalUrl: 'https://cdn.test/real.zip' },
        'https://x.test/download',
      ),
    ).toBe('real.zip')
    expect(
      suggestedFileName(
        withHeader('Content-Type', 'application/json; charset=utf-8'),
        'https://x.test/',
      ),
    ).toBe('response.json')
    expect(suggestedFileName(base, 'not a url')).toBe('response.bin')
  })
})

describe('canSaveResponse', () => {
  it('needs a stored body unless the preview is complete text', () => {
    expect(canSaveResponse(base)).toBe(true)
    expect(canSaveResponse({ ...base, truncated: true })).toBe(false)
    expect(canSaveResponse({ ...base, binary: true })).toBe(false)
    expect(canSaveResponse({ ...base, binary: true, bodyId: 'b' })).toBe(true)
  })
})

describe('browser saves', () => {
  it('downloads a stored blob and forgets it after release', async () => {
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
    URL.createObjectURL = vi.fn(() => 'blob:test')
    URL.revokeObjectURL = vi.fn()
    const bodyId = storeBlob(new Blob([new Uint8Array([0, 1])]))
    const response = { ...base, bodyId, binary: true }
    expect(await saveResponse(response, 'https://x.test/f.bin')).toBe(true)
    expect(click).toHaveBeenCalledOnce()
    releaseResponse(response)
    await expect(saveResponse(response, 'https://x.test/f.bin')).rejects.toThrow(
      'no longer available',
    )
  })
  it('saves complete preview text without a stored body', async () => {
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
    URL.createObjectURL = vi.fn(() => 'blob:test')
    URL.revokeObjectURL = vi.fn()
    expect(await saveResponse(base, 'https://x.test/')).toBe(true)
    await expect(saveResponse({ ...base, truncated: true }, 'https://x.test/')).rejects.toThrow(
      'no longer available',
    )
  })
})
