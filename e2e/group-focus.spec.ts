import { expect, test } from "@playwright/test";
import { createSession } from "../src/lib/session";
import { encodeWorkspace, WORKSPACE_KEY } from "../src/lib/workspace";

test("focus shows one group and its tabs, then restores the view", async ({
  page,
}) => {
  const billing = createSession();
  billing.groupId = 601;
  billing.draft.url = "https://example.test/invoices";
  const identity = createSession();
  identity.groupId = 602;
  identity.draft.url = "https://example.test/users";
  const snapshot = encodeWorkspace(
    [billing, identity],
    billing.id,
    [
      { id: 601, name: "Billing", parentId: null, collapsed: false },
      { id: 602, name: "Identity", parentId: null, collapsed: false },
    ],
    {},
  );
  await page.addInitScript(
    ({ key, snapshot }) => {
      if (!localStorage.getItem(key)) localStorage.setItem(key, snapshot);
    },
    { key: WORKSPACE_KEY, snapshot },
  );
  await page.goto("/");
  const tabs = page.locator("[data-tab-id]");
  const browser = page.locator("[data-browser-list]");
  await expect(tabs).toHaveCount(2);

  await page.getByRole("button", { name: "More actions for Identity" }).click();
  await page.getByRole("menuitem", { name: "Focus" }).click();

  await expect(page.locator("[data-browser-focus]")).toContainText("Identity");
  await expect(browser).not.toContainText("Billing");
  await expect(tabs).toHaveCount(1);
  await expect(tabs.getByRole("tab", { selected: true })).toContainText(
    "/users",
  );

  await page.getByRole("button", { name: "Unfocus Identity" }).focus();
  await page.keyboard.press("Escape");

  await expect(page.locator("[data-browser-focus]")).toHaveCount(0);
  await expect(browser).toContainText("Billing");
  await expect(tabs).toHaveCount(2);
  await expect(tabs.getByRole("tab", { selected: true })).toContainText(
    "/invoices",
  );
});
