import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import { nextTick } from "vue";
import RequestBrowser from "@/components/RequestBrowser.vue";
import { useDragDrop } from "@/composables/useDragDrop";
import { createSession, type RequestSession } from "@/lib/session";
import type { RequestGroup } from "@/lib/groups";

// Row layout: each key maps to [top, height]; rows are 240px wide.
let layout: Record<string, [number, number]> = {};
const mounted: VueWrapper[] = [];

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
function pointer(type: string, x: number, y: number) {
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
      if (el.hasAttribute("data-browser-list")) return rect(0, 0, 240, 600);
      const row = layout[el.dataset?.dropKey ?? ""];
      return row ? rect(0, row[0], 240, row[1]) : rect(0, 0, 0, 0);
    },
  );
});
afterEach(() => {
  useDragDrop().cancel();
  mounted.splice(0).forEach((wrapper) => wrapper.unmount());
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function setup(
  sessions: RequestSession[],
  groups: RequestGroup[],
  extra: Record<string, unknown> = {},
) {
  const handlers = {
    onMoveRequests: vi.fn(),
    onMoveGroup: vi.fn(),
    onSelect: vi.fn(),
    onToggleGroup: vi.fn(),
    onUpdateSelection: vi.fn(),
  };
  // Attach so window-level click suppression sees row clicks.
  const wrapper = mount(RequestBrowser, {
    props: { sessions, groups, activeId: null, ...handlers, ...extra },
    attachTo: document.body,
  });
  mounted.push(wrapper);
  return { wrapper, ...handlers };
}

async function dragTo(wrapper: VueWrapper, fromKey: string, y: number) {
  const from = wrapper.get(`[data-drop-key="${fromKey}"]`).element;
  const [top, height] = layout[fromKey];
  from.dispatchEvent(pointer("pointerdown", 20, top + height / 2));
  window.dispatchEvent(pointer("pointermove", 20, top + height / 2 + 10));
  window.dispatchEvent(pointer("pointermove", 20, y));
  await nextTick();
}
const release = (y: number) =>
  window.dispatchEvent(pointer("pointerup", 20, y));

describe("RequestBrowser drag and drop", () => {
  const first = createSession();
  const second = createSession();
  const platform: RequestGroup = {
    id: 1,
    name: "Platform",
    parentId: null,
    collapsed: false,
  };

  beforeEach(() => {
    layout = {
      "root-start": [0, 28],
      [`request-${first.id}`]: [28, 27],
      [`request-${second.id}`]: [55, 27],
      "group-1": [82, 28],
      "root-end": [110, 32],
    };
  });

  it("reorders a request after the last ungrouped request", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform]);
    await dragTo(wrapper, `request-${first.id}`, 78);
    expect(
      wrapper
        .get(`[data-drop-key="request-${second.id}"] [data-drop-indicator]`)
        .attributes("data-drop-indicator"),
    ).toBe("after");
    release(78);
    expect(onMoveRequests).toHaveBeenCalledWith([first.id], null, null);
  });

  it("moves a request into a folder", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform]);
    await dragTo(wrapper, `request-${first.id}`, 96);
    expect(
      wrapper.get('[data-drop-key="group-1"]').attributes("data-drop-target"),
    ).toBe("into");
    release(96);
    expect(onMoveRequests).toHaveBeenCalledWith([first.id], 1, null);
  });

  it("drags the whole selection", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform], {
      selectedIds: [first.id, second.id],
    });
    await dragTo(wrapper, `request-${second.id}`, 96);
    release(96);
    expect(onMoveRequests).toHaveBeenCalledWith([first.id, second.id], 1, null);
  });

  it("un-nests a folder to the root", async () => {
    const child: RequestGroup = {
      id: 2,
      name: "Child",
      parentId: 1,
      collapsed: false,
    };
    layout["group-2"] = [110, 28];
    layout["root-end"] = [138, 32];
    const { wrapper, onMoveGroup } = setup([first, second], [platform, child]);
    await dragTo(wrapper, "group-2", 150);
    release(150);
    expect(onMoveGroup).toHaveBeenCalledWith(2, null, null);
  });

  it("does not nest a folder inside its own child", async () => {
    const child: RequestGroup = {
      id: 2,
      name: "Child",
      parentId: 1,
      collapsed: false,
    };
    layout["group-2"] = [110, 28];
    layout["root-end"] = [138, 32];
    const { wrapper, onMoveGroup } = setup([first, second], [platform, child]);
    await dragTo(wrapper, "group-1", 124);
    expect(wrapper.find("[data-drop-target]").exists()).toBe(false);
    release(124);
    expect(onMoveGroup).not.toHaveBeenCalled();
  });

  it("does not select the row under the pointer after a drop", async () => {
    const { wrapper, onSelect } = setup([first, second], [platform]);
    await dragTo(wrapper, `request-${first.id}`, 78);
    release(78);
    await wrapper
      .get(`[data-drop-key="request-${second.id}"]`)
      .trigger("click");
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("keeps shift-click range selection", async () => {
    const { wrapper, onUpdateSelection } = setup([first, second], [platform], {
      selectedIds: [first.id],
      selectionAnchorId: first.id,
    });
    await wrapper
      .get(`[data-drop-key="request-${second.id}"]`)
      .trigger("click", { shiftKey: true });
    expect(onUpdateSelection).toHaveBeenCalledWith(
      [first.id, second.id],
      first.id,
    );
  });

  it("expands a collapsed folder after a hover", async () => {
    vi.useFakeTimers();
    const { wrapper, onToggleGroup } = setup(
      [first, second],
      [{ ...platform, collapsed: true }],
    );
    await dragTo(wrapper, `request-${first.id}`, 96);
    vi.advanceTimersByTime(600);
    expect(onToggleGroup).toHaveBeenCalledWith(1);
  });

  it("does not open the context menu while a touch drag is held still", async () => {
    vi.useFakeTimers();
    const { wrapper } = setup([first, second], [platform]);
    const row = wrapper.get(`[data-drop-key="request-${first.id}"]`).element;
    row.dispatchEvent(
      new PointerEvent("pointerdown", {
        bubbles: true,
        button: 0,
        clientX: 20,
        clientY: 40,
        pointerType: "touch",
      }),
    );
    // Reka starts its long-press timer one tick after pointerdown.
    await nextTick();
    vi.advanceTimersByTime(250);
    expect(useDragDrop().state.payload).not.toBeNull();
    // Reka also clears the timer one tick after the pointermove.
    await nextTick();
    vi.advanceTimersByTime(1000);
    await nextTick();
    expect(document.querySelector('[data-state="open"]')).toBeNull();
  });

  it("moves the selection down with Alt+ArrowDown", async () => {
    const { wrapper, onMoveRequests } = setup([first, second], [platform]);
    await wrapper
      .get(`[data-drop-key="request-${first.id}"]`)
      .trigger("keydown", { key: "ArrowDown", altKey: true });
    expect(onMoveRequests).toHaveBeenCalledWith([first.id], null, null);
  });
});
