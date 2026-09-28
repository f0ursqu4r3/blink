import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDragDrop, type DropHit } from "@/composables/useDragDrop";

const drag = useDragDrop();
let surfaceEl: HTMLElement;
let unregister: () => void;
let hit: DropHit | null;
const commit = vi.fn();
const expand = vi.fn();

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
function pointer(
  type: string,
  x: number,
  y: number,
  init: PointerEventInit = {},
) {
  return new PointerEvent(type, {
    bubbles: true,
    button: 0,
    clientX: x,
    clientY: y,
    pointerType: "mouse",
    ...init,
  });
}
const move = (x: number, y: number) =>
  window.dispatchEvent(pointer("pointermove", x, y));
const up = (x: number, y: number) =>
  window.dispatchEvent(pointer("pointerup", x, y));
const source = {
  payload: () => ({ kind: "requests" as const, ids: [1] }),
  preview: () => ({ label: "one" }),
};

beforeEach(() => {
  surfaceEl = document.createElement("div");
  surfaceEl.getBoundingClientRect = () => rect(0, 0, 200, 400);
  document.body.append(surfaceEl);
  hit = { key: "request-2", zone: "before", commit, expand };
  unregister = drag.registerSurface({
    el: () => surfaceEl,
    axis: "y",
    resolve: () => hit,
  });
});
afterEach(() => {
  drag.cancel();
  unregister();
  surfaceEl.remove();
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("useDragDrop", () => {
  it("starts only after 4px of movement", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(102, 101);
    expect(drag.state.payload).toBeNull();
    move(100, 105);
    expect(drag.state.payload).toEqual({ kind: "requests", ids: [1] });
    expect(drag.state.preview).toEqual({ label: "one" });
    expect(drag.state.hit).toEqual({ key: "request-2", zone: "before" });
    expect(document.documentElement.hasAttribute("data-dragging")).toBe(true);
  });

  it("commits on release and swallows the next click", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    up(100, 120);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(drag.state.payload).toBeNull();
    const click = vi.fn();
    surfaceEl.addEventListener("click", click);
    surfaceEl.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(click).not.toHaveBeenCalled();
  });

  it("does not swallow a click when no drag started", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    up(100, 100);
    const click = vi.fn();
    surfaceEl.addEventListener("click", click);
    surfaceEl.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(click).toHaveBeenCalledTimes(1);
  });

  it("cancels on Escape without committing", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    up(100, 120);
    expect(commit).not.toHaveBeenCalled();
    expect(drag.state.payload).toBeNull();
  });

  it("cancels on window blur", () => {
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    window.dispatchEvent(new Event("blur"));
    expect(drag.state.payload).toBeNull();
  });

  it("ignores non-primary buttons and presses inside inputs", () => {
    drag.startPress(pointer("pointerdown", 100, 100, { button: 2 }), source);
    move(100, 120);
    expect(drag.state.payload).toBeNull();
    const input = document.createElement("input");
    surfaceEl.append(input);
    const event = pointer("pointerdown", 100, 100);
    input.addEventListener("pointerdown", (pressed) =>
      drag.startPress(pressed as PointerEvent, source),
    );
    input.dispatchEvent(event);
    move(100, 120);
    expect(drag.state.payload).toBeNull();
  });

  it("marks the document when no target accepts", () => {
    hit = null;
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    expect(drag.state.hit).toBeNull();
    expect(document.documentElement.hasAttribute("data-drag-invalid")).toBe(
      true,
    );
  });

  it("expands a folder after hovering the into zone for 600ms", () => {
    vi.useFakeTimers();
    hit = { key: "group-1", zone: "into", commit, expand };
    drag.startPress(pointer("pointerdown", 100, 100), source);
    move(100, 120);
    vi.advanceTimersByTime(599);
    expect(expand).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(expand).toHaveBeenCalledTimes(1);
  });

  it("starts a touch drag after a 250ms hold, and not after a swipe", () => {
    vi.useFakeTimers();
    drag.startPress(
      pointer("pointerdown", 100, 100, { pointerType: "touch" }),
      source,
    );
    vi.advanceTimersByTime(250);
    expect(drag.state.payload).not.toBeNull();
    drag.cancel();
    drag.startPress(
      pointer("pointerdown", 100, 100, { pointerType: "touch" }),
      source,
    );
    move(100, 115);
    vi.advanceTimersByTime(250);
    expect(drag.state.payload).toBeNull();
  });
});
