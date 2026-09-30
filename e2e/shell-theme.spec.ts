import { expect, test } from '@playwright/test'

test('a pasted Ghostty palette applies and persists', async ({ page }) => {
  await page.goto('/')
  await page.evaluate(() => localStorage.removeItem('blink.theme'))
  await page.reload()

  await page.getByRole('button', { name: 'Application settings' }).click()
  const dialog = page.getByRole('dialog', { name: 'Application Settings' })
  await dialog
    .getByLabel('Ghostty colors')
    .fill('background = #ffffff\nforeground = #111111\npalette = 3=#0055ff')
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'ghostty')
  await dialog.getByRole('button', { name: 'Save' }).click()
  await expect(dialog).toBeHidden()

  await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(255, 255, 255)')
  await expect(page.locator('[data-theme-name]')).toHaveText('Custom')

  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'ghostty')
  await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(255, 255, 255)')
})

test('cancel restores the previous theme', async ({ page }) => {
  await page.goto('/')
  await page.evaluate(() => localStorage.removeItem('blink.theme'))
  await page.reload()

  await page.getByRole('button', { name: 'Application settings' }).click()
  const dialog = page.getByRole('dialog', { name: 'Application Settings' })
  await dialog.getByLabel('Ghostty colors').fill('background = #ffffff')
  await dialog.getByRole('button', { name: 'Cancel' }).click()

  await expect(page.locator('html')).not.toHaveAttribute('data-theme', /.+/)
  await expect(page.locator('[data-theme-name]')).toHaveText('Blink')
})

test('the command center jumps to a request', async ({ page }) => {
  await page.goto('/')
  await page.getByLabel('Request URL', { exact: true }).fill('https://example.test/v1/users')
  await page.getByRole('button', { name: 'New request', exact: true }).click()

  await page.keyboard.press('ControlOrMeta+p')
  const search = page.getByRole('combobox', { name: 'Search requests' })
  await expect(search).toBeFocused()
  await search.fill('users')
  await page.keyboard.press('Enter')

  await expect(
    page
      .getByRole('tablist', { name: 'Requests', exact: true })
      .getByRole('tab', { selected: true }),
  ).toContainText('users')
})
