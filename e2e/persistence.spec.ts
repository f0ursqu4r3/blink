import { test, expect, chromium } from "@playwright/test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const activePane = '[data-request-pane][data-active="true"]';
test("a fresh browser process restores tabs, request data, response and view state without sending", async () => {
  const directory = await mkdtemp(join(tmpdir(), "blink-restart-"));
  let context = await chromium.launchPersistentContext(directory, {
    headless: true,
    viewport: { width: 1180, height: 780 },
  });
  try {
    let page = await context.newPage();
    await page.route("https://example.test/**", (route) =>
      route.fulfill({
        status: 201,
        contentType: "application/json",
        body: '{"id":9223372036854775807,"saved":true}',
        headers: {
          "x-saved": "yes",
          "access-control-expose-headers": "x-saved",
        },
      }),
    );
    await page.goto("http://127.0.0.1:1420/");
    let pane = page.locator(activePane);
    await pane
      .getByLabel("Request URL", { exact: true })
      .fill("https://example.test/original");
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
      .fill('{"id":9223372036854775807}');
    await pane.getByRole("button", { name: /Send/ }).click();
    await expect(pane.locator("[data-response-status]")).toContainText("201");
    await pane.getByRole("button", { name: "Pretty", exact: true }).click();
    await pane.getByRole("button", { name: "Wrap lines" }).click();
    await page
      .getByRole("button", { name: "New request", exact: true })
      .click();
    await pane
      .getByLabel("Request URL", { exact: true })
      .fill("https://example.test/second");
    await expect(
      page.getByText("SAVED LOCALLY", { exact: true }),
    ).toBeVisible();
    await context.close();
    context = await chromium.launchPersistentContext(directory, {
      headless: true,
      viewport: { width: 1180, height: 780 },
    });
    page = await context.newPage();
    const requests: string[] = [];
    await page.route("https://example.test/**", (route) => {
      requests.push(route.request().url());
      return route.abort();
    });
    await page.goto("http://127.0.0.1:1420/");
    pane = page.locator(activePane);
    const tabs = page
      .getByRole("tablist", { name: "Requests", exact: true })
      .getByRole("tab");
    await expect(tabs).toHaveCount(2);
    await expect(pane.getByLabel("Request URL", { exact: true })).toHaveValue(
      "https://example.test/second",
    );
    await tabs.first().click();
    await expect(pane.getByLabel("HTTP method")).toHaveValue("POST");
    await expect(pane.getByLabel("Request body", { exact: true })).toHaveValue(
      '{"id":9223372036854775807}',
    );
    await expect(pane.locator("[data-response-status]")).toContainText("201");
    await expect(pane.locator("[data-response-body]")).toContainText(
      "9223372036854775807",
    );
    await expect(
      pane.getByRole("button", { name: "Raw", exact: true }),
    ).toBeVisible();
    await expect(
      pane.getByRole("button", { name: "Wrap lines" }),
    ).toHaveAttribute("aria-pressed", "true");
    await pane
      .getByRole("tablist", { name: "Response view" })
      .getByRole("tab", { name: /Headers/ })
      .click();
    await expect(
      pane.getByRole("table", { name: "Response headers" }),
    ).toContainText("x-saved");
    await page.reload();
    await expect(
      pane.getByRole("table", { name: "Response headers" }),
    ).toBeVisible();
    expect(requests).toEqual([]);
    await page.screenshot({ path: "artifacts/blink-restored.png" });
  } finally {
    await context.close();
    await rm(directory, { recursive: true, force: true });
  }
});

test("a restart marks an unfinished request interrupted and never replays it", async ({
  page,
}) => {
  let sends = 0;
  await page.route("https://example.test/**", () => {
    sends++;
  });
  await page.goto("/");
  const pane = page.locator(activePane);
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/unfinished");
  await pane.getByRole("button", { name: /Send/ }).click();
  await expect.poll(() => sends).toBe(1);
  await expect(page.getByText("SAVED LOCALLY", { exact: true })).toBeVisible();
  await page.reload();
  await expect(
    pane.getByText(/Request interrupted when Blink closed/),
  ).toBeVisible();
  await expect(pane.getByRole("button", { name: /Send/ })).toBeEnabled();
  expect(sends).toBe(1);
});

test("response scroll position survives reload", async ({ page }) => {
  await page.route("https://example.test/**", (route) =>
    route.fulfill({
      contentType: "text/plain",
      body: Array.from({ length: 200 }, (_, i) => `Line ${i}`).join("\n"),
    }),
  );
  await page.goto("/");
  const pane = page.locator(activePane);
  await pane
    .getByLabel("Request URL", { exact: true })
    .fill("https://example.test/scroll");
  await pane.getByRole("button", { name: /Send/ }).click();
  const responseScroller = pane.locator("[data-virtual-scroller]");
  await expect(responseScroller).toBeVisible();
  await responseScroller.evaluate((el) => {
    el.scrollTop = 320;
  });
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          JSON.parse(localStorage.getItem("blink.workspace.v1")!).tabs[0].view
            .responseScroll,
      ),
    )
    .toBe(320);
  await page.reload();
  await expect
    .poll(() => responseScroller.evaluate((el) => el.scrollTop))
    .toBe(320);
});
