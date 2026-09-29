/** The label of each open menu item, without its shortcut hint. */
export function menuLabels(root: ParentNode = document.body) {
  return [...root.querySelectorAll('[role^="menuitem"]')].map((el) => {
    const copy = el.cloneNode(true) as HTMLElement;
    copy
      .querySelectorAll('[data-slot$="shortcut"]')
      .forEach((hint) => hint.remove());
    return copy.textContent?.trim() ?? "";
  });
}

/**
 * Open a submenu from its trigger. jsdom does not open a Reka submenu on
 * click, so press ArrowRight as a keyboard user does.
 */
export async function openSubmenu(trigger: Element | null) {
  // Let the parent menu finish mounting first.
  await new Promise((resolve) => setTimeout(resolve, 0));
  trigger?.dispatchEvent(
    new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
  );
  // The submenu mounts after Reka's open-state and popper updates settle.
  await new Promise((resolve) => setTimeout(resolve, 20));
}
