import { test, expect } from "@playwright/test";
import { buildSchema, introspectionFromSchema } from "graphql";

const introspection = JSON.stringify({
  data: introspectionFromSchema(
    buildSchema("type Query { viewer: User } type User { id: ID! }"),
  ),
});

test("fetch schema enables GraphQL completion; JSON is highlighted", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("https://graph.test/**", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: introspection,
    });
  });
  await page.goto("/");
  await page
    .getByLabel("Request URL", { exact: true })
    .fill("https://graph.test/graphql");
  await page.getByLabel("HTTP method").selectOption("POST");
  const requestTabs = page.getByRole("tablist", { name: "Request options" });
  await requestTabs.getByRole("tab", { name: "Body" }).click();
  const bodyMode = page.getByRole("combobox", { name: "Body", exact: true });

  await bodyMode.selectOption("json");
  await page.getByLabel("Request body", { exact: true }).fill('{"a": "b"}');
  await expect(
    page.locator("[data-testid='body-editor'] span[class]").first(),
  ).toBeVisible();

  await bodyMode.selectOption("graphql");
  await page.getByRole("button", { name: "Fetch schema" }).click();
  await expect(page.getByTestId("schema-status")).toHaveText(
    "Schema loaded · just now",
  );
  const query = page.getByLabel("GraphQL query", { exact: true });
  await query.fill("");
  await query.click();
  await page.keyboard.type("{ vi");
  await expect(page.locator(".cm-tooltip-autocomplete")).toContainText(
    "viewer",
  );
  expect(errors).toEqual([]);
});

test("format error names the line and marks it in the editor", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByLabel("HTTP method").selectOption("POST");
  const requestTabs = page.getByRole("tablist", { name: "Request options" });
  await requestTabs.getByRole("tab", { name: "Body" }).click();
  await page
    .getByRole("combobox", { name: "Body", exact: true })
    .selectOption("json");
  const body = page.getByLabel("Request body", { exact: true });
  await body.click();
  await page.keyboard.insertText('{\n"a": 1,\n"b": }');
  await page.getByRole("button", { name: "Format body", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Invalid JSON at line 3, column 6",
  );
  await expect(page.locator(".cm-errorLine")).toHaveText('"b": }');
  await expect(body).toBeFocused();
  await page.keyboard.type("1");
  await expect(page.locator(".cm-errorLine")).toHaveCount(0);
});

test("variables pane resizes by drag and collapses", async ({ page }) => {
  await page.goto("/");
  await page.getByLabel("HTTP method").selectOption("POST");
  const requestTabs = page.getByRole("tablist", { name: "Request options" });
  await requestTabs.getByRole("tab", { name: "Body" }).click();
  await page
    .getByRole("combobox", { name: "Body", exact: true })
    .selectOption("graphql");
  const pane = page.locator("[id$='-variables-pane']");
  const handle = page.getByRole("separator", { name: "Resize variables" });
  const height = async () => (await pane.boundingBox())!.height;
  expect(await height()).toBe(128);

  const box = (await handle.boundingBox())!;
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y - 60, { steps: 5 });
  await page.mouse.up();
  expect(await height()).toBe(188);

  await page.mouse.move(x, y - 60);
  await page.mouse.down();
  await page.mouse.move(x, y - 2000, { steps: 5 });
  await page.mouse.up();
  const query = await page
    .getByLabel("GraphQL query", { exact: true })
    .evaluate((el) => el.closest(".cm-editor")!.getBoundingClientRect().height);
  expect(query).toBeGreaterThanOrEqual(96);

  await handle.focus();
  await page.keyboard.press("Home");
  expect(await height()).toBe(64);

  const toggle = page.getByRole("button", { name: "Variables" });
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(pane).toBeHidden();
  await expect(handle).toHaveCount(0);
  await toggle.click();
  await expect(pane).toBeVisible();
  expect(await height()).toBe(64);
});
