import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { ref } from "vue";
import KeyValueEditor from "../KeyValueEditor.vue";
import { pair, type Pair } from "@/lib/request";

const wrappers: ReturnType<typeof mount>[] = [];
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount());
  document.body.innerHTML = "";
});

function render(initialRows?: Pair[]) {
  const modelValue = ref<Pair[]>(initialRows ?? [pair()]);
  const wrapper = mount(KeyValueEditor, {
    props: {
      label: "Query",
      modelValue: modelValue.value,
      "onUpdate:modelValue": (v: Pair[]) => {
        modelValue.value = v;
        wrapper.setProps({ modelValue: v });
      },
    },
    attachTo: document.body,
  });
  wrappers.push(wrapper);
  return { wrapper, modelValue };
}

// ── Table blank area context menu ──────────────────────────────────────────

describe("KeyValueEditor table context menu", () => {
  it("opens on right-click of table trigger", async () => {
    const { wrapper } = render();
    await wrapper
      .get('[data-testid="kv-table-ctx-trigger"]')
      .trigger("contextmenu");
    expect(
      document.body.querySelector('[data-testid="kv-ctx-add-row"]'),
    ).not.toBeNull();
  });

  it("'Add row' appends a new pair", async () => {
    const { wrapper, modelValue } = render([pair()]);
    await wrapper
      .get('[data-testid="kv-table-ctx-trigger"]')
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="kv-ctx-add-row"]',
      ) as HTMLElement
    ).click();
    expect(modelValue.value.length).toBe(2);
  });

  it("'Enable all' sets all rows enabled", async () => {
    const p1 = { ...pair(), enabled: false };
    const p2 = { ...pair(), enabled: false };
    const { wrapper, modelValue } = render([p1, p2]);
    await wrapper
      .get('[data-testid="kv-table-ctx-trigger"]')
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="kv-ctx-enable-all"]',
      ) as HTMLElement
    ).click();
    expect(modelValue.value.every((r) => r.enabled)).toBe(true);
  });

  it("'Disable all' sets all rows disabled", async () => {
    const p1 = { ...pair(), enabled: true };
    const p2 = { ...pair(), enabled: true };
    const { wrapper, modelValue } = render([p1, p2]);
    await wrapper
      .get('[data-testid="kv-table-ctx-trigger"]')
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="kv-ctx-disable-all"]',
      ) as HTMLElement
    ).click();
    expect(modelValue.value.every((r) => !r.enabled)).toBe(true);
  });
});

// ── Row context menu ──────────────────────────────────────────────────────

describe("KeyValueEditor row context menu", () => {
  it("opens on right-click of row trigger", async () => {
    const p = pair();
    const { wrapper } = render([p]);
    await wrapper
      .get(`[data-testid="kv-row-ctx-trigger-${p.id}"]`)
      .trigger("contextmenu");
    expect(
      document.body.querySelector('[data-testid="kv-row-ctx-remove"]'),
    ).not.toBeNull();
  });

  it("toggle enabled updates that row", async () => {
    const p = { ...pair(), enabled: true };
    const { wrapper, modelValue } = render([p]);
    await wrapper
      .get(`[data-testid="kv-row-ctx-trigger-${p.id}"]`)
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="kv-row-ctx-toggle"]',
      ) as HTMLElement
    ).click();
    expect(modelValue.value[0].enabled).toBe(false);
  });

  it("'Duplicate' inserts a copy after the row", async () => {
    const p = { ...pair(), key: "x", value: "y" };
    const { wrapper, modelValue } = render([p]);
    await wrapper
      .get(`[data-testid="kv-row-ctx-trigger-${p.id}"]`)
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="kv-row-ctx-duplicate"]',
      ) as HTMLElement
    ).click();
    expect(modelValue.value.length).toBe(2);
    expect(modelValue.value[1].key).toBe("x");
    expect(modelValue.value[1].value).toBe("y");
    expect(modelValue.value[1].id).not.toBe(p.id);
  });

  it("'Remove' deletes that row", async () => {
    const p1 = pair();
    const p2 = pair();
    const { wrapper, modelValue } = render([p1, p2]);
    await wrapper
      .get(`[data-testid="kv-row-ctx-trigger-${p1.id}"]`)
      .trigger("contextmenu");
    (
      document.body.querySelector(
        '[data-testid="kv-row-ctx-remove"]',
      ) as HTMLElement
    ).click();
    expect(modelValue.value.length).toBe(1);
    expect(modelValue.value[0].id).toBe(p2.id);
  });
});

describe("KeyValueEditor row menu items", () => {
  async function openRow(wrapper: ReturnType<typeof mount>, id: number) {
    await wrapper
      .get(`[data-testid="kv-row-ctx-trigger-${id}"]`)
      .trigger("contextmenu");
  }

  it("shows Enabled as a checked checkbox item and a destructive Delete row", async () => {
    const row = pair("a", "1");
    const { wrapper } = render([row]);
    await openRow(wrapper, row.id);
    const toggle = document.body.querySelector(
      '[data-testid="kv-row-ctx-toggle"]',
    )!;
    expect(toggle.getAttribute("role")).toBe("menuitemcheckbox");
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    const remove = document.body.querySelector(
      '[data-testid="kv-row-ctx-remove"]',
    )!;
    expect(remove.getAttribute("data-variant")).toBe("destructive");
    expect(remove.textContent?.trim()).toBe("Delete row");
  });

  it("Copy name copies the row key", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    const row = pair("X-Id", "7");
    const { wrapper } = render([row]);
    await openRow(wrapper, row.id);
    (
      document.body.querySelector(
        '[data-testid="kv-row-ctx-copy-name"]',
      ) as HTMLElement
    ).click();
    expect(writeText).toHaveBeenCalledWith("X-Id");
    vi.unstubAllGlobals();
  });
});
