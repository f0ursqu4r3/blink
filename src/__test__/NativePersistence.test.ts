import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import App from "../App.vue";

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  beforeExit: () => {},
  stop: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: native.invoke,
  isTauri: () => true,
}));
vi.mock("@/lib/transport", () => ({
  nativeTransport: true,
  sendRequest: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_: string, listener: () => void) => {
    native.beforeExit = listener;
    return native.stop;
  }),
}));
const wrappers: ReturnType<typeof mount>[] = [];
beforeEach(() => {
  native.invoke.mockReset();
  native.invoke.mockImplementation(async (name: string) =>
    name === "load_app_state" ? null : undefined,
  );
});
afterEach(async () => {
  wrappers.splice(0).forEach((w) => w.unmount());
  await flushPromises();
});
const render = async () => {
  const app = mount(App, { attachTo: document.body });
  wrappers.push(app);
  await flushPromises();
  return app;
};

it("waits for the latest native save before allowing application exit", async () => {
  const app = await render();
  let release!: () => void;
  const writes: string[] = [];
  native.invoke.mockImplementation(
    async (name: string, args?: { content: string }) => {
      if (name === "save_app_state") {
        writes.push(args!.content);
        if (writes.length === 1)
          await new Promise<void>((resolve) => {
            release = resolve;
          });
      }
    },
  );
  await app.get("[data-request-url]").setValue("https://example.test/first");
  await flushPromises();
  await app.get("[data-request-url]").setValue("https://example.test/latest");
  native.beforeExit();
  await flushPromises();
  expect(app.get("main").attributes("inert")).toBeDefined();
  expect(
    native.invoke.mock.calls.some((call) => call[0] === "finish_app_exit"),
  ).toBe(false);
  release();
  await flushPromises();
  expect(JSON.parse(writes[writes.length - 1]).tabs[0].draft.url).toBe(
    "https://example.test/latest",
  );
  expect(
    native.invoke.mock.calls.some((call) => call[0] === "finish_app_exit"),
  ).toBe(true);
});

it("keeps the app open on exit-save failure until the user explicitly discards changes", async () => {
  const app = await render();
  native.invoke.mockImplementation(async (name: string) => {
    if (name === "save_app_state") throw new Error("Disk full");
  });
  await app.get("[data-request-url]").setValue("https://example.test/unsaved");
  await flushPromises();
  native.beforeExit();
  await flushPromises();
  expect(app.get("main").attributes("inert")).toBeUndefined();
  expect(
    native.invoke.mock.calls.some((call) => call[0] === "finish_app_exit"),
  ).toBe(false);
  await app
    .findAll("button")
    .find((button) => button.text() === "Quit without saving")!
    .trigger("click");
  await flushPromises();
  expect(
    native.invoke.mock.calls.some((call) => call[0] === "finish_app_exit"),
  ).toBe(true);
});

it("does not overwrite unreadable native state on load or quit", async () => {
  native.invoke.mockImplementation(async (name: string) =>
    name === "load_app_state" ? "broken" : undefined,
  );
  const app = await render();
  expect(app.find("[data-request-url]").exists()).toBe(false);
  native.beforeExit();
  await flushPromises();
  expect(
    native.invoke.mock.calls.some((call) => call[0] === "save_app_state"),
  ).toBe(false);
  expect(
    native.invoke.mock.calls.some((call) => call[0] === "finish_app_exit"),
  ).toBe(true);
});
