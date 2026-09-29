<script setup lang="ts">
import type { ContextMenuTriggerProps } from "reka-ui";
import { ContextMenuTrigger, useForwardProps } from "reka-ui";
import { isContextMenuKey, openContextMenuAt } from "@/lib/menu-keyboard";

const props = defineProps<ContextMenuTriggerProps>();

const forwardedProps = useForwardProps(props);

function onKeydown(event: KeyboardEvent) {
  if (!isContextMenuKey(event) || !(event.target instanceof Element)) return;
  // A text field keeps the native text menu; right-click does nothing there
  // either, because those fields stop the contextmenu event.
  if (
    event.target.closest(
      "input, textarea, [contenteditable='true'], .cm-editor",
    )
  )
    return;
  event.preventDefault();
  // A nested trigger handles the key first; the outer one must not open too.
  event.stopPropagation();
  openContextMenuAt(event.target);
}
</script>

<template>
  <ContextMenuTrigger
    data-slot="context-menu-trigger"
    v-bind="forwardedProps"
    @keydown="onKeydown"
  >
    <slot />
  </ContextMenuTrigger>
</template>
