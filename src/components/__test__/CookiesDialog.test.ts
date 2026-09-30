import { afterEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

vi.mock("@/lib/transport", () => ({ nativeTransport: true }));
const cookies = [
  {
    domain: "api.example.test",
    path: "/",
    name: "session",
    value: "abc",
    expires: null,
    secure: true,
    httpOnly: true,
  },
  {
    domain: "other.test",
    path: "/v1",
    name: "pref",
    value: "dark",
    expires: 1_900_000_000_000,
    secure: false,
    httpOnly: false,
  },
];
const invoke = vi.fn(async (command: string, _args?: unknown) =>
  command === "list_cookies" ? cookies : undefined,
);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => invoke(command, args),
  isTauri: () => true,
}));

import CookiesDialog from "../CookiesDialog.vue";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount());
  invoke.mockClear();
});

describe("CookiesDialog", () => {
  async function open() {
    const wrapper = mount(CookiesDialog, {
      attachTo: document.body,
      props: { open: true, enabled: true },
    });
    wrappers.push(wrapper);
    await flushPromises();
    return wrapper;
  }
  it("lists and filters cookies", async () => {
    await open();
    expect(document.querySelectorAll("[data-cookie]").length).toBe(2);
    expect(document.body.textContent).toContain("2 COOKIES · 2 DOMAINS");
    const filter = document.querySelector<HTMLInputElement>(
      "[data-cookie-filter]",
    )!;
    filter.value = "dark";
    filter.dispatchEvent(new Event("input"));
    await flushPromises();
    expect(document.querySelectorAll("[data-cookie]").length).toBe(1);
  });
  it("deletes one cookie and clears all", async () => {
    await open();
    document
      .querySelector<HTMLButtonElement>(
        '[aria-label="Delete cookie session for api.example.test"]',
      )!
      .click();
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("delete_cookie", {
      domain: "api.example.test",
      path: "/",
      name: "session",
    });
    document.querySelector<HTMLButtonElement>("[data-clear-cookies]")!.click();
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("clear_cookies", undefined);
  });
  it("warns when storage is off", async () => {
    const wrapper = mount(CookiesDialog, {
      attachTo: document.body,
      props: { open: true, enabled: false },
    });
    wrappers.push(wrapper);
    await flushPromises();
    expect(document.body.textContent).toContain("Cookie storage is off");
  });
});
