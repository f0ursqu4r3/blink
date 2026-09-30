import { expect, test } from "@playwright/test";

test("the title bar stacks request above response and restores it", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  const pane = page.locator('[data-request-pane][data-active="true"]');
  const request = pane.locator(".panels > :first-child");
  const response = pane.locator(".panels > :last-child");
  const handle = pane.locator("[data-panel-resize]");

  const toggle = page.getByRole("button", {
    name: "Stack request above response",
  });
  await toggle.click();
  await expect(pane.locator(".panels")).toHaveAttribute(
    "data-layout",
    "vertical",
  );
  const top = (await request.boundingBox())!;
  const bottom = (await response.boundingBox())!;
  expect(bottom.y).toBeGreaterThan(top.y + top.height - 1);
  expect(Math.round(top.width)).toBe(Math.round(bottom.width));

  // Dragging the handle down makes the request pane taller.
  const bar = (await handle.boundingBox())!;
  await page.mouse.move(bar.x + bar.width / 2, bar.y + bar.height / 2);
  await page.mouse.down();
  await page.mouse.move(bar.x + bar.width / 2, bar.y + 80);
  await page.mouse.up();
  expect((await request.boundingBox())!.height).toBeGreaterThan(
    top.height + 40,
  );

  await page.reload();
  await expect(pane.locator(".panels")).toHaveAttribute(
    "data-layout",
    "vertical",
  );
  await page
    .getByRole("button", { name: "Place request and response side by side" })
    .click();
  const left = (await request.boundingBox())!;
  const right = (await response.boundingBox())!;
  expect(right.x).toBeGreaterThan(left.x + left.width - 1);
  await page.screenshot({ path: "artifacts/pane-layout-side-by-side.png" });
  await toggle.click();
  await page.screenshot({ path: "artifacts/pane-layout-stacked.png" });
});
