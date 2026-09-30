import { test, expect } from '@playwright/test'
import { readFile } from 'node:fs/promises'

test('a large response shows a preview and saves the full body', async ({ page }) => {
  const body = 'x'.repeat(5 * 1024 * 1024)
  await page.route('https://example.test/**', (route) =>
    route.fulfill({ status: 200, contentType: 'text/plain', body }),
  )
  await page.goto('/')
  await page.getByLabel('Request URL', { exact: true }).fill('https://example.test/big.txt')
  await page.getByRole('button', { name: /Send/ }).click()
  const bar = page.locator('[data-response-truncated]')
  await expect(bar).toContainText('Preview shows the first 4.00 MiB of 5.00 MiB.')
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.locator('[data-save-response]').click(),
  ])
  expect(download.suggestedFilename()).toBe('big.txt')
  const saved = await readFile((await download.path())!, 'utf8')
  expect(saved.length).toBe(body.length)
})
