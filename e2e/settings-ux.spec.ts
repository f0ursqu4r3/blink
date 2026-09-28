import { expect, test, type Page } from "@playwright/test";
import { createSession } from "../src/lib/session";
import { encodeWorkspace, WORKSPACE_KEY } from "../src/lib/workspace";
import { defaultPreferences } from "../src/lib/preferences";

async function seedGroups(page: Page) {
  const source = createSession();
  source.groupId = 502;
  source.draft.url = "https://example.test/original";
  source.view = {
    ...source.view,
    requestTab: "headers",
    pretty: false,
    wrap: true,
  };
  const snapshot = encodeWorkspace(
    [source],
    source.id,
    [
      {
        id: 501,
        name: "Parent API",
        parentId: null,
        collapsed: false,
        defaultMethod: "POST",
        defaultUrl: "{{host}}/v1",
        localDefinitions: { host: "https://example.test" },
      },
      {
        id: 502,
        name: "Child API",
        parentId: 501,
        collapsed: true,
        defaultMethod: "PATCH",
      },
    ],
    {},
    { ...defaultPreferences(), defaultMethod: "DELETE" },
  );
  await page.addInitScript(
    ({ key, snapshot }) => {
      if (!localStorage.getItem(key)) localStorage.setItem(key, snapshot);
    },
    { key: WORKSPACE_KEY, snapshot },
  );
  await page.goto("/");
}

test("application settings save, tooltip, and modal keyboard lifecycle", async ({
  page,
}) => {
  await page.goto("/");
  const opener = page.getByRole("button", { name: "Application settings" });
  await opener.click();
  const dialog = page.getByRole("dialog", { name: "Application Settings" });
  await expect(dialog).toBeVisible();

  const help = dialog.getByRole("button", { name: "Token syntax help" });
  await help.click();
  await expect(page.getByText(/Workspace-global tokens use/)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeVisible();
  await page.mouse.move(0, 0);
  await help.hover();
  await expect(page.getByText(/Workspace-global tokens use/)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeVisible();

  await expect(dialog.getByText("New request defaults")).toBeVisible();
  await dialog.locator("select").first().selectOption("POST");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();
  await expect(opener).toBeFocused();

  await page.reload();
  await page.getByRole("button", { name: "New request" }).click();
  await expect(
    page
      .locator('[data-request-pane][data-active="true"]')
      .getByLabel("HTTP method"),
  ).toHaveValue("POST");

  await opener.click();
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(opener).toBeFocused();
});

test("tooltip opens from a touch tap", async ({ browser }) => {
  const context = await browser.newContext({
    hasTouch: true,
    isMobile: true,
    viewport: { width: 390, height: 844 },
  });
  const page = await context.newPage();
  await page.goto("/");
  await page.getByRole("button", { name: "Application settings" }).click();
  const help = page
    .getByRole("dialog", { name: "Application Settings" })
    .getByRole("button", { name: "Token syntax help" });
  const box = await help.boundingBox();
  if (!box) throw new Error("Tooltip trigger has no touch target.");
  await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.getByText(/Workspace-global tokens use/)).toBeVisible();
  await context.close();
});

test("group defaults save and inherit without changing existing requests or duplicates", async ({
  page,
}) => {
  await seedGroups(page);
  const pane = page.locator('[data-request-pane][data-active="true"]');
  await page
    .getByRole("button", { name: "New request in Child API", exact: true })
    .click();
  await expect(pane.getByLabel("HTTP method")).toHaveValue("PATCH");
  await expect(pane.getByLabel("Request URL", { exact: true })).toHaveValue(
    "{{host}}/v1",
  );
  await expect(pane.getByLabel("Request URL", { exact: true })).toBeFocused();
  await expect(
    page.locator("[data-request-browser] [data-request-id]"),
  ).toHaveCount(2);

  const opener = page.getByRole("button", {
    name: "Group settings for Child API",
  });
  await opener.click();
  const dialog = page.getByRole("dialog", { name: "Group Settings" });
  await expect(dialog.getByLabel("Name", { exact: true })).toBeFocused();
  await expect(dialog.getByLabel("Initial URL")).toHaveAttribute(
    "placeholder",
    "{{host}}/v1",
  );
  await expect(dialog.getByLabel("Method", { exact: true })).toContainText(
    "Inherit (POST)",
  );
  await expect(dialog.getByLabel("Parent").locator("option")).toHaveCount(2);
  await dialog.getByLabel("Method", { exact: true }).selectOption("PUT");
  await dialog.getByLabel("Initial URL").fill("https://example.test/new");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(opener).toBeFocused();
  await expect(pane.getByLabel("HTTP method")).toHaveValue("PATCH");
  await page.reload();
  await page
    .getByRole("button", { name: "New request in Child API", exact: true })
    .click();
  await expect(pane.getByLabel("HTTP method")).toHaveValue("PUT");
  await expect(pane.getByLabel("Request URL", { exact: true })).toHaveValue(
    "https://example.test/new",
  );

  await page
    .getByRole("tablist", { name: "Requests", exact: true })
    .getByRole("tab")
    .first()
    .click();
  await page
    .getByRole("button", { name: "Duplicate request", exact: true })
    .click();
  await expect(pane.getByLabel("HTTP method")).toHaveValue("GET");
  await expect(pane.getByLabel("Request URL", { exact: true })).toHaveValue(
    "https://example.test/original",
  );
  await expect(
    pane.getByRole("tab", { name: /Headers/ }).first(),
  ).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText("SAVED LOCALLY", { exact: true })).toBeVisible();
  const snapshot = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key)!),
    WORKSPACE_KEY,
  );
  const duplicate = snapshot.tabs.find(
    (session: { id: number }) => session.id === snapshot.activeId,
  );
  expect(duplicate.view).toMatchObject({
    pretty: false,
    wrap: true,
    responseScroll: 0,
  });
  expect(duplicate.response).toBeNull();
});

test("settings isolate keyboard shortcuts, trap focus and discard canceled edits", async ({
  page,
}) => {
  await page.goto("/");
  await page.keyboard.press("Control+,");
  const dialog = page.getByRole("dialog", { name: "Application Settings" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel("Method", { exact: true })).toBeFocused();
  for (const shortcut of [
    "Control+t",
    "Meta+t",
    "Control+Shift+d",
    "Meta+Shift+d",
    "Control+w",
    "Meta+w",
    "Control+l",
    "Control+f",
  ])
    await page.keyboard.press(shortcut);
  await expect(page.locator("[data-request-pane]")).toHaveCount(1);
  for (let i = 0; i < 14; i++) {
    await page.keyboard.press("Tab");
    expect(
      await page.evaluate(() =>
        Boolean(document.activeElement?.closest('[role="dialog"]')),
      ),
    ).toBe(true);
  }
  await dialog.getByLabel("Method", { exact: true }).selectOption("POST");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("button", { name: "Application settings" }).click();
  await expect(dialog.getByLabel("Method", { exact: true })).toHaveValue("GET");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("group overflow actions keep names readable and confirm deletion", async ({
  page,
}) => {
  await seedGroups(page);
  const row = page.locator('[data-group-id="501"]');
  const name = row.locator('span[title="Parent API"]');
  expect(await name.evaluate((el) => el.clientWidth >= el.scrollWidth)).toBe(
    true,
  );
  await page
    .getByRole("button", { name: "More actions for Parent API" })
    .click();
  const menu = page.getByRole("menu");
  await expect(menu.getByRole("menuitem")).toHaveCount(4);
  await menu
    .getByRole("menuitem", { name: "Add group inside Parent API" })
    .click();
  await page.getByLabel("Group name in Parent API").fill("Sibling API");
  await page.getByLabel("Group name in Parent API").press("Enter");
  await expect(
    page.getByRole("button", { name: "Group settings for Sibling API" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "More actions for Sibling API" })
    .click();
  await page.getByRole("menuitem", { name: "Delete group" }).click();
  await expect(page.getByText(/Delete Sibling API\?/)).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Group settings for Sibling API" }),
  ).toBeVisible();
});

test("both settings dialogs fit narrow windows with reachable actions", async ({
  page,
}) => {
  await seedGroups(page);
  await page.setViewportSize({ width: 390, height: 640 });
  await page.getByRole("button", { name: "Show request browser" }).click();
  for (const name of ["Group settings for Child API", "Application settings"]) {
    await page.getByRole("button", { name, exact: true }).click();
    const dialog = page.getByRole("dialog");
    const box = (await dialog.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(390);
    expect(box.y + box.height).toBeLessThanOrEqual(640);
    expect(
      await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth),
    ).toBe(true);
    const save = dialog.getByRole("button", { name: "Save", exact: true });
    const saveBox = (await save.boundingBox())!;
    expect(saveBox.y + saveBox.height).toBeLessThanOrEqual(640);
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    390,
  );
});
