import { test, expect } from "@playwright/test";

const fixture =
  '{"request_id":"blink-local-test","status":"ok","data":{"id":9223372036854775807,"name":"Diagnostic endpoint","active":true,"regions":["local","development"]}}';
test("compose, send, inspect and close through real controls", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  let sent:
    | { url: string; body: string | null; authorization: string | undefined }
    | undefined;
  await page.route("https://example.test/**", async (route) => {
    const request = route.request();
    sent = {
      url: request.url(),
      body: request.postData(),
      authorization: request.headers()["authorization"],
    };
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      headers: {
        "x-request-id": "local-fixture",
        "access-control-expose-headers": "x-request-id",
      },
      body: fixture,
    });
  });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "BLINK", exact: true }),
  ).toBeVisible();
  await expect(
    page.locator("header").filter({
      has: page.getByRole("heading", { name: "BLINK", exact: true }),
    }),
  ).toHaveCSS("height", "42px");
  await page.screenshot({ path: "artifacts/blink-idle.png" });
  await page
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/v1/check?existing=1");
  await page.getByLabel("Query name 1", { exact: true }).fill("region");
  await page
    .getByLabel("Query value 1", { exact: true })
    .fill("local development");
  await page.getByLabel("HTTP method").selectOption("POST");
  const requestTabs = page.getByRole("tablist", { name: "Request options" });
  await requestTabs.getByRole("tab", { name: "Body" }).click();
  await page
    .getByRole("combobox", { name: "Body", exact: true })
    .selectOption("json");
  await page
    .getByLabel("Request body", { exact: true })
    .fill('{"id":9223372036854775807,"operation":"inspect"}');
  await page.getByRole("button", { name: "Format body", exact: true }).click();
  await expect(page.getByLabel("Request body", { exact: true })).toHaveText(
    /9223372036854775807/,
  );
  await requestTabs.getByRole("tab", { name: "Auth" }).click();
  await page
    .getByLabel("Authorization", { exact: true })
    .selectOption("bearer");
  await page.getByLabel("Token", { exact: true }).fill("synthetic-test-token");
  await requestTabs.getByRole("tab", { name: "Body" }).click();
  await page.getByLabel("Request body", { exact: true }).press("Meta+Enter");
  await expect(page.locator("[data-response-status]")).toContainText("200");
  expect(sent?.url).toContain("existing=1&region=local+development");
  expect(sent?.authorization).toBe("Bearer synthetic-test-token");
  expect(sent?.body).toContain("9223372036854775807");
  await expect(page.locator("[data-response-body]")).toContainText(
    "9223372036854775807",
  );
  await page.screenshot({ path: "artifacts/blink-response.png" });
  await page.getByRole("button", { name: "Pretty", exact: true }).click();
  await expect(page.locator("[data-response-body]")).toHaveText(fixture);
  await page.getByRole("button", { name: "Wrap lines" }).click();
  await expect(
    page.getByRole("button", { name: "Wrap lines" }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.locator("[data-response-headers]").click();
  await expect(
    page.getByRole("table", { name: "Response headers" }),
  ).toContainText("local-fixture");
  await page.getByRole("button", { name: "cURL", exact: true }).click();
  await expect(page.getByRole("region", { name: "cURL export" })).toContainText(
    "INCLUDES CREDENTIALS",
  );
  await expect(page.getByRole("region", { name: "cURL export" })).toContainText(
    "--request POST",
  );
  await page.getByRole("button", { name: "New request" }).click();
  const activePane = page.locator('[data-request-pane][data-active="true"]');
  await expect(
    activePane.getByLabel("Request URL", { exact: true }),
  ).toHaveValue("");
  await expect(
    activePane.getByLabel("Request URL", { exact: true }),
  ).toBeFocused();
  const requests = page.getByRole("tablist", { name: "Requests", exact: true });
  await requests.getByRole("tab").first().click();
  await expect(
    activePane.getByLabel("Request URL", { exact: true }),
  ).toHaveValue("https://example.test/v1/check?existing=1");
  await expect(
    activePane.getByRole("table", { name: "Response headers" }),
  ).toContainText("local-fixture");
  await page.locator("[data-close-request]").first().click();
  await page.getByRole("button", { name: "Keep open", exact: true }).click();
  await expect(requests.getByRole("tab")).toHaveCount(2);
  await page.locator("[data-close-request]").first().click();
  await page.getByRole("button", { name: "Discard tab", exact: true }).click();
  await expect(requests.getByRole("tab")).toHaveCount(1);
  await expect(
    activePane.getByLabel("Request URL", { exact: true }),
  ).toHaveValue("");
  expect(errors).toEqual([]);
});

for (const width of [1180, 860, 768, 390]) {
  test(`layout at ${width}px contains controls and response`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 780 });
    await page.goto("/");
    await page
      .getByLabel("Request URL", { exact: true })
      .fill("https://example.test/" + "a".repeat(300));
    await expect(page.locator("[data-send]")).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    );
    expect(overflow).toBe(false);
    const urlBox = await page
      .getByLabel("Request URL", { exact: true })
      .boundingBox();
    const sendBox = await page.locator("[data-send]").boundingBox();
    expect(urlBox!.x + urlBox!.width).toBeLessThan(sendBox!.x);
    await page.screenshot({
      path: `artifacts/blink-${width}.png`,
      fullPage: true,
    });
  });
}

test("keyboard tabs, validation and hostile response rendering", async ({
  page,
}) => {
  await page.route("https://example.test/**", (route) =>
    route.fulfill({
      status: 500,
      body: "<script>window.injected=true</script>",
      contentType: "text/html",
    }),
  );
  await page.goto("/");
  await page
    .getByLabel("Request URL", { exact: true })
    .fill("file:///tmp/test");
  await expect(page.locator("[data-send]")).toBeDisabled();
  await expect(page.locator("[data-request-validation]")).toContainText("HTTP");
  const tabs = page.getByRole("tablist", { name: "Request options" });
  await tabs.getByRole("tab", { name: "Query" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.getByRole("tab", { name: "Headers" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await page
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/error");
  await page.getByLabel("Request URL", { exact: true }).press("Enter");
  await expect(page.locator("[data-response-status]")).toHaveAttribute(
    "data-tone",
    "error",
  );
  await expect(page.locator("[data-response-body]")).toContainText("<script>");
  expect(await page.evaluate(() => "injected" in window)).toBe(false);
});
