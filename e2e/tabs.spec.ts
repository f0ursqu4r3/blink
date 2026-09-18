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
  await expect(page.locator("[data-close-request]").first()).toBeDisabled();
  await page.getByRole("button", { name: "New request", exact: true }).click();
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/v1/diagnostics");
  await pane.getByLabel("HTTP method").selectOption("POST");
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
  await pane.getByRole("button", { name: "Wrap lines" }).click();
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
  await expect(pane.getByLabel("Request body", { exact: true })).toHaveValue(
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
  await expect(
    page.getByRole("group", { name: "Confirm close request" }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("group", { name: "Confirm close request" }),
  ).toHaveCount(0);
  await expect(tabs.nth(2)).toBeFocused();
  await page.keyboard.press("Home");
  await expect(tabs.first()).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.nth(1)).toBeFocused();
  await page.keyboard.press("Delete");
  await expect(tabs).toHaveCount(2);
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
  await pane.locator(".code-view").evaluate((element) => {
    element.scrollTop = 320;
  });
  const scroll = await pane
    .locator(".code-view")
    .evaluate((element) => element.scrollTop);
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
  await tabs.first().click();
  expect(
    await pane.locator(".code-view").evaluate((element) => element.scrollTop),
  ).toBe(scroll);
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
