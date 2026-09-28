import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
import RequestTabs from "@/components/RequestTabs.vue";
import { useDragDrop } from "@/composables/useDragDrop";
import { createSession } from "@/lib/session";

const first = createSession();
const second = createSession();
const third = createSession();
const tabLeft: Record<string, number> = {
  [`tab-${first.id}`]: 0,
  [`tab-${second.id}`]: 210,
  [`tab-${third.id}`]: 420,
};

function rect(left: number, top: number, width: number, height: number) {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON() {},
  } as DOMRect;
}
function pointer(type: string, x: number, y = 18) {
  return new PointerEvent(type, {
    bubbles: true,
    button: 0,
    clientX: x,
    clientY: y,
    pointerType: "mouse",
  });
}

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(
    function (this: Element) {
      const el = this as HTMLElement;
      if (
        el.hasAttribute("data-tab-bar") ||
        el.getAttribute("role") === "tablist"
      )
        return rect(0, 0, 1000, 36);
      const left = tabLeft[el.dataset?.dropKey ?? ""];
      return left === undefined ? rect(0, 0, 0, 0) : rect(left, 0, 210, 36);
    },
  );
});
const mounted: VueWrapper[] = [];
afterEach(() => {
  useDragDrop().cancel();
  mounted.splice(0).forEach((wrapper) => wrapper.unmount());
  vi.restoreAllMocks();
});

function setup() {
  const onOpenRequests = vi.fn();
  const onSelect = vi.fn();
  const wrapper = mount(RequestTabs, {
    props: {
      sessions: [first, second, third],
      activeId: first.id,
      onOpenRequests,
      onSelect,
    },
    attachTo: document.body,
  });
  mounted.push(wrapper);
  return { wrapper, onOpenRequests, onSelect };
}

describe("RequestTabs drag and drop", () => {
  it("reorders a tab after another", async () => {
    const { wrapper, onOpenRequests } = setup();
    wrapper
      .get(`[data-drop-key="tab-${first.id}"]`)
      .element.dispatchEvent(pointer("pointerdown", 100));
    window.dispatchEvent(pointer("pointermove", 110));
    window.dispatchEvent(pointer("pointermove", 400));
    await nextTick();
    expect(
      wrapper
        .get(`[data-drop-key="tab-${second.id}"] [data-drop-indicator]`)
        .attributes("data-drop-indicator"),
    ).toBe("after");
    window.dispatchEvent(pointer("pointerup", 400));
    expect(onOpenRequests).toHaveBeenCalledWith([first.id], third.id);
  });

  it("appends when dropped past the last tab", async () => {
    const { wrapper, onOpenRequests } = setup();
    wrapper
      .get(`[data-drop-key="tab-${first.id}"]`)
      .element.dispatchEvent(pointer("pointerdown", 100));
    window.dispatchEvent(pointer("pointermove", 110));
    window.dispatchEvent(pointer("pointermove", 900));
    window.dispatchEvent(pointer("pointerup", 900));
    expect(onOpenRequests).toHaveBeenCalledWith([first.id], null);
  });

  it("does not start a drag from the close button", async () => {
    const { wrapper } = setup();
    wrapper
      .get("[data-close-request]")
      .element.dispatchEvent(pointer("pointerdown", 190));
    window.dispatchEvent(pointer("pointermove", 400));
    expect(useDragDrop().state.payload).toBeNull();
  });

  it("does not select a tab after a drop", async () => {
    const { wrapper, onSelect } = setup();
    wrapper
      .get(`[data-drop-key="tab-${first.id}"]`)
      .element.dispatchEvent(pointer("pointerdown", 100));
    window.dispatchEvent(pointer("pointermove", 110));
    window.dispatchEvent(pointer("pointermove", 400));
    window.dispatchEvent(pointer("pointerup", 400));
    await wrapper.get(`#request-tab-${second.id}`).trigger("click");
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("moves the active tab with Alt+ArrowRight", async () => {
    const { wrapper, onOpenRequests } = setup();
    await wrapper
      .get(`#request-tab-${first.id}`)
      .trigger("keydown", { key: "ArrowRight", altKey: true });
    expect(onOpenRequests).toHaveBeenCalledWith([first.id], third.id);
  });
});
