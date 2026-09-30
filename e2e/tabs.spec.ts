import { test, expect } from "@playwright/test";

test("parallel sends stay in their own tabs and duplicates are independent", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  let release!: () => void;
  let calls = 0;
  await page.route("https://example.test/**", async (route) => {
    calls++;
    const slow = route.request().url().endsWith("/systems");
    if (slow)
      await new Promise<void>((done) => {
        release = done;
      });
    await route.fulfill({
      status: slow ? 200 : 201,
      contentType: "application/json",
      body: JSON.stringify({
        resource: slow ? "systems" : "diagnostics",
        active: true,
        id: "B-014",
        checks: ["transport", "authentication", "payload"],
      }),
    });
  });
  await page.goto("/");
  const pane = page.locator('[data-request-pane][data-active="true"]');
  const tabs = page
    .getByRole("tablist", { name: "Requests", exact: true })
    .getByRole("tab");
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/v1/systems");
  await pane.getByRole("button", { name: /Send/ }).click();
  await expect(page.locator("[data-close-request]").first()).toBeEnabled();
  await page.getByRole("button", { name: "New request", exact: true }).click();
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/v1/diagnostics");
  await pane.getByLabel("HTTP method").fill("POST");
  await pane
    .getByRole("tablist", { name: "Request options" })
    .getByRole("tab", { name: "Body" })
    .click();
  await pane
    .getByRole("combobox", { name: "Body", exact: true })
    .selectOption("json");
  await pane
    .getByLabel("Request body", { exact: true })
    .fill('{\n  "target": "local",\n  "depth": 2,\n  "verbose": true\n}');
  await pane.getByRole("button", { name: /Send/ }).click();
  await expect(pane.locator("[data-response-status]")).toContainText("201");
  await expect(pane.locator("[data-response-search]")).toHaveCount(0);
  await page.keyboard.press("Control+f");
  await expect(pane.locator("[data-response-search]")).toBeVisible();
  await pane.getByRole("button", { name: "Wrap lines" }).click();
  await expect(pane.locator("[data-json-tree]")).toHaveClass(/wrapped/);
  await page.getByRole("button", { name: "Duplicate request" }).click();
  await expect(tabs).toHaveCount(3);
  await expect(pane.locator("[data-response-body]")).toHaveCount(0);
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/v1/diagnostics/copy");
  release();
  await expect(tabs.first()).toContainText("200");
  await expect(pane.locator("[data-response-body]")).toHaveCount(0);
  await tabs.first().click();
  await expect(pane.locator("[data-response-body]")).toContainText("systems");
  await tabs.nth(1).click();
  await expect(pane.locator("[data-response-body]")).toContainText(
    "diagnostics",
  );
  await expect(
    pane.getByRole("button", { name: "Wrap lines" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(pane.getByLabel("Request body", { exact: true })).toHaveText(
    /"depth": 2/,
  );
  await expect(pane.getByLabel("Request URL", { exact: true })).toHaveValue(
    "https://example.test/v1/diagnostics",
  );
  await page.screenshot({ path: "artifacts/blink-tabs.png" });
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/v1/changed");
  await expect(
    pane.getByText("Previous response · request edited since send"),
  ).toBeVisible();
  expect(calls).toBe(2);
  const duplicateIds = await page.evaluate(() => {
    const ids = [...document.querySelectorAll("[id]")].map((node) => node.id);
    return ids.filter((id, index) => ids.indexOf(id) !== index);
  });
  expect(duplicateIds).toEqual([]);
  expect(errors).toEqual([]);
});

test("keyboard new, switch, duplicate and close remain scoped to requests", async ({
  page,
}) => {
  await page.goto("/");
  const pane = page.locator('[data-request-pane][data-active="true"]');
  const tabs = page
    .getByRole("tablist", { name: "Requests", exact: true })
    .getByRole("tab");
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/keyboard");
  await page.keyboard.press("Control+t");
  await expect(tabs).toHaveCount(2);
  await expect(pane.getByLabel("Request URL", { exact: true })).toHaveValue("");
  await page.keyboard.press("Control+Shift+Tab");
  await expect(tabs.first()).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Control+Shift+d");
  await expect(tabs).toHaveCount(3);
  await expect(pane.getByLabel("Request URL", { exact: true })).toHaveValue(
    "https://example.test/keyboard",
  );
  await page.keyboard.press("Control+w");
  await expect(tabs).toHaveCount(2);
  await expect(page.locator("[data-request-id]")).toHaveCount(3);
  await expect(tabs.nth(1)).toBeFocused();
  await page.keyboard.press("Home");
  await expect(tabs.first()).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.nth(1)).toBeFocused();
  await page.keyboard.press("Delete");
  await expect(tabs).toHaveCount(1);
  await expect(page.locator("[data-request-id]")).toHaveCount(3);
  expect(
    await page.evaluate(() => ({
      local: localStorage.getItem("blink.workspace.v1") !== null,
      session: sessionStorage.length,
    })),
  ).toEqual({ local: true, session: 0 });
});

test("tab switches preserve response scrolling and send only the active request", async ({
  page,
}) => {
  const sent: string[] = [];
  await page.route("https://example.test/**", (route) => {
    sent.push(route.request().url());
    return route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(
        Array.from({ length: 200 }, (_, id) => ({
          id,
          value: "local fixture",
        })),
      ),
    });
  });
  await page.goto("/");
  const pane = page.locator('[data-request-pane][data-active="true"]');
  const tabs = page
    .getByRole("tablist", { name: "Requests", exact: true })
    .getByRole("tab");
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/first");
  await pane.getByLabel("Request URL", { exact: true }).press("Control+Enter");
  await expect(pane.locator("[data-response-status]")).toBeVisible();
  const responseScroller = pane.locator("[data-json-tree], .code-view");
  await responseScroller.evaluate((element) => {
    element.scrollTop = 320;
  });
  const scroll = await responseScroller.evaluate(
    (element) => element.scrollTop,
  );
  expect(scroll).toBeGreaterThan(0);
  await page.getByRole("button", { name: "New request", exact: true }).click();
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/second");
  await pane.getByLabel("Request URL", { exact: true }).press("Control+Enter");
  await expect(pane.locator("[data-response-status]")).toBeVisible();
  expect(sent).toEqual([
    "https://example.test/first",
    "https://example.test/second",
  ]);
  // The next real change saves the first tab's scroll offset.
  await expect
    .poll(() =>
      page.evaluate(() => {
        const workspace = JSON.parse(
          localStorage.getItem("blink.workspace.v1")!,
        );
        return workspace.tabs.find(
          (session: { draft: { url: string } }) =>
            session.draft.url === "https://example.test/first",
        )?.view.responseScroll;
      }),
    )
    .toBe(scroll);
  await tabs.first().click();
  await expect
    .poll(() =>
      pane
        .locator("[data-json-tree], .code-view")
        .evaluate((element) => element.scrollTop),
    )
    .toBe(scroll);
});

for (const width of [1180, 390]) {
  test(`many tabs scroll without losing the new action at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 780 });
    await page.goto("/");
    const pane = page.locator('[data-request-pane][data-active="true"]');
    for (let index = 0; index < 9; index++) {
      await pane
        .getByLabel("Request URL", { exact: true })
        .fill(`https://example.test/long-endpoint-${index}`);
      await page
        .getByRole("button", { name: "New request", exact: true })
        .click();
    }
    const tabs = page
      .getByRole("tablist", { name: "Requests", exact: true })
      .getByRole("tab");
    await expect(tabs).toHaveCount(10);
    const add = page.getByRole("button", { name: "New request", exact: true });
    const addBox = await add.boundingBox();
    expect(addBox!.x + addBox!.width).toBeLessThanOrEqual(width);
    await expect(tabs.last()).toBeInViewport();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth > innerWidth,
      ),
    ).toBe(false);
    await tabs.last().focus();
    await page.keyboard.press("Home");
    await expect(tabs.first()).toBeInViewport();
    await page.screenshot({
      path: `artifacts/blink-many-tabs-${width}.png`,
      fullPage: true,
    });
  });
}

test("many tabs keep the bar height and show an overflow menu", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await page.goto("/");
  const add = page.locator("[data-new-request]");
  for (let i = 0; i < 11; i++) await add.click();
  const bar = page.locator("[data-tab-bar]");
  const strip = bar.getByRole("tablist");
  await expect(strip.getByRole("tab")).toHaveCount(12);
  expect((await bar.boundingBox())!.height).toBeCloseTo(36, 0);
  // A visible horizontal scrollbar makes offsetHeight larger than clientHeight.
  const gap = await strip.evaluate((el) => el.offsetHeight - el.clientHeight);
  expect(gap).toBe(0);
  await expect(strip).toHaveAttribute("data-overflow-left", "true");
  const overflow = page.locator("[data-tab-overflow]");
  await expect(overflow).toBeVisible();
  await overflow.click();
  await page.locator("[data-tab-overflow-item]").first().click();
  await expect(strip.getByRole("tab").first()).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(strip).toHaveAttribute("data-overflow-left", "false");
});
