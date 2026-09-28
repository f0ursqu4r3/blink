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
