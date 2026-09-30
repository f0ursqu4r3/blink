import { test, expect, type Page } from '@playwright/test'

const rows = Array.from({ length: 60 }, (_, i) => ({
  id: i,
  path: `home/${i}`,
  title: `Page ${i}`,
  isFolder: i % 2 === 0,
  parent: null,
}))
const bodies = {
  json: JSON.stringify({ data: rows }),
  text: rows.map((r) => `line ${r.id} ${r.path}`).join('\n'),
}

async function viewport(page: Page) {
  return page.locator('[data-response-body]').evaluate((el) => {
    const indexes = [...el.querySelectorAll('[data-index]')].map((row) =>
      Number(row.getAttribute('data-index')),
    )
    return { scrollTop: el.scrollTop, first: Math.min(...indexes) }
  })
}

for (const [kind, body] of Object.entries(bodies))
  test(`${kind} response opens at the top with rows rendered`, async ({ page }) => {
    await page.route('https://example.test/**', (route) =>
      route.fulfill({
        status: 200,
        contentType: kind === 'json' ? 'application/json' : 'text/plain',
        body,
      }),
    )
    await page.goto('/')
    await page.getByLabel('Request URL', { exact: true }).fill('https://example.test/pages')
    const send = page.getByRole('button', { name: /Send/ })
    for (let attempt = 0; attempt < 2; attempt++) {
      await send.click()
      await expect(page.locator('[data-response-body]')).toBeVisible()
      await expect.poll(() => viewport(page)).toEqual({ scrollTop: 0, first: 0 })
    }
  })
