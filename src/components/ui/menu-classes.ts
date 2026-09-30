// Class strings shared by the context menu and the dropdown menu, so every
// menu in Blink looks the same. The layout follows VS Code: a 24px row, a
// check gutter on every item, the shortcut at the right.

export const menuContent =
  'z-50 min-w-45 overflow-x-hidden overflow-y-auto rounded-lg border border-border bg-popover p-1 font-sans text-xs text-popover-foreground shadow-menu'

export const menuItem =
  "relative flex h-6 cursor-default select-none items-center gap-2 rounded-sm pr-2 pl-8 outline-hidden data-highlighted:bg-accent data-highlighted:text-foreground data-disabled:pointer-events-none data-disabled:opacity-40 data-[variant=destructive]:text-destructive pointer-coarse:h-8 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3"

export const menuSubTrigger = `${menuItem} data-[state=open]:bg-accent`

export const menuIndicator =
  'pointer-events-none absolute left-2.5 flex size-3 items-center justify-center'

export const menuSeparator = '-mx-1 my-1 h-px bg-border'

export const menuLabel = 'pr-2 pl-8 py-1 text-[11px] text-muted-foreground select-none'

export const menuShortcut = 'ml-auto pl-4 text-muted-foreground'
