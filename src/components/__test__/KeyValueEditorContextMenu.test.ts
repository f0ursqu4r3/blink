import { afterEach, describe, expect, it } from "vitest";
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

  it("shows 'Disable' when row is enabled", async () => {
    const p = { ...pair(), enabled: true };
    const { wrapper } = render([p]);
    await wrapper
      .get(`[data-testid="kv-row-ctx-trigger-${p.id}"]`)
      .trigger("contextmenu");
    expect(
      document.body.querySelector('[data-testid="kv-row-ctx-toggle"]'),
    ).not.toBeNull();
    expect(
      (
        document.body.querySelector(
          '[data-testid="kv-row-ctx-toggle"]',
        ) as HTMLElement
      ).textContent,
    ).toContain("Disable");
  });

  it("shows 'Enable' when row is disabled", async () => {
    const p = { ...pair(), enabled: false };
    const { wrapper } = render([p]);
    await wrapper
      .get(`[data-testid="kv-row-ctx-trigger-${p.id}"]`)
      .trigger("contextmenu");
    expect(
      (
        document.body.querySelector(
          '[data-testid="kv-row-ctx-toggle"]',
        ) as HTMLElement
      ).textContent,
    ).toContain("Enable");
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
