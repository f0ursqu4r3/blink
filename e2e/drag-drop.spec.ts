import { test, expect, type Locator, type Page } from "@playwright/test";

type Spot = "top" | "middle" | "bottom" | "left" | "right";

async function drag(
  page: Page,
  from: Locator,
  to: Locator,
  spot: Spot = "middle",
) {
  const a = await from.boundingBox();
  const b = await to.boundingBox();
  if (!a || !b) throw new Error("drag target not visible");
  const x =
    spot === "left"
      ? b.x + 6
      : spot === "right"
        ? b.x + b.width - 6
        : b.x + b.width / 2;
  const y =
    spot === "top"
      ? b.y + 3
      : spot === "bottom"
        ? b.y + b.height - 3
        : b.y + b.height / 2;
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2 + 8, {
    steps: 2,
  });
  await page.mouse.move(x, y, { steps: 10 });
  await page.mouse.up();
}

const list = (page: Page) => page.locator("[data-browser-list]");
const row = (page: Page, text: string) =>
  list(page).locator("[data-drop-key^='request-']", { hasText: text });
const folder = (page: Page, name: string) =>
  list(page).locator("[data-drop-key^='group-']", { hasText: name });
const tabs = (page: Page) =>
  page.getByRole("tablist", { name: "Requests", exact: true }).getByRole("tab");
async function rowOrder(page: Page) {
  return list(page)
    .locator("[data-drop-key]")
    .evaluateAll((els) =>
      els.map((el) => el.textContent?.trim().replace(/\s+/g, " ") ?? ""),
    );
}

async function addRequest(page: Page, path: string, first = false) {
  if (!first)
    await page
      .getByRole("button", { name: "New request", exact: true })
      .click();
  await page
    .locator('[data-request-pane][data-active="true"]')
    .getByLabel("Request URL", { exact: true })
    .fill(`https://example.test/${path}`);
}

async function addGroup(page: Page, name: string) {
  await page.getByRole("button", { name: "Add top-level group" }).click();
  await page.getByLabel("Top-level group name").fill(name);
  await page.getByRole("button", { name: "Create top-level group" }).click();
}

test("drag and drop in the browser and the tab bar", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await addRequest(page, "alpha", true);
  await addRequest(page, "beta");
  await addRequest(page, "gamma");

  // Reorder: gamma before alpha.
  await drag(page, row(page, "/gamma"), row(page, "/alpha"), "top");
  const order = await rowOrder(page);
  expect(order.findIndex((text) => text.includes("/gamma"))).toBeLessThan(
    order.findIndex((text) => text.includes("/alpha")),
  );

  // Into a folder.
  await addGroup(page, "Platform");
  await drag(page, row(page, "/alpha"), folder(page, "Platform"));
  await page.getByRole("button", { name: "Collapse Platform" }).click();
  await expect(row(page, "/alpha")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand Platform" }).click();

  // Nest a folder.
  await addGroup(page, "Nested");
  await drag(page, folder(page, "Nested"), folder(page, "Platform"));
  await page.getByRole("button", { name: "Collapse Platform" }).click();
  await expect(folder(page, "Nested")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand Platform" }).click();

  // Reorder tabs: alpha to the end.
  await drag(
    page,
    tabs(page).filter({ hasText: "/alpha" }),
    tabs(page).last(),
    "right",
  );
  await expect(tabs(page).last()).toContainText("/alpha");

  // Sidebar to tab bar: close beta, then drag its row before the first tab.
  await page.getByRole("button", { name: "Close /beta", exact: true }).click();
  await expect(tabs(page).filter({ hasText: "/beta" })).toHaveCount(0);
  await drag(page, row(page, "/beta"), tabs(page).first(), "left");
  await expect(tabs(page).first()).toContainText("/beta");

  // Tab to folder: gamma's tab into Platform.
  await drag(
    page,
    tabs(page).filter({ hasText: "/gamma" }),
    folder(page, "Platform"),
  );
  await page.getByRole("button", { name: "Collapse Platform" }).click();
  await expect(row(page, "/gamma")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand Platform" }).click();

  // Persistence.
  const before = await rowOrder(page);
  const tabOrder = await tabs(page).allTextContents();
  await expect(page.getByText("SAVED LOCALLY", { exact: true })).toBeVisible();
  await page.reload();
  await expect(
    list(page).locator("[data-drop-key^='request-']").first(),
  ).toBeVisible();
  expect(await rowOrder(page)).toEqual(before);
  expect(await tabs(page).allTextContents()).toEqual(tabOrder);
  expect(errors).toEqual([]);
});

test("Escape cancels a drag", async ({ page }) => {
  await page.goto("/");
  await addRequest(page, "alpha", true);
  await addRequest(page, "beta");
  const before = await rowOrder(page);
  const from = await row(page, "/alpha").boundingBox();
  const to = await row(page, "/beta").boundingBox();
  if (!from || !to) throw new Error("rows not visible");
  await page.mouse.move(from.x + 20, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + 20, to.y + to.height - 3, { steps: 10 });
  await expect(page.locator("[data-drag-preview]")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(page.locator("[data-drag-preview]")).toHaveCount(0);
  expect(await rowOrder(page)).toEqual(before);
});
