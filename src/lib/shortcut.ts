const isMac =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad/i.test(navigator.platform);

const modifiers = ["mod", "shift", "alt"];
// VS Code order: ⌥⇧⌘ on macOS, Ctrl+Alt+Shift elsewhere.
const macOrder = ["alt", "shift", "mod"];
const wordOrder = ["mod", "alt", "shift"];
const macSymbols: Record<string, string> = {
  alt: "⌥",
  shift: "⇧",
  mod: "⌘",
  enter: "↵",
  esc: "Esc",
};
const words: Record<string, string> = {
  alt: "Alt",
  shift: "Shift",
  mod: "Ctrl",
  enter: "Enter",
  esc: "Esc",
};

/**
 * A shortcut hint for menus, such as "⇧⌘D" on macOS and "Ctrl+Shift+D"
 * elsewhere. Keys: "mod", "shift", "alt", "enter", "esc", or one character.
 */
export function shortcutLabel(keys: string[], mac = isMac) {
  const order = mac ? macOrder : wordOrder;
  const names = mac ? macSymbols : words;
  const sorted = [
    ...keys
      .filter((key) => modifiers.includes(key))
      .sort((a, b) => order.indexOf(a) - order.indexOf(b)),
    ...keys.filter((key) => !modifiers.includes(key)),
  ];
  return sorted
    .map((key) => names[key] ?? key.toUpperCase())
    .join(mac ? "" : "+");
}
